//! Access control for the server (Milestone 7, step 6 — closes P-20 for
//! the single-user case).
//!
//! - `MOONKALE_TOKEN` set: every request except the login page, static
//!   assets and dev-tools needs the token — as the `moonkale_token`
//!   HttpOnly cookie (set by `/login`) or `Authorization: Bearer`. Server
//!   functions and websocket relays answer 401 without it; the app page
//!   redirects to `/login`.
//! - `MOONKALE_TOKEN` unset: dev mode, everything open — allowed only on a
//!   loopback bind ([`guard_bind`] refuses to start otherwise).
//! - Every `/api/*` and `/mcp` request writes one audit line.
//! - `/login` is rate-limited per client address (10 attempts / minute).
//!
//! The MCP endpoint keeps its own `MOONKALE_MCP_TOKEN` bearer check for
//! external agents; when that is set, `/mcp` is left to it.

use axum::extract::{ConnectInfo, Request};
use axum::http::{header, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::Form;
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const COOKIE: &str = "moonkale_token";

/// The configured token, if any.
pub fn token() -> Option<String> {
    std::env::var("MOONKALE_TOKEN")
        .ok()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
}

/// Refuse to serve a non-loopback address without a token. Called before
/// the router is built; prints the mode once.
pub fn guard_bind() {
    let ip: Option<IpAddr> = std::env::var("IP").ok().and_then(|s| s.parse().ok());
    let tls = tls_files().is_some();
    let insecure = std::env::var("MOONKALE_INSECURE_HTTP").is_ok_and(|v| v == "1");
    match bind_mode(ip, token().is_some(), tls, insecure) {
        Ok(msg) => eprintln!("moonkale: {msg}"),
        Err(msg) => {
            eprintln!("moonkale: {msg}");
            std::process::exit(2);
        }
    }
}

/// `MOONKALE_TLS_CERT` + `MOONKALE_TLS_KEY` (PEM files): serve HTTPS
/// (Milestone 11). Both or neither.
pub fn tls_files() -> Option<(String, String)> {
    let cert = std::env::var("MOONKALE_TLS_CERT").ok()?;
    let key = std::env::var("MOONKALE_TLS_KEY").ok()?;
    Some((cert, key))
}

/// What serving on `ip` means with or without a token and TLS: `Ok(mode)`
/// or the reason to refuse. Off loopback, a token is required (Milestone 7)
/// and so is TLS (Milestone 11) — the token would otherwise cross the
/// network in clear — unless `MOONKALE_INSECURE_HTTP=1` says a reverse
/// proxy or a VPN terminates TLS in front of us.
pub fn bind_mode(
    ip: Option<IpAddr>,
    has_token: bool,
    tls: bool,
    insecure_ok: bool,
) -> Result<String, String> {
    let loopback = ip.map(|i| i.is_loopback()).unwrap_or(true);
    let scheme = if tls { "https" } else { "http" };
    match (has_token, loopback) {
        (true, false) if !tls && !insecure_ok => Err(format!(
            "refusing to bind {} over plain HTTP — set MOONKALE_TLS_CERT/MOONKALE_TLS_KEY, or MOONKALE_INSECURE_HTTP=1 behind a TLS proxy",
            ip.map(|i| i.to_string()).unwrap_or_default()
        )),
        (true, _) => Ok(format!(
            "access token required (MOONKALE_TOKEN); log in at /login ({scheme})"
        )),
        (false, true) => Ok(format!(
            "dev mode — no MOONKALE_TOKEN, serving on loopback only ({scheme})"
        )),
        (false, false) => Err(format!(
            "refusing to bind {} without MOONKALE_TOKEN — set a token to expose the server",
            ip.map(|i| i.to_string()).unwrap_or_default()
        )),
    }
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn presented(req: &Request) -> Option<String> {
    if let Some(v) = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
    {
        if let Some(t) = v.strip_prefix("Bearer ") {
            return Some(t.trim().to_string());
        }
    }
    req.headers()
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .map(str::trim)
        .find_map(|kv| kv.strip_prefix(&format!("{COOKIE}=")).map(str::to_string))
}

fn is_upgrade(req: &Request) -> bool {
    req.headers()
        .get(header::UPGRADE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.eq_ignore_ascii_case("websocket"))
}

/// `Some(origin)` when the request carries an `Origin` whose host differs
/// from the `Host` it was sent to (scheme and port ignored: a reverse proxy
/// may terminate TLS on another port).
fn cross_origin(req: &Request) -> Option<String> {
    let origin = req
        .headers()
        .get(header::ORIGIN)?
        .to_str()
        .ok()?
        .to_string();
    let host = req
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    (!same_host(&origin, host)).then_some(origin)
}

/// Compare the host part of an origin (`https://a.b:1`) with a `Host` header (`a.b:2`).
pub fn same_host(origin: &str, host: &str) -> bool {
    let o = origin
        .split("://")
        .nth(1)
        .unwrap_or(origin)
        .split('/')
        .next()
        .unwrap_or("");
    let strip = |s: &str| -> String {
        // `[::1]:8080` keeps its brackets; `host:port` loses the port.
        if let Some(end) = s.strip_prefix('[').and_then(|r| r.find(']')) {
            return s[..end + 2].to_string();
        }
        s.rsplit_once(':').map(|(h, _)| h).unwrap_or(s).to_string()
    };
    !o.is_empty() && strip(o).eq_ignore_ascii_case(&strip(host))
}

/// The client address: `X-Forwarded-For` (the reverse proxy this server is
/// meant to sit behind), else the socket (when the host installs
/// `ConnectInfo`; dioxus's `serve` does not), else `?`.
fn client_ip(req: &Request) -> String {
    if let Some(v) = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
    {
        if let Some(first) = v.split(',').next().map(str::trim).filter(|s| !s.is_empty()) {
            return first.to_string();
        }
    }
    req.extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0.ip().to_string())
        .unwrap_or_else(|| "?".into())
}

/// The gate: see the module docs.
pub async fn middleware(req: Request, next: Next) -> Response {
    let path = req.uri().path().to_string();
    // Websocket upgrades from a *different* origin are refused outright: a
    // page elsewhere must not drive the terminal or the LSP relay with the
    // user's cookie (Milestone 11). Native clients send no Origin at all.
    if is_upgrade(&req) {
        if let Some(bad) = cross_origin(&req) {
            let ip = client_ip(&req);
            tracing::warn!(target: "moonkale::audit", "{ip} upgrade {path} from origin {bad} → 403");
            return (
                StatusCode::FORBIDDEN,
                "moonkale: cross-origin websocket refused",
            )
                .into_response();
        }
    }
    let public = path == "/login"
        || path.starts_with("/assets/")
        || path.starts_with("/wasm/")
        || path.starts_with("/_dioxus")
        || path == "/favicon.ico";
    let relay = path.starts_with("/api/") || path == "/mcp";
    let mcp_own_token = path == "/mcp" && std::env::var("MOONKALE_MCP_TOKEN").is_ok();
    let ip = client_ip(&req);
    let method = req.method().clone();
    let auth = match token() {
        None => "dev",
        Some(expected) => {
            let ok = presented(&req).is_some_and(|t| constant_time_eq(&t, &expected));
            if ok {
                "token"
            } else if public || mcp_own_token {
                "public"
            } else if relay {
                tracing::warn!(target: "moonkale::audit", "{ip} {method} {path} → 401");
                return (
                    StatusCode::UNAUTHORIZED,
                    "moonkale: log in at /login or send Authorization: Bearer <token>",
                )
                    .into_response();
            } else {
                return Redirect::to("/login").into_response();
            }
        }
    };
    if relay {
        tracing::info!(target: "moonkale::audit", "{ip} {method} {path} auth={auth}");
    }
    next.run(req).await
}

static ATTEMPTS: Mutex<Option<HashMap<String, (u32, Instant)>>> = Mutex::new(None);

fn rate_limited(ip: &str) -> bool {
    let mut g = ATTEMPTS.lock().unwrap();
    let map = g.get_or_insert_with(HashMap::new);
    let now = Instant::now();
    let entry = map.entry(ip.to_string()).or_insert((0, now));
    if now.duration_since(entry.1) > Duration::from_secs(60) {
        *entry = (0, now);
    }
    entry.0 += 1;
    entry.0 > 10
}

const LOGIN_HTML: &str = r#"<!doctype html><html><head><meta charset="utf-8"><title>Moonkale — log in</title>
<style>body{font-family:system-ui,sans-serif;background:#0f1116;color:#e6e8ee;display:flex;justify-content:center;align-items:center;height:100vh;margin:0}
form{background:#161922;border:1px solid #2a2e3a;border-radius:8px;padding:24px;min-width:320px}h1{font-size:18px;margin:0 0 12px}
input{width:100%;box-sizing:border-box;font:inherit;padding:8px;border-radius:4px;border:1px solid #2a2e3a;background:#0b0d12;color:inherit;margin:8px 0}
button{font:inherit;padding:8px 14px;border-radius:4px;border:1px solid #2a2e3a;background:#6ea8fe;color:#0b0d12;cursor:pointer}p.err{color:#e07a75}</style></head>
<body><form method="post" action="/login"><h1>Moonkale</h1><p>This server needs its access token.</p>MSG
<input type="password" name="token" placeholder="token" autofocus autocomplete="current-password"><button type="submit">Log in</button></form></body></html>"#;

pub async fn login_page() -> Html<String> {
    Html(LOGIN_HTML.replace("MSG", ""))
}

#[derive(serde::Deserialize)]
pub struct LoginForm {
    token: String,
}

pub async fn login_post(req: Request) -> Response {
    use axum::extract::FromRequest;
    let ip = client_ip(&req);
    let req_headers = req.headers().clone();
    let form = match Form::<LoginForm>::from_request(req, &()).await {
        Ok(Form(f)) => f,
        Err(e) => return (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    };
    if rate_limited(&ip) {
        tracing::warn!(target: "moonkale::audit", "{ip} POST /login → 429");
        return (
            StatusCode::TOO_MANY_REQUESTS,
            "too many attempts; wait a minute",
        )
            .into_response();
    }
    let Some(expected) = token() else {
        return Redirect::to("/").into_response();
    };
    if !constant_time_eq(form.token.trim(), &expected) {
        tracing::warn!(target: "moonkale::audit", "{ip} POST /login → wrong token");
        return (
            StatusCode::UNAUTHORIZED,
            Html(LOGIN_HTML.replace("MSG", "<p class=err>Wrong token.</p>")),
        )
            .into_response();
    }
    let https = req_headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.eq_ignore_ascii_case("https"));
    let cookie = format!(
        "{COOKIE}={}; Path=/; HttpOnly; SameSite=Strict{}",
        expected,
        if https { "; Secure" } else { "" }
    );
    tracing::info!(target: "moonkale::audit", "{ip} POST /login → ok");
    let mut resp = Redirect::to("/").into_response();
    resp.headers_mut()
        .insert(header::SET_COOKIE, HeaderValue::from_str(&cookie).unwrap());
    resp
}

/// Wire the gate and the login routes onto a router, plus the cross-origin
/// isolation headers the browser wasm runtime needs (`SharedArrayBuffer`):
/// the app loads nothing cross-origin, so they cost nothing. Set
/// `MOONKALE_ISOLATE=0` to drop them (then extensions run on the server).
pub fn protect(router: axum::Router) -> axum::Router {
    router
        .route("/login", axum::routing::get(login_page).post(login_post))
        .layer(axum::middleware::from_fn(middleware))
        .layer(axum::middleware::from_fn(isolation_headers))
}

async fn isolation_headers(req: Request, next: Next) -> Response {
    let mut resp = next.run(req).await;
    if std::env::var("MOONKALE_ISOLATE")
        .map(|v| v != "0")
        .unwrap_or(true)
    {
        let h = resp.headers_mut();
        h.insert(
            "cross-origin-opener-policy",
            HeaderValue::from_static("same-origin"),
        );
        h.insert(
            "cross-origin-embedder-policy",
            HeaderValue::from_static("require-corp"),
        );
    }
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_host_comparison() {
        assert!(same_host("http://127.0.0.1:8090", "127.0.0.1:8090"));
        assert!(same_host("https://moon.example", "moon.example:8443"));
        assert!(same_host("http://[::1]:3000", "[::1]:3000"));
        assert!(!same_host("http://evil.example", "127.0.0.1:8090"));
        assert!(!same_host("", "127.0.0.1:8090"));
    }

    #[test]
    fn bind_rules() {
        let lo: IpAddr = "127.0.0.1".parse().unwrap();
        let any: IpAddr = "0.0.0.0".parse().unwrap();
        assert!(bind_mode(Some(lo), false, false, false).is_ok());
        assert!(bind_mode(None, false, false, false).is_ok());
        assert!(bind_mode(Some(any), false, false, false).is_err());
        // Off loopback: token and TLS (or an explicit opt-out).
        assert!(bind_mode(Some(any), true, false, false).is_err());
        assert!(bind_mode(Some(any), true, true, false).is_ok());
        assert!(bind_mode(Some(any), true, false, true).is_ok());
        assert!(bind_mode(Some(any), false, true, false).is_err());
    }

    #[test]
    fn constant_time_compare() {
        assert!(constant_time_eq("abc", "abc"));
        assert!(!constant_time_eq("abc", "abd"));
        assert!(!constant_time_eq("abc", "abcd"));
    }
}
