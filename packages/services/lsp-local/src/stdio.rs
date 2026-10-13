use futures_channel::mpsc;
use moonkale_lsp::LspTransport;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Mutex;

pub struct StdioTransport {
    child: Mutex<Child>,
    stdin: Mutex<ChildStdin>,
    incoming: Option<mpsc::UnboundedReceiver<String>>,
}

impl StdioTransport {
    pub fn spawn(program: &str, args: &[&str], cwd: &str) -> Result<Self, String> {
        let mut child = Command::new(program)
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("spawn {program}: {e}"))?;
        let stdin = child.stdin.take().ok_or("no stdin")?;
        let stdout = child.stdout.take().ok_or("no stdout")?;
        let (tx, rx) = mpsc::unbounded::<String>();
        std::thread::Builder::new()
            .name("moonkale-lsp-reader".into())
            .spawn(move || {
                let mut reader = BufReader::new(stdout);
                loop {
                    // Headers until an empty line; we only need Content-Length.
                    let mut len: Option<usize> = None;
                    loop {
                        let mut line = String::new();
                        match reader.read_line(&mut line) {
                            Ok(0) | Err(_) => return,
                            Ok(_) => {}
                        }
                        let line = line.trim_end();
                        if line.is_empty() {
                            break;
                        }
                        if let Some(v) = line.strip_prefix("Content-Length:") {
                            len = v.trim().parse().ok();
                        }
                    }
                    let Some(len) = len else { return };
                    let mut body = vec![0u8; len];
                    if reader.read_exact(&mut body).is_err() {
                        return;
                    }
                    if tx
                        .unbounded_send(String::from_utf8_lossy(&body).into_owned())
                        .is_err()
                    {
                        return;
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            child: Mutex::new(child),
            stdin: Mutex::new(stdin),
            incoming: Some(rx),
        })
    }
}

impl LspTransport for StdioTransport {
    fn send(&self, message: String) {
        if let Ok(mut w) = self.stdin.lock() {
            let _ = write!(w, "Content-Length: {}\r\n\r\n{}", message.len(), message);
            let _ = w.flush();
        }
    }
    fn close(&self) -> moonkale_lsp::LspCloseFuture {
        if let Ok(mut child) = self.child.lock() {
            if matches!(child.try_wait(), Ok(None)) {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
        Box::pin(async {})
    }
    fn take_incoming(&mut self) -> Option<mpsc::UnboundedReceiver<String>> {
        self.incoming.take()
    }
}

impl Drop for StdioTransport {
    fn drop(&mut self) {
        if let Ok(mut c) = self.child.lock() {
            if matches!(c.try_wait(), Ok(None)) {
                let _ = c.kill();
                let _ = c.wait();
            }
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use futures_util::StreamExt;
    #[tokio::test]
    async fn explicit_close_reaps_the_child_even_while_transport_is_retained() {
        let mut transport =
            StdioTransport::spawn("cat", &[], std::env::temp_dir().to_str().unwrap()).unwrap();
        let mut incoming = transport.take_incoming().unwrap();
        assert!(transport
            .child
            .lock()
            .unwrap()
            .try_wait()
            .unwrap()
            .is_none());
        transport.close().await;
        assert!(transport
            .child
            .lock()
            .unwrap()
            .try_wait()
            .unwrap()
            .is_some());
        assert!(
            tokio::time::timeout(std::time::Duration::from_secs(2), incoming.next())
                .await
                .unwrap()
                .is_none()
        );
        transport.close().await; // Idempotent; never signal a recycled PID.
    }
}
