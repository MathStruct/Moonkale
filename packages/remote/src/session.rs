//! The SSH session: one control-master `ssh` under a PTY, the remote script,
//! the upload over the multiplexed connection, and the handshake with the
//! server it starts.

use crate::tee::TeeBackend;
use futures_util::StreamExt;
use moonkale_terminal::TerminalBackend;
use moonkale_terminal_pty::PtyBackend;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Where to go: `host` is anything `ssh` accepts as its destination
/// (`box`, `me@10.0.0.2`, a `~/.ssh/config` alias, `ssh://me@host:443`),
/// `options` go on the command line before it (`-p 443`, `-i ~/.ssh/key`,
/// `-J jump`, `-v`), `env` is set for the `ssh` process (`SSH_AUTH_SOCK=0`),
/// `path` is the folder on that machine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SshTarget {
    pub host: String,
    pub options: Vec<String>,
    pub env: Vec<(String, String)>,
    pub path: String,
}

impl SshTarget {
    /// Parse what a person types after `ssh` in a shell — leading `VAR=value`
    /// words become environment, everything up to the last word is options,
    /// the last word is the destination:
    /// `SSH_AUTH_SOCK=0 -p 443 -v daniel@192.168.178.62`,
    /// `-i ~/.ssh/MathStruct daniel@dtrmblog.de`, `build-box`.
    pub fn parse(spec: &str, path: &str) -> Result<Self, String> {
        let words: Vec<&str> = spec.split_whitespace().collect();
        let Some((host, rest)) = words.split_last() else {
            return Err("no host".into());
        };
        if host.starts_with('-') {
            return Err(format!(
                "the last word must be the host, not the option {host}"
            ));
        }
        // `ssh -p 443` — the last word is an option's argument, not a host.
        const WITH_ARG: [&str; 22] = [
            "-B", "-b", "-c", "-D", "-E", "-e", "-F", "-I", "-i", "-J", "-L", "-l", "-m", "-O",
            "-o", "-P", "-p", "-Q", "-R", "-S", "-W", "-w",
        ];
        if rest.last().is_some_and(|o| WITH_ARG.contains(o)) {
            return Err(format!(
                "{} {host} needs a host after it",
                rest[rest.len() - 1]
            ));
        }
        let mut env = Vec::new();
        let mut options = Vec::new();
        let mut in_options = false;
        for w in rest {
            match w.split_once('=') {
                Some((k, v))
                    if !in_options
                        && !k.is_empty()
                        && k.chars()
                            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_') =>
                {
                    env.push((k.to_string(), v.to_string()));
                }
                _ => {
                    in_options = true;
                    options.push(w.to_string());
                }
            }
        }
        Ok(Self {
            host: host.to_string(),
            options,
            env,
            path: path.trim().to_string(),
        })
    }

    pub fn label(&self) -> String {
        format!("{}:{}", self.host, self.path)
    }

    /// The whole spec back as one line (what the dialog shows again).
    pub fn spec(&self) -> String {
        let mut words: Vec<String> = self.env.iter().map(|(k, v)| format!("{k}={v}")).collect();
        words.extend(self.options.iter().cloned());
        words.push(self.host.clone());
        words.join(" ")
    }
}

/// What the session is doing, for the status bar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Phase {
    Connecting,
    /// `ssh` printed something that looks like a prompt: the user answers
    /// in the terminal tab.
    Prompt(String),
    Uploading,
    Starting,
    Ready {
        url: String,
    },
    Failed(String),
    Closed,
}

impl Phase {
    pub fn is_final(&self) -> bool {
        matches!(self, Phase::Ready { .. } | Phase::Failed(_) | Phase::Closed)
    }
}

/// The version string a remote server must match (`moonkale-server 0.1.0`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Marker lines the remote script prints; parsed from the PTY stream.
const NEED_UPLOAD: &str = "MOONKALE_NEED_UPLOAD";
const ASK_TOKEN: &str = "MOONKALE_TOKEN?";
const STARTING: &str = "MOONKALE_STARTING";
const REMOTE_DIR: &str = "~/.local/share/moonkale/server";

/// A live session; dropping it ends everything on both machines.
pub struct SshSession {
    pub target: SshTarget,
    pub phase: Arc<Mutex<Phase>>,
    pub local_port: u16,
    master: Arc<Mutex<Option<Arc<PtyBackend>>>>,
    control: PathBuf,
}

impl SshSession {
    /// Start a session. Returns immediately with the tee backend (a
    /// terminal tab for the `ssh` output and prompts) and the session; the
    /// session runs the handshake on a thread of its own and updates `phase`.
    /// `server_binary` is the file to upload when the host has none.
    pub fn open(
        target: SshTarget,
        server_binary: Option<PathBuf>,
        on_phase: impl Fn(Phase) + Send + Sync + 'static,
    ) -> Result<(Self, TeeBackend), String> {
        let local_port = free_port()?;
        // A random port on the host **below** the ephemeral range (Linux
        // 32768–60999, macOS 49152+): a busy host has thousands of TIME_WAIT
        // sockets up there and a bind to one of them fails (P-105 — 3 of 5
        // sessions failed while host and desktop were the same machine).
        // A real listener on the chosen port is the remaining, rare case;
        // then the server fails to bind and the terminal tab shows why.
        let remote_port = {
            let mut b = [0u8; 2];
            getrandom::fill(&mut b).expect("OS randomness");
            20_000 + (u16::from_le_bytes(b) % 12_000)
        };
        let dir = std::env::temp_dir().join(format!("moonkale-ssh-{}", std::process::id()));
        std::fs::create_dir_all(&dir).map_err(|e| format!("temp dir: {e}"))?;
        let control = dir.join(format!("cm-{local_port}"));
        let script = remote_script(&target.path, remote_port);
        let mut args: Vec<String> = vec![
            // A remote PTY: the script's `stty -echo` then really hides the
            // token, ssh puts our PTY in raw mode (no local echo), and the
            // remote server gets SIGHUP when the session ends (P-104).
            "-t".into(),
            "-M".into(),
            "-S".into(),
            control.to_string_lossy().into_owned(),
            "-o".into(),
            "ControlPersist=no".into(),
            "-o".into(),
            "ExitOnForwardFailure=yes".into(),
            "-o".into(),
            "ServerAliveInterval=15".into(),
            "-L".into(),
            format!("127.0.0.1:{local_port}:127.0.0.1:{remote_port}"),
        ];
        args.extend(target.options.iter().cloned());
        args.extend([
            target.host.clone(),
            "--".into(),
            wrap_for_any_shell(&script),
        ]);
        tracing::info!("remote: ssh {}", args.join(" "));
        let mut pty = PtyBackend::spawn_with_env(None, Some("ssh"), &args, &target.env, 100, 30)?;
        let output = pty.take_output();
        let pty = Arc::new(pty);
        let handle = PtyHandle {
            pty: pty.clone(),
            output,
        };
        let (tee, mut watch) = TeeBackend::new(Box::new(handle), &format!("ssh {}", target.host));
        let notice = tee.notices();
        let phase = Arc::new(Mutex::new(Phase::Connecting));
        let session = Self {
            target: target.clone(),
            phase: phase.clone(),
            local_port,
            master: Arc::new(Mutex::new(Some(pty.clone()))),
            control: control.clone(),
        };
        let token = api::client::session_token();
        let writer = tee.writer();
        let pty_alive = pty.clone();
        let set = {
            let phase = phase.clone();
            let on_phase = Arc::new(on_phase);
            let master = pty.clone();
            move |p: Phase| {
                // A failure is written into the terminal tab too (the status
                // bar line is easy to miss) and ends the master `ssh`, so the
                // host's script stops waiting (P-106).
                if let Phase::Failed(reason) = &p {
                    let _ = notice.unbounded_send(
                        format!("\r\n[moonkale] remote session failed: {reason}\r\n").into_bytes(),
                    );
                    master.kill();
                }
                *phase.lock().unwrap() = p.clone();
                on_phase(p);
            }
        };
        let handshake = async move {
            let mut line = String::new();
            let mut uploaded = false;
            let deadline = tokio::time::Instant::now() + Duration::from_secs(300);
            loop {
                let chunk = match tokio::time::timeout_at(deadline, watch.next()).await {
                    Ok(Some(c)) => c,
                    Ok(None) => {
                        set(Phase::Failed("ssh ended before the server started".into()));
                        return;
                    }
                    Err(_) => {
                        set(Phase::Failed(
                            "timed out waiting for the remote server".into(),
                        ));
                        return;
                    }
                };
                for b in chunk {
                    if b == b'\n' || b == b'\r' {
                        let l = strip_ansi(line.trim());
                        line.clear();
                        if l.is_empty() {
                            continue;
                        }
                        if let Some(arch) = l.strip_prefix(NEED_UPLOAD) {
                            let arch = arch.trim().to_string();
                            if uploaded {
                                set(Phase::Failed(format!(
                                    "the uploaded server did not run on {arch}"
                                )));
                                return;
                            }
                            set(Phase::Uploading);
                            match server_binary.clone() {
                                Some(bin) => {
                                    if let Err(e) = upload(&control, &target, &bin, &arch).await {
                                        set(Phase::Failed(e));
                                        return;
                                    }
                                    uploaded = true;
                                }
                                None => {
                                    set(Phase::Failed(format!(
                                        "the host has no moonkale-server ({arch}) and this machine has none to upload — build one with `cd packages/web && dx build --platform server --release`, or point MOONKALE_SERVER_BINARY at it"
                                    )));
                                    return;
                                }
                            }
                        } else if l == "MOONKALE_UPLOAD_TIMEOUT" {
                            set(Phase::Failed(
                                "the host waited 10 minutes for the server upload".into(),
                            ));
                            return;
                        } else if l == ASK_TOKEN {
                            set(Phase::Starting);
                            if let Ok(w) = writer.lock() {
                                w.write(format!("{token}\n").as_bytes());
                            }
                        } else if l == STARTING {
                            let url = format!("http://127.0.0.1:{local_port}");
                            match wait_for_server(&url, &token, &pty_alive).await {
                                Ok(()) => {
                                    api::client::connect(&url, Some(&token), &target.label());
                                    set(Phase::Ready { url });
                                    return;
                                }
                                Err(e) => {
                                    set(Phase::Failed(e));
                                    return;
                                }
                            }
                        } else if looks_like_prompt(&l) {
                            set(Phase::Prompt(l.clone()));
                        }
                    } else {
                        line.push(b as char);
                        // Prompts end without a newline: check the partial line too.
                        let partial = strip_ansi(line.trim());
                        if looks_like_prompt(&partial) {
                            set(Phase::Prompt(partial));
                        }
                    }
                }
            }
        };
        // Always a thread of its own with its own runtime: the desktop's
        // tokio runtime is only polled when the event loop wakes, and the
        // handshake stalled there after the server's first answer (P-098).
        std::thread::Builder::new()
            .name("moonkale-remote-handshake".into())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("tokio");
                rt.block_on(handshake);
            })
            .map_err(|e| e.to_string())?;
        Ok((session, tee))
    }

    /// End the session: the master `ssh` dies, the forward and the remote
    /// server with it; the desktop goes back to local sources.
    pub fn close(&self) {
        if let Some(pty) = self.master.lock().unwrap().take() {
            pty.kill();
        }
        let _ = std::fs::remove_file(&self.control);
        api::client::disconnect();
        *self.phase.lock().unwrap() = Phase::Closed;
    }

    pub fn phase(&self) -> Phase {
        self.phase.lock().unwrap().clone()
    }
}

impl Drop for SshSession {
    fn drop(&mut self) {
        self.close();
    }
}

/// `Arc<PtyBackend>` as a backend: the tee owns this box, the session keeps
/// another handle to kill the child. The output stream is taken before the
/// `Arc` is made (`take_output` needs `&mut`).
struct PtyHandle {
    pty: Arc<PtyBackend>,
    output: Option<moonkale_terminal::Output>,
}
impl TerminalBackend for PtyHandle {
    fn write(&self, data: &[u8]) {
        self.pty.write(data)
    }
    fn resize(&self, cols: u16, rows: u16) {
        self.pty.resize(cols, rows)
    }
    fn take_output(&mut self) -> Option<moonkale_terminal::Output> {
        self.output.take()
    }
    fn title(&self) -> String {
        self.pty.title()
    }
}

/// The script `ssh` runs on the host: report the machine, wait for the
/// binary if it is missing, take the token with echo off, start the server.
fn remote_script(path: &str, remote_port: u16) -> String {
    let dir = format!("{REMOTE_DIR}/{VERSION}");
    format!(
        r#"set -e
d={dir}
mkdir -p "$d"
if [ ! -x "$d/moonkale-server" ]; then
  echo "{NEED_UPLOAD} $(uname -m)"
  i=0
  while [ ! -f "$d/moonkale-server.ready" ]; do
    sleep 0.5; i=$((i+1)); if [ $i -gt 1200 ]; then echo "MOONKALE_UPLOAD_TIMEOUT"; exit 3; fi
  done
  rm -f "$d/moonkale-server.ready"
fi
stty -echo 2>/dev/null || true
echo "{ASK_TOKEN}"
read TOKEN
stty echo 2>/dev/null || true
echo "{STARTING}"
mkdir -p "$d/public"
cd "$d"
printf '%s\n' "$TOKEN" | MOONKALE_ROOT={path} exec "$d/moonkale-server" --port {remote_port} --bind 127.0.0.1 --root {path} --token-stdin
"#,
        path = shell_quote(path)
    )
}

/// Stream `bin` into the host's server directory over the control socket
/// (no second authentication), mark it executable, and set the ready flag
/// the script waits for.
async fn upload(control: &Path, target: &SshTarget, bin: &Path, arch: &str) -> Result<(), String> {
    let host = &target.host;
    let dir = format!("{REMOTE_DIR}/{VERSION}");
    tracing::info!(
        "remote: uploading {} for {arch} to {host}:{dir}",
        bin.display()
    );
    let mut file = tokio::fs::File::open(bin)
        .await
        .map_err(|e| format!("open {}: {e}", bin.display()))?;
    let remote_cmd = format!(
        "d={dir}; cat > \"$d/moonkale-server.tmp\" && chmod +x \"$d/moonkale-server.tmp\" && mv \"$d/moonkale-server.tmp\" \"$d/moonkale-server\" && touch \"$d/moonkale-server.ready\""
    );
    let mut child = tokio::process::Command::new("ssh")
        .arg("-S")
        .arg(control)
        .arg("-o")
        .arg("BatchMode=yes")
        .args(&target.options)
        .envs(target.env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .arg(host)
        .arg("--")
        .arg(wrap_for_any_shell(&remote_cmd))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("ssh (upload): {e}"))?;
    {
        use tokio::io::AsyncWriteExt;
        let mut stdin = child.stdin.take().ok_or("upload stdin")?;
        tokio::io::copy(&mut file, &mut stdin)
            .await
            .map_err(|e| format!("upload: {e}"))?;
        stdin.shutdown().await.map_err(|e| format!("upload: {e}"))?;
    }
    let out = child
        .wait_with_output()
        .await
        .map_err(|e| format!("upload: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "upload failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(())
}

/// Poll the forwarded port until the server answers a token-checked
/// request (`/api/sources/list` → 200), or give up after 60 s.
async fn wait_for_server(url: &str, token: &str, ssh: &PtyBackend) -> Result<(), String> {
    let client = reqwest_client();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    loop {
        if !ssh.is_running() {
            return Err("the remote server exited (see the ssh terminal)".into());
        }
        let r = client
            .post(format!("{url}/api/sources/list"))
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body("{}")
            .send()
            .await;
        match r {
            Ok(resp) if resp.status().is_success() => return Ok(()),
            Ok(resp) if resp.status().as_u16() == 401 => {
                return Err("the remote server rejected the session token".into())
            }
            _ => {}
        }
        if tokio::time::Instant::now() > deadline {
            return Err("the remote server did not answer on the forwarded port".into());
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
}

fn reqwest_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

/// The server binary to upload: `MOONKALE_SERVER_BINARY`, else
/// `moonkale-server` next to this executable, else the workspace's dev
/// build (`target/dx/web/{release,debug}/web/server`).
pub fn server_binary() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("MOONKALE_SERVER_BINARY") {
        let p = PathBuf::from(p);
        return p.is_file().then_some(p);
    }
    let mut candidates = Vec::new();
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf));
    if let Some(dir) = &exe_dir {
        candidates.push(dir.join("moonkale-server"));
    }
    // The dev build: walk up from the executable (dx runs it from
    // `target/dx/<app>/debug/linux/app`, P-106) and from the cwd to the
    // workspace root (the directory with a `[workspace]` Cargo.toml).
    let mut starts: Vec<PathBuf> = exe_dir.into_iter().collect();
    if let Ok(cwd) = std::env::current_dir() {
        starts.push(cwd);
    }
    for start in starts {
        let mut dir = Some(start.as_path());
        while let Some(d) = dir {
            let manifest = d.join("Cargo.toml");
            if std::fs::read_to_string(&manifest)
                .map(|t| t.contains("[workspace]"))
                .unwrap_or(false)
            {
                candidates.push(d.join("target/dx/web/release/web/server"));
                candidates.push(d.join("target/dx/web/debug/web/server"));
                break;
            }
            dir = d.parent();
        }
    }
    candidates.into_iter().find(|p| p.is_file())
}

fn free_port() -> Result<u16, String> {
    std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .map_err(|e| format!("no free local port: {e}"))
}

/// The remote command as the host's **login shell** will see it — and that
/// shell may be bash, zsh, fish or nushell, each with its own quoting. The
/// only argument they all pass through unchanged is a single-quoted string
/// without quotes or backslashes inside, so the script travels as base64:
/// `sh -c 'eval "$(echo <b64> | base64 -d)"'` — `sh` does the decoding and
/// runs the script with the PTY still on stdin (a pipe into `sh` would
/// swallow the token prompt). P-103.
fn wrap_for_any_shell(script: &str) -> String {
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(script.as_bytes());
    format!("sh -c 'eval \"$(echo {b64} | base64 -d)\"'")
}

/// POSIX single-quote quoting (inside scripts that `sh` runs).
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            // CSI … final byte in @..~
            if chars.peek() == Some(&'[') {
                chars.next();
                for n in chars.by_ref() {
                    if ('@'..='~').contains(&n) {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

/// Does this line want an answer from the user? `ssh`'s own prompts.
pub fn looks_like_prompt(line: &str) -> bool {
    let l = line.to_ascii_lowercase();
    l.ends_with("password:")
        || l.ends_with("passphrase:")
        || l.contains("enter passphrase for")
        || l.contains("(yes/no")
        || l.contains("are you sure you want to continue connecting")
        || l.ends_with("verification code:")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompts_and_quoting() {
        assert!(looks_like_prompt("daniel@box's password:"));
        assert!(looks_like_prompt(
            "Enter passphrase for key '/home/d/.ssh/id':"
        ));
        assert!(looks_like_prompt(
            "Are you sure you want to continue connecting (yes/no/[fingerprint])?"
        ));
        assert!(!looks_like_prompt("MOONKALE_STARTING"));
        assert_eq!(shell_quote("a'b"), "'a'\\''b'");
        let w = wrap_for_any_shell("echo 'hi' \"$x\"");
        assert!(w.starts_with("sh -c 'eval \"$(echo ") && w.ends_with(" | base64 -d)\"'"));
        assert!(
            !w[6..].contains('\'') || w.matches('\'').count() == 2,
            "no quotes inside: {w}"
        );
        assert_eq!(strip_ansi("\u{1b}[32mok\u{1b}[0m"), "ok");
        let t = SshTarget::parse(
            "SSH_AUTH_SOCK=0 -p 443 -v daniel@192.168.178.62",
            "/home/daniel/Code",
        )
        .unwrap();
        assert_eq!(t.host, "daniel@192.168.178.62");
        assert_eq!(t.options, ["-p", "443", "-v"]);
        assert_eq!(t.env, [("SSH_AUTH_SOCK".to_string(), "0".to_string())]);
        assert_eq!(t.spec(), "SSH_AUTH_SOCK=0 -p 443 -v daniel@192.168.178.62");
        let t = SshTarget::parse("-i ~/.ssh/MathStruct daniel@dtrmblog.de", "/srv").unwrap();
        assert_eq!(t.options, ["-i", "~/.ssh/MathStruct"]);
        assert!(t.env.is_empty());
        // `-o Foo=bar` after an option is an option, not environment.
        let t = SshTarget::parse("-o IdentityAgent=none box", "/").unwrap();
        assert_eq!(t.options, ["-o", "IdentityAgent=none"]);
        assert!(SshTarget::parse("-p 443", "/").is_err());
        assert!(SshTarget::parse("", "/").is_err());
        let s = remote_script("/srv/code", 41234);
        assert!(s.contains("--token-stdin"));
        assert!(s.contains(NEED_UPLOAD));
        assert!(s.contains("MOONKALE_ROOT='/srv/code'"));
    }
}
