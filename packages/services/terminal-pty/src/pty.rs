use futures_channel::mpsc;
use moonkale_terminal::{Output, TerminalBackend};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use std::io::{Read, Write};
use std::sync::Mutex;

pub struct PtyBackend {
    master: Mutex<Box<dyn MasterPty + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
    child: Mutex<Box<dyn Child + Send + Sync>>,
    output: Option<Output>,
    title: String,
}

impl PtyBackend {
    /// Start the login shell (`$SHELL`, else `/bin/sh`) — or `program` when
    /// given — in `cwd`, with a `cols`×`rows` terminal.
    pub fn spawn(
        cwd: Option<&str>,
        program: Option<&str>,
        cols: u16,
        rows: u16,
    ) -> Result<Self, String> {
        Self::spawn_args(cwd, program, &[], cols, rows)
    }

    /// `spawn` with arguments for `program` (Milestone 11: `ssh host …`
    /// under a PTY so its prompts work).
    pub fn spawn_args(
        cwd: Option<&str>,
        program: Option<&str>,
        args: &[String],
        cols: u16,
        rows: u16,
    ) -> Result<Self, String> {
        Self::spawn_with_env(cwd, program, args, &[], cols, rows)
    }

    /// `spawn_args` plus environment variables for the child (`SSH_AUTH_SOCK=0
    /// ssh …`, the way a line typed in a shell would set them).
    pub fn spawn_with_env(
        cwd: Option<&str>,
        program: Option<&str>,
        args: &[String],
        env: &[(String, String)],
        cols: u16,
        rows: u16,
    ) -> Result<Self, String> {
        let pty = native_pty_system();
        let pair = pty
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("openpty: {e}"))?;
        let shell = program
            .map(str::to_string)
            .or_else(|| std::env::var("SHELL").ok())
            .unwrap_or_else(|| "/bin/sh".into());
        let mut cmd = CommandBuilder::new(&shell);
        for a in args {
            cmd.arg(a);
        }
        if let Some(dir) = cwd {
            cmd.cwd(dir);
        }
        cmd.env("TERM", "xterm-256color");
        cmd.env("MOONKALE", "1");
        for (k, v) in env {
            cmd.env(k, v);
        }
        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| format!("spawn {shell}: {e}"))?;
        drop(pair.slave);
        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| format!("reader: {e}"))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|e| format!("writer: {e}"))?;
        let (tx, rx) = mpsc::unbounded::<Vec<u8>>();
        std::thread::Builder::new()
            .name("moonkale-pty-reader".into())
            .spawn(move || {
                let mut buf = [0u8; 8192];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if tx.unbounded_send(buf[..n].to_vec()).is_err() {
                                break;
                            }
                        }
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        let title = std::path::Path::new(&shell)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or(shell.clone());
        Ok(Self {
            master: Mutex::new(pair.master),
            writer: Mutex::new(writer),
            child: Mutex::new(child),
            output: Some(rx),
            title,
        })
    }

    pub fn is_running(&self) -> bool {
        matches!(self.child.lock().unwrap().try_wait(), Ok(None))
    }

    /// End the child now (a remote session being closed).
    pub fn kill(&self) {
        if let Ok(mut c) = self.child.lock() {
            let _ = c.kill();
        }
    }
}

impl TerminalBackend for PtyBackend {
    fn write(&self, data: &[u8]) {
        if let Ok(mut w) = self.writer.lock() {
            let _ = w.write_all(data);
            let _ = w.flush();
        }
    }

    fn resize(&self, cols: u16, rows: u16) {
        if let Ok(m) = self.master.lock() {
            let _ = m.resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            });
        }
    }

    fn take_output(&mut self) -> Option<Output> {
        self.output.take()
    }

    fn title(&self) -> String {
        self.title.clone()
    }
}

impl Drop for PtyBackend {
    fn drop(&mut self) {
        if let Ok(mut c) = self.child.lock() {
            let _ = c.kill();
        }
    }
}
