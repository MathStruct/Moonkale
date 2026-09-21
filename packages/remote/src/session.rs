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

/// Where to go: `host` is anything `ssh` accepts (`box`, `me@10.0.0.2`, a
/// `~/.ssh/config` alias), `path` the folder on that machine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SshTarget {
    pub host: String,
    pub path: String,
}

impl SshTarget {
    pub fn label(&self) -> String {
        format!("{}:{}", self.host, self.path)
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
    /// session runs the handshake on the tokio runtime and updates `phase`.
    /// `server_binary` is the file to upload when the host has none.
    pub fn open(
        target: SshTarget,
        server_binary: Option<PathBuf>,
        on_phase: impl Fn(Phase) + Send + Sync + 'static,
    ) -> Result<(Self, TeeBackend), String> {
        let local_port = free_port()?;
        let remote_port = 40_000 + (local_port % 20_000);
        let dir = std::env::temp_dir().join(format!("moonkale-ssh-{}", std::process::id()));
        std::fs::create_dir_all(&dir).map_err(|e| format!("temp dir: {e}"))?;
        let control = dir.join(format!("cm-{local_port}"));
        let script = remote_script(&target.path, remote_port);
        let args: Vec<String> = vec![
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
            target.host.clone(),
            "--".into(),
            "sh".into(),
            "-c".into(),
            shell_quote(&script),
        ];
        tracing::info!("remote: ssh {}", args.join(" "));
        let mut pty = PtyBackend::spawn_args(None, Some("ssh"), &args, 100, 30)?;
        let output = pty.take_output();
        let pty = Arc::new(pty);
        let handle = PtyHandle {
            pty: pty.clone(),
            output,
        };
        let (tee, mut watch) = TeeBackend::new(Box::new(handle), &format!("ssh {}", target.host));
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
            move |p: Phase| {
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
                                    if let Err(e) =
                                        upload(&control, &target.host, &bin, &arch).await
                                    {
                                        set(Phase::Failed(e));
                                        return;
                                    }
                                    uploaded = true;
                                }
                                None => {
                                    set(Phase::Failed(format!(
                                        "the host has no moonkale-server ({arch}) and no binary to upload was found (MOONKALE_SERVER_BINARY)"
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
        match tokio::runtime::Handle::try_current() {
            Ok(h) => {
                h.spawn(handshake);
            }
            Err(_) => {
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
            }
        }
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
async fn upload(control: &Path, host: &str, bin: &Path, arch: &str) -> Result<(), String> {
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
        .arg(host)
        .arg("--")
        .arg("sh")
        .arg("-c")
        .arg(shell_quote(&remote_cmd))
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
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("moonkale-server"));
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        for root in [cwd.clone(), cwd.join(".."), cwd.join("../..")] {
            candidates.push(root.join("target/dx/web/release/web/server"));
            candidates.push(root.join("target/dx/web/debug/web/server"));
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

/// POSIX single-quote quoting.
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
        assert_eq!(strip_ansi("\u{1b}[32mok\u{1b}[0m"), "ok");
        let s = remote_script("/srv/code", 41234);
        assert!(s.contains("--token-stdin"));
        assert!(s.contains(NEED_UPLOAD));
        assert!(s.contains("MOONKALE_ROOT='/srv/code'"));
    }
}
