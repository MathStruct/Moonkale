//! The desktop's way to change servers at run time (Milestone 11).
//!
//! dioxus-fullstack keeps the server URL in a `OnceLock` that `launch` sets
//! once (P-098): `set_server_url` afterwards panics. So the desktop sets it
//! *before* launch to a loopback listener of its own, and this relay pipes
//! every connection on that listener to whatever [`super::client::active`]
//! names at the time — plain TCP for `http://`, TLS with the WebPKI roots
//! for `https://`. Server functions and typed websockets both go through
//! it unchanged; with no remote active, connections are refused and the
//! desktop's dispatchers never make them (they check `active()` first).

use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::{TcpListener, TcpStream};

/// Bind the relay and return its URL (`http://127.0.0.1:<port>`); the
/// caller passes it to `dioxus::fullstack::set_server_url` before launch.
/// The relay runs on a thread of its own.
pub fn install() -> Result<String, String> {
    let std_listener =
        std::net::TcpListener::bind("127.0.0.1:0").map_err(|e| format!("relay bind: {e}"))?;
    std_listener
        .set_nonblocking(true)
        .map_err(|e| format!("relay: {e}"))?;
    let addr = std_listener
        .local_addr()
        .map_err(|e| format!("relay: {e}"))?;
    std::thread::Builder::new()
        .name("moonkale-client-relay".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("relay runtime");
            let local = tokio::task::LocalSet::new();
            local.block_on(&rt, async move {
                let listener = TcpListener::from_std(std_listener).expect("relay listener");
                loop {
                    let Ok((sock, _)) = listener.accept().await else {
                        continue;
                    };
                    tokio::task::spawn_local(pipe(sock));
                }
            });
        })
        .map_err(|e| e.to_string())?;
    Ok(format!("http://{addr}"))
}

/// Where the relay currently sends: scheme, host, port.
fn target() -> Option<(bool, String, u16)> {
    let url = super::client::active()?.url;
    let uri: http::Uri = url.parse().ok()?;
    let tls = uri.scheme_str() == Some("https");
    let host = uri.host()?.to_string();
    let port = uri.port_u16().unwrap_or(if tls { 443 } else { 80 });
    Some((tls, host, port))
}

async fn pipe(mut client: TcpStream) {
    let Some((tls, host, port)) = target() else {
        tracing::warn!("relay: connection with no remote active — closed");
        return;
    };
    let upstream = match TcpStream::connect((host.as_str(), port)).await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("relay: {host}:{port}: {e}");
            return;
        }
    };
    if tls {
        let config = tokio_rustls::rustls::ClientConfig::builder()
            .with_root_certificates(tokio_rustls::rustls::RootCertStore {
                roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
            })
            .with_no_client_auth();
        let connector = tokio_rustls::TlsConnector::from(Arc::new(config));
        let name = match tokio_rustls::rustls::pki_types::ServerName::try_from(host.clone()) {
            Ok(n) => n,
            Err(e) => {
                tracing::warn!("relay: {host}: {e}");
                return;
            }
        };
        match connector.connect(name, upstream).await {
            Ok(mut s) => copy_both(&mut client, &mut s).await,
            Err(e) => tracing::warn!("relay: TLS to {host}: {e}"),
        }
    } else {
        let mut upstream = upstream;
        copy_both(&mut client, &mut upstream).await;
    }
}

async fn copy_both<A, B>(a: &mut A, b: &mut B)
where
    A: AsyncRead + AsyncWrite + Unpin,
    B: AsyncRead + AsyncWrite + Unpin,
{
    let _ = tokio::io::copy_bidirectional(a, b).await;
}

/// A tokio runtime is needed for `TcpListener::from_std`; `SocketAddr` is
/// re-exported for callers that log it.
pub type Addr = SocketAddr;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    #[test]
    fn relays_to_the_active_remote_and_refuses_without_one() {
        // A tiny upstream that answers one line.
        let up = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let up_addr = up.local_addr().unwrap();
        std::thread::spawn(move || {
            for s in up.incoming().flatten() {
                let mut s = s;
                let mut buf = [0u8; 64];
                let n = s.read(&mut buf).unwrap();
                let _ =
                    s.write_all(format!("echo:{}", String::from_utf8_lossy(&buf[..n])).as_bytes());
            }
        });
        let url = install().unwrap();
        let relay_addr: SocketAddr = url.trim_start_matches("http://").parse().unwrap();
        // No remote: the connection is closed without data.
        let mut c = std::net::TcpStream::connect(relay_addr).unwrap();
        c.set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .unwrap();
        let _ = c.write_all(b"x");
        let mut buf = Vec::new();
        let _ = c.read_to_end(&mut buf);
        assert!(buf.is_empty());
        // With a remote: bytes go through.
        super::super::client::connect(&format!("http://{up_addr}"), None, "test");
        let mut c = std::net::TcpStream::connect(relay_addr).unwrap();
        c.set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .unwrap();
        c.write_all(b"hi").unwrap();
        let mut buf = [0u8; 64];
        let n = c.read(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"echo:hi");
        super::super::client::disconnect();
    }
}
