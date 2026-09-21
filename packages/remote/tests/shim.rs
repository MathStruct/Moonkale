//! The whole session against a fake `ssh` on `PATH`: the shim runs the
//! remote script *locally* under a scratch `HOME`, turns the `-L` forward
//! into "bind the local port directly", and takes the upload on stdin like
//! the multiplexed `ssh -S` call would. Needs the server binary
//! (`cd packages/web && dx build --platform server [--release]`), so it is
//! ignored by default:
//!
//! `cargo test -p moonkale-remote --test shim -- --ignored --nocapture`

use moonkale_core::{NodeKind, Query, SourceFamily};
use moonkale_remote::{server_binary, Phase, SshSession, SshTarget};
use std::sync::mpsc;
use std::time::Duration;

const SHIM: &str = r#"#!/bin/sh
# fake ssh: run the remote command here, under a scratch HOME
export HOME=__HOME__
# The target's environment and options must reach every ssh call.
[ "$SSH_AUTH_SOCK" = 0 ] || { echo "SSH_AUTH_SOCK not set" >&2; exit 9; }
fwd=""; port=""
while [ $# -gt 0 ]; do
  case "$1" in
    -p) port="$2"; shift 2 ;;
    -L) fwd="$2"; shift 2 ;;
    -S|-o) shift 2 ;;
    -M) shift ;;
    --) shift; break ;;
    *) shift ;;
  esac
done
[ "$port" = 443 ] || { echo "-p 443 missing" >&2; exit 8; }
cmd="$*"
if [ -n "$fwd" ]; then
  L=$(printf '%s' "$fwd" | cut -d: -f2); R=$(printf '%s' "$fwd" | cut -d: -f4)
  cmd=$(printf '%s' "$cmd" | sed "s/--port $R/--port $L/")
fi
eval "exec $cmd"
"#;

#[test]
#[ignore]
fn fake_ssh_session_reaches_ready() {
    let Some(bin) = server_binary() else {
        eprintln!("no server binary (dx build --platform server); skipping");
        return;
    };
    let scratch = std::env::temp_dir().join(format!("moonkale-remote-shim-{}", std::process::id()));
    let home = scratch.join("home");
    let bindir = scratch.join("bin");
    let root = scratch.join("project");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&bindir).unwrap();
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("Hello.md"), "# Hello\n\nfrom the other side\n").unwrap();
    let shim = bindir.join("ssh");
    std::fs::write(&shim, SHIM.replace("__HOME__", &home.to_string_lossy())).unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let path = format!(
        "{}:{}",
        bindir.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    std::env::set_var("PATH", path);
    eprintln!(
        "server binary: {} ({} MB)",
        bin.display(),
        std::fs::metadata(&bin)
            .map(|m| m.len() / 1_000_000)
            .unwrap_or(0)
    );

    // The desktop installs this before launch; server functions go through it.
    dioxus::fullstack::set_server_url(api::relay::install().unwrap().leak());
    let (tx, rx) = mpsc::channel();
    let target =
        SshTarget::parse("SSH_AUTH_SOCK=0 -p 443 fake-host", &root.to_string_lossy()).unwrap();
    let (session, mut tee) = SshSession::open(target, Some(bin), move |p| {
        let _ = tx.send(p);
    })
    .expect("open");
    // Echo what the terminal tab would show.
    if let Some(mut out) = moonkale_terminal::TerminalBackend::take_output(&mut tee) {
        std::thread::spawn(move || {
            use futures_util::StreamExt;
            futures_executor::block_on(async move {
                while let Some(chunk) = out.next().await {
                    eprint!("{}", String::from_utf8_lossy(&chunk));
                }
            })
        });
    }
    let mut seen = Vec::new();
    let deadline = std::time::Instant::now() + Duration::from_secs(120);
    let ready = loop {
        match rx.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now())) {
            Ok(p) => {
                eprintln!("phase: {p:?}");
                let done = matches!(p, Phase::Ready { .. } | Phase::Failed(_));
                seen.push(p.clone());
                if done {
                    break p;
                }
            }
            Err(_) => panic!("no terminal phase within 120 s; seen {seen:?}"),
        }
    };
    let Phase::Ready { url } = ready else {
        panic!("session failed: {ready:?}");
    };
    assert!(
        seen.contains(&Phase::Uploading),
        "the shim's fresh HOME must trigger an upload"
    );
    assert!(seen.contains(&Phase::Starting));
    assert!(home
        .join(".local/share/moonkale/server")
        .join(moonkale_remote::VERSION)
        .join("moonkale-server")
        .is_file());
    assert_eq!(url, format!("http://127.0.0.1:{}", session.local_port));
    let remote = api::client::active().expect("client connected");
    assert_eq!(remote.url, url);

    // The desktop now talks to that server: open the folder and read the file.
    let rt = tokio::runtime::Runtime::new().unwrap();
    let text = rt.block_on(async {
        let sources =
            api::client::open_folder(root.to_string_lossy().into_owned(), Default::default())
                .await
                .expect("open_folder over the session");
        let folder = sources
            .iter()
            .find(|s| s.descriptor().family == SourceFamily::Folder)
            .expect("a folder source");
        let d = folder.descriptor();
        let top = folder
            .query(Query::Children(d.root))
            .await
            .expect("children");
        let file = top
            .nodes
            .iter()
            .find(|n| n.kind == NodeKind::File && n.native_key.ends_with("Hello.md"))
            .expect("Hello.md at the root");
        folder.fetch_text(file.id).await.expect("fetch_text").0
    });
    assert!(text.contains("from the other side"), "got {text:?}");

    session.close();
    assert_eq!(session.phase(), Phase::Closed);
    assert!(api::client::active().is_none());
    let _ = std::fs::remove_dir_all(&scratch);
}
