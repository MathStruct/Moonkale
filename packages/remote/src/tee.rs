//! A terminal backend that shows the `ssh` session in the terminal panel
//! *and* lets the session machinery read the same bytes (prompt detection,
//! the script's markers).

use futures_channel::mpsc;
use moonkale_terminal::{Output, TerminalBackend};
use std::sync::{Arc, Mutex};

pub struct TeeBackend {
    inner: Arc<Mutex<Box<dyn TerminalBackend + Send>>>,
    panel_out: Option<Output>,
    title: String,
}

impl TeeBackend {
    /// Wrap `inner`; returns the tee (for the terminal panel) and a second
    /// receiver of the same output (for the session).
    pub fn new(mut inner: Box<dyn TerminalBackend + Send>, title: &str) -> (Self, Output) {
        let source = inner.take_output();
        let (panel_tx, panel_rx) = mpsc::unbounded::<Vec<u8>>();
        let (watch_tx, watch_rx) = mpsc::unbounded::<Vec<u8>>();
        if let Some(mut src) = source {
            std::thread::Builder::new()
                .name("moonkale-remote-tee".into())
                .spawn(move || {
                    use futures_util::StreamExt;
                    futures_executor::block_on(async move {
                        while let Some(chunk) = src.next().await {
                            let _ = panel_tx.unbounded_send(chunk.clone());
                            let _ = watch_tx.unbounded_send(chunk);
                        }
                    });
                })
                .ok();
        }
        (
            Self {
                inner: Arc::new(Mutex::new(inner)),
                panel_out: Some(panel_rx),
                title: title.to_string(),
            },
            watch_rx,
        )
    }

    /// A handle that writes to the same PTY (the session answers the
    /// script's token prompt with it).
    pub fn writer(&self) -> Arc<Mutex<Box<dyn TerminalBackend + Send>>> {
        self.inner.clone()
    }
}

impl TerminalBackend for TeeBackend {
    fn write(&self, data: &[u8]) {
        if let Ok(i) = self.inner.lock() {
            i.write(data);
        }
    }
    fn resize(&self, cols: u16, rows: u16) {
        if let Ok(i) = self.inner.lock() {
            i.resize(cols, rows);
        }
    }
    fn take_output(&mut self) -> Option<Output> {
        self.panel_out.take()
    }
    fn title(&self) -> String {
        self.title.clone()
    }
}
