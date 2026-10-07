//! A terminal backend that shows the `ssh` session in the terminal panel
//! *and* lets the session machinery read the same bytes (prompt detection,
//! the script's markers).

use futures_channel::mpsc;
use moonkale_terminal::{Output, TerminalBackend};
use std::sync::{Arc, Mutex};

pub struct TeeBackend {
    inner: Arc<Mutex<Box<dyn TerminalBackend + Send>>>,
    panel_out: Option<Output>,
    /// Lines the session itself adds to the tab (failure notices).
    notices: mpsc::Sender<Vec<u8>>,
    title: String,
}

impl TeeBackend {
    /// Wrap `inner`; returns the tee (for the terminal panel) and a second
    /// receiver of the same output (for the session).
    pub fn new(mut inner: Box<dyn TerminalBackend + Send>, title: &str) -> (Self, Output) {
        let source = inner.take_output();
        let (mut panel_tx, panel_rx) = moonkale_terminal::output_channel();
        let (mut watch_tx, watch_rx) = moonkale_terminal::output_channel();
        let notices = panel_tx.clone();
        if let Some(mut src) = source {
            std::thread::Builder::new()
                .name("moonkale-remote-tee".into())
                .spawn(move || {
                    use futures_util::StreamExt;
                    futures_executor::block_on(async move {
                        while let Some(chunk) = src.next().await {
                            // The tab gets everything (waiting while it lags);
                            // the session reads only the early output, so its
                            // copy is dropped when full instead of growing (#14).
                            let _ = watch_tx.try_send(chunk.clone());
                            if !moonkale_terminal::send_blocking(&mut panel_tx, chunk) {
                                break;
                            }
                        }
                    });
                })
                .ok();
        }
        (
            Self {
                inner: Arc::new(Mutex::new(inner)),
                panel_out: Some(panel_rx),
                notices,
                title: title.to_string(),
            },
            watch_rx,
        )
    }

    /// A sender whose bytes appear in the terminal tab as if the PTY had
    /// printed them (the session's own notices).
    pub fn notices(&self) -> mpsc::Sender<Vec<u8>> {
        self.notices.clone()
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
