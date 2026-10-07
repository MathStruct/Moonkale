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

/// `MOONKALE_MCP_TOKEN`, if set to something (#7: an empty value used to
/// count as set and leave `/mcp` to a check nobody could fail).
pub fn mcp_token() -> Option<String> {
    std::env::var("MOONKALE_MCP_TOKEN")
        .ok()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
}

/// Whether `t` is a token we can put in a cookie: visible ASCII, no
/// separators (#7: a `;` or a space made the login response panic).
fn token_ok(t: &str) -> bool {
    t.bytes()
        .all(|b| (0x21..=0x7e).contains(&b) && !matches!(b, b';' | b',' | b'"' | b'\\'))
}

/// Whether a presented token matches `expected` (constant time).
pub fn token_matches(presented: &str, expected: &str) -> bool {
    constant_time_eq(presented, expected)
}

/// Refuse to serve a non-loopback address without a token. Called before
/// the router is built; prints the mode once.
pub fn guard_bind() {
    for (name, t) in [("MOONKALE_TOKEN", token()), ("MOONKALE_MCP_TOKEN", mcp_token())] {
        if t.as_deref().is_some_and(|t| !token_ok(t)) {
            eprintln!("moonkale: {name} may contain only visible ASCII without ; , \" or \\");
            std::process::exit(2);
        }
    }
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

/// Compare in time that depends on neither the contents nor where they
/// differ, nor on `a`'s length (#7: an early return leaked the length).
fn constant_time_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let mut diff = (a.len() != b.len()) as u8;
    for (i, y) in b.iter().enumerate() {
        diff |= a.get(i).copied().unwrap_or(0) ^ y;
    }
    diff == 0
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

/// The `Host` the browser used. Under `dx serve` the dev server proxies to
/// the app on an inner port and rewrites `Host` to it, so that port is
/// read as the dev server's — the one the browser's `Origin` carries.
fn host_header(req: &Request) -> String {
    let host = req
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    if !dioxus::cli_config::is_cli_enabled() {
        return host;
    }
    let (inner, outer) = (
        dioxus::cli_config::server_port(),
        dioxus::cli_config::devserver_raw_addr().map(|a| a.port()),
    );
    match (inner, outer) {
        (Some(inner), Some(outer)) => {
            let (h, port) = authority(&host, "http");
            if port == inner {
                format!("{h}:{outer}")
            } else {
                host
            }
        }
        _ => host,
    }
}

/// `Some(origin)` when the request carries an `Origin` that is not this
/// server: host **and port** must match the `Host` it was sent to (#3 — a
/// page on another port of the same host shares the cookie, so a
/// port-blind check let it drive the terminal). Origins a reverse proxy
/// presents (TLS on another port) are listed in `MOONKALE_ALLOWED_ORIGINS`
/// (comma-separated, e.g. `https://moon.example`).
fn cross_origin(req: &Request) -> Option<String> {
    let origin = req
        .headers()
        .get(header::ORIGIN)?
        .to_str()
        .ok()?
        .to_string();
    let allowed = std::env::var("MOONKALE_ALLOWED_ORIGINS").unwrap_or_default();
    let listed = allowed
        .split(',')
        .map(|o| o.trim().trim_end_matches('/'))
        .any(|o| !o.is_empty() && o.eq_ignore_ascii_case(origin.trim_end_matches('/')));
    (!listed && !same_origin(&origin, &host_header(req))).then_some(origin)
}

/// `host[:port]` of an origin or a `Host` header, lower-cased, with the
/// default port of `scheme` filled in.
fn authority(s: &str, scheme: &str) -> (String, u16) {
    let default = if scheme == "https" || scheme == "wss" { 443 } else { 80 };
    let s = s.to_ascii_lowercase();
    if let Some(rest) = s.strip_prefix('[') {
        if let Some((host, after)) = rest.split_once(']') {
            let port = after.strip_prefix(':').and_then(|p| p.parse().ok()).unwrap_or(default);
            return (format!("[{host}]"), port);
        }
    }
    match s.rsplit_once(':') {
        Some((h, p)) if p.chars().all(|c| c.is_ascii_digit()) && !p.is_empty() => {
            (h.to_string(), p.parse().unwrap_or(default))
        }
        _ => (s, default),
    }
}

/// Whether `origin` (`https://a.b:1`) is the server a request with this
/// `Host` header (`a.b:1`) reached: scheme-default ports are equal to
/// absent ones.
pub fn same_origin(origin: &str, host: &str) -> bool {
    let (scheme, rest) = origin.split_once("://").unwrap_or(("http", origin));
    let o = rest.split('/').next().unwrap_or("");
    if o.is_empty() || host.is_empty() {
        return false;
    }
    let (a, b) = (authority(o, scheme), authority(host, scheme));
    // localhost, 127.0.0.1 and [::1] are one machine (a proxy may rewrite
    // one into another): then the port decides.
    a == b || (a.1 == b.1 && loopback_host(o) && loopback_host(host))
}

/// Dev mode serves loopback only, so the `Host` a browser sends must be a
/// loopback name (#3: DNS rebinding makes `Origin` and `Host` both
/// `evil.example`, which an origin check alone passes).
fn loopback_host(host: &str) -> bool {
    let (h, _) = authority(host, "http");
    matches!(h.as_str(), "localhost" | "[::1]") || h.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

/// The client address: the socket's (when the host installs `ConnectInfo`;
/// dioxus's `serve` does not, then `?` — one limiter for everyone), or the
/// first `X-Forwarded-For` entry **only** when the socket peer is a proxy
/// listed in `MOONKALE_TRUSTED_PROXIES` (#7: anyone could send the header
/// and get a fresh rate limit per request).
fn client_ip(req: &Request) -> String {
    let peer = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0.ip());
    let trusted = std::env::var("MOONKALE_TRUSTED_PROXIES").unwrap_or_default();
    let via_proxy = peer.is_some_and(|p| {
        trusted
            .split(',')
            .filter_map(|t| t.trim().parse::<IpAddr>().ok())
            .any(|t| t == p)
    });
    if via_proxy {
        if let Some(first) = req
            .headers()
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(',').next())
            .map(str::trim)
            .filter(|s| s.parse::<IpAddr>().is_ok())
        {
            return first.to_string();
        }
    }
    peer.map(|p| p.to_string()).unwrap_or_else(|| "?".into())
}

/// The gate: see the module docs.
pub async fn middleware(req: Request, next: Next) -> Response {
    let path = req.uri().path().to_string();
    let relay = path.starts_with("/api/") || path == "/mcp";
    // A request from a page that is not this server is refused: websocket
    // upgrades (the terminal, the LSP relay — Milestone 11) and, since #3,
    // every `/api/*` and `/mcp` call, which a page on another port of the
    // same host could otherwise make with the user's cookie. Native
    // clients send no Origin at all.
    if is_upgrade(&req) || relay {
        if let Some(bad) = cross_origin(&req) {
            let ip = client_ip(&req);
            let host = host_header(&req);
            tracing::warn!(target: "moonkale::audit", "{ip} {path} from origin {bad} to host {host} → 403");
            return (
                StatusCode::FORBIDDEN,
                format!("moonkale: requests from {bad} are refused (this is {host}; a proxy's origin goes in MOONKALE_ALLOWED_ORIGINS)"),
            )
                .into_response();
        }
    }
    let public = path == "/login"
        || path.starts_with("/assets/")
        || path.starts_with("/wasm/")
        || path.starts_with("/_dioxus")
        || path == "/favicon.ico";
    let mcp_own_token = path == "/mcp" && mcp_token().is_some();
    let ip = client_ip(&req);
    let method = req.method().clone();
    let auth = match token() {
        None => {
            // Dev mode is loopback-only; a browser that reached it under
            // another name was sent here by DNS rebinding (#3).
            let host = host_header(&req);
            if !host.is_empty() && !loopback_host(&host) {
                tracing::warn!(target: "moonkale::audit", "{ip} {method} {path} for host {host} → 403 (dev mode)");
                return (
                    StatusCode::FORBIDDEN,
                    "moonkale: dev mode answers only to localhost (set MOONKALE_TOKEN to serve other names)",
                )
                    .into_response();
            }
            "dev"
        }
        Some(expected) => {
            let ok = presented(&req).is_some_and(|t| constant_time_eq(&t, &expected));
            if ok {
                "token"
            } else if public || mcp_own_token {
                "public"
            } else if relay {
                // Guessing the token over /api or /mcp is rate-limited like
                // /login (#7).
                if failures_limited(&ip) {
                    tracing::warn!(target: "moonkale::audit", "{ip} {method} {path} → 429");
                    return (StatusCode::TOO_MANY_REQUESTS, "moonkale: too many failed attempts; wait a minute")
                        .into_response();
                }
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

type Attempts = Mutex<Option<HashMap<String, (u32, Instant)>>>;
static ATTEMPTS: Attempts = Mutex::new(None);
static FAILURES: Attempts = Mutex::new(None);

/// Counts one attempt by `ip` in a one-minute window; true beyond `limit`.
/// Expired windows are dropped, and the map never holds more than 10 000
/// addresses (#7: entries were never removed).
fn counted(map: &Attempts, ip: &str, limit: u32) -> bool {
    let mut g = map.lock().unwrap_or_else(|e| e.into_inner());
    let map = g.get_or_insert_with(HashMap::new);
    let now = Instant::now();
    let window = Duration::from_secs(60);
    if map.len() >= 10_000 {
        map.retain(|_, (_, start)| now.duration_since(*start) <= window);
        if map.len() >= 10_000 {
            map.clear();
        }
    }
    let entry = map.entry(ip.to_string()).or_insert((0, now));
    if now.duration_since(entry.1) > window {
        *entry = (0, now);
    }
    entry.0 += 1;
    entry.0 > limit
}

/// `/login`: 10 attempts per minute and client.
fn rate_limited(ip: &str) -> bool {
    counted(&ATTEMPTS, ip, 10)
}

/// Requests with a wrong or missing token on `/api` and `/mcp`: 60 per
/// minute and client (a page that lost its cookie makes a few).
fn failures_limited(ip: &str) -> bool {
    counted(&FAILURES, ip, 60)
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
    // Secure when this server serves TLS or says a TLS proxy is in front of
    // it (MOONKALE_INSECURE_HTTP=1 off loopback); a forwarded-proto header
    // can add it, never take it away (#7).
    let https = tls_files().is_some()
        || std::env::var("MOONKALE_INSECURE_HTTP").is_ok_and(|v| v == "1")
        || req_headers
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
    match HeaderValue::from_str(&cookie) {
        Ok(v) => {
            resp.headers_mut().insert(header::SET_COOKIE, v);
            resp
        }
        // guard_bind refuses such tokens; never panic on a login.
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "the server's token cannot be a cookie").into_response(),
    }
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

/// What a Content-Security-Policy can do for a Dioxus fullstack page (#10).
/// Script sources cannot be restricted: the page carries inline scripts
/// (the hydration bootstrap and per-page hydration data, which no hash
/// matches and Dioxus gives no nonce), and the eval bridges need
/// `unsafe-eval`. So `javascript:` links are stopped by the shell's link
/// guard; the policy adds what holds regardless: no plugins, no `<base>`
/// rewriting, no framing (clickjacking), forms only to this server.
pub const CSP: &str =
    "object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'";

async fn isolation_headers(req: Request, next: Next) -> Response {
    let mut resp = next.run(req).await;
    resp.headers_mut()
        .insert("content-security-policy", HeaderValue::from_static(CSP));
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
        assert!(same_origin("http://127.0.0.1:8090", "127.0.0.1:8090"));
        assert!(same_origin("http://[::1]:3000", "[::1]:3000"));
        assert!(same_origin("https://moon.example", "moon.example"));
        assert!(same_origin("https://moon.example", "moon.example:443"));
        assert!(same_origin("HTTP://Moon.Example:80", "moon.example"));
        // #3: another port of the same host is another origin.
        assert!(!same_origin("http://127.0.0.1:3000", "127.0.0.1:8090"));
        assert!(!same_origin("https://moon.example", "moon.example:8443"));
        assert!(!same_origin("http://evil.example", "127.0.0.1:8090"));
        assert!(!same_origin("", "127.0.0.1:8090"));
        assert!(!same_origin("http://127.0.0.1:8090", ""));
        assert!(same_origin("http://localhost:8095", "127.0.0.1:8095"));
        assert!(!same_origin("http://localhost:3000", "127.0.0.1:8095"));
    }

    #[test]
    fn dev_mode_answers_to_loopback_names_only() {
        for h in ["127.0.0.1:8090", "localhost:8080", "[::1]:3000", "127.0.0.1"] {
            assert!(loopback_host(h), "{h}");
        }
        for h in ["evil.example:8090", "192.168.1.5:8090", "localhost.evil.example"] {
            assert!(!loopback_host(h), "{h}");
        }
    }

    #[test]
    fn attempts_are_counted_per_window_and_bounded() {
        let map: Attempts = Mutex::new(None);
        for _ in 0..3 {
            assert!(!counted(&map, "a", 3));
        }
        assert!(counted(&map, "a", 3));
        assert!(!counted(&map, "b", 3), "another client has its own count");
        for i in 0..20_000 {
            counted(&map, &format!("x{i}"), 3);
        }
        assert!(map.lock().unwrap().as_ref().unwrap().len() <= 10_000);
    }

    #[test]
    fn tokens_must_fit_a_cookie() {
        assert!(token_ok("e2e-secret-token_42"));
        assert!(!token_ok("has space"));
        assert!(!token_ok("semi;colon"));
        assert!(!token_ok("ümlaut"));
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
        assert!(!constant_time_eq("abcd", "abc"));
        assert!(!constant_time_eq("", "abc"));
        assert!(constant_time_eq("", ""));
    }
}
