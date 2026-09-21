// Milestone 11: the standalone `moonkale-server` binary (`cd packages/web && dx build --platform server`).
// No browser — plain fetch/WebSocket against a server this script starts itself:
//   --token-stdin (the token never touches the command line), 401/200, the terminal switched off
//   (MOONKALE_TERMINAL=0 → Exit message), a cross-origin websocket upgrade refused (403),
//   a non-loopback bind refused without TLS, and HTTPS with a self-signed certificate.
import { spawn, execFileSync } from "node:child_process";
import { mkdtempSync, writeFileSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
const REPO = process.env.MOONKALE_REPO ?? join(process.env.HOME, "Code/Moonkale");
const BIN = process.env.MOONKALE_SERVER_BINARY ?? join(REPO, "target/dx/web/debug/web/server");
const ROOT = process.env.M1_ROOT;
const TOKEN = "e2e-stdin-token-" + Math.random().toString(16).slice(2);
const step = async (n, f) => { process.stdout.write(`- ${n} … `); await f(); console.log("ok"); };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const port = () => 18000 + Math.floor(Math.random() * 20000);
const procs = [];
const start = (args, env, { expectExit = false } = {}) => new Promise((resolve, reject) => {
  const p = spawn(BIN, args, { env: { ...process.env, MOONKALE_ROOT: ROOT, MOONKALE_LLM: "mock", ...env }, stdio: ["pipe", "pipe", "pipe"] });
  procs.push(p);
  let log = "";
  const onData = (d) => { log += d.toString(); if (expectExit) return; if (/moonkale: (access token|dev mode|serving)/.test(log) && /Registering|serving https/.test(log)) resolve({ p, log: () => log }); };
  p.stdout.on("data", onData); p.stderr.on("data", onData);
  p.on("exit", (code) => { if (expectExit) resolve({ code, log: () => log }); else reject(new Error(`server exited ${code}: ${log.slice(-400)}`)); });
  p.stdin.write(TOKEN + "\n"); p.stdin.end();
  setTimeout(() => reject(new Error("server did not come up: " + log.slice(-400))), 20000);
});
const post = (base, path, headers = {}, extra = {}) => fetch(`${base}${path}`, { method: "POST", headers: { "content-type": "application/json", ...headers }, body: "{}", ...extra });
const wsOnce = (url, headers, first) => new Promise((resolve) => {
  // node's global WebSocket has no custom headers: use the cookie-less bearer query? No — the server
  // takes the bearer header only, so go through the `ws` package when we need headers.
  import("ws").then(({ default: WS }) => {
    const ws = new WS(url, { headers });
    const out = { messages: [] };
    ws.on("unexpected-response", (_, res) => { out.status = res.statusCode; resolve(out); });
    ws.on("open", () => { out.status = 101; if (first) ws.send(JSON.stringify(first)); });
    ws.on("message", (m) => { out.messages.push(JSON.parse(m.toString())); if (out.messages.some((x) => x.kind === "exit")) { ws.close(); resolve(out); } });
    ws.on("error", (e) => { out.error = String(e); resolve(out); });
    ws.on("close", () => resolve(out));
    setTimeout(() => { try { ws.close(); } catch {} resolve(out); }, 8000);
  });
});
try {
  if (!existsSync(BIN)) throw new Error(`no server binary at ${BIN} — cd packages/web && dx build --platform server`);
  console.log("\n  " + execFileSync(BIN, ["--version"]).toString().trim());
  let s, base;
  await step("--token-stdin: 401 without the bearer, 200 with it; the token is not on the command line", async () => {
    const P = port();
    s = await start(["--port", String(P), "--bind", "127.0.0.1", "--token-stdin"], { MOONKALE_TERMINAL: "0" });
    base = `http://127.0.0.1:${P}`;
    const a = await post(base, "/api/sources/list");
    const b = await post(base, "/api/sources/list", { authorization: `Bearer ${TOKEN}` });
    console.log(`\n  no token ${a.status}, bearer ${b.status}`);
    if (a.status !== 401 || b.status !== 200) throw new Error("token check failed");
    const cmdline = execFileSync("cat", [`/proc/${s.p.pid}/cmdline`]).toString();
    if (cmdline.includes(TOKEN)) throw new Error("token visible in the command line");
  });
  await step("MOONKALE_TERMINAL=0: the terminal socket answers Exit with the switched-off message", async () => {
    const P = base.split(":").pop();
    const r = await wsOnce(`ws://127.0.0.1:${P}/api/terminal`, { authorization: `Bearer ${TOKEN}`, origin: `http://127.0.0.1:${P}` }, { kind: "open", cwd: null, cols: 80, rows: 24 });
    const exit = r.messages.find((m) => m.kind === "exit");
    console.log("\n  ws:", r.status, exit ? exit.message : r.messages.map((m) => m.kind).join(","));
    if (r.status !== 101 || !exit || !/switched off/.test(exit.message ?? "")) throw new Error("terminal not refused");
  });
  await step("a cross-origin websocket upgrade is refused (403); same-origin is not", async () => {
    const P = base.split(":").pop();
    const bad = await wsOnce(`ws://127.0.0.1:${P}/api/presence`, { authorization: `Bearer ${TOKEN}`, origin: "http://evil.example" });
    const good = await wsOnce(`ws://127.0.0.1:${P}/api/presence`, { authorization: `Bearer ${TOKEN}`, origin: `http://127.0.0.1:${P}` });
    console.log(`\n  cross-origin ${bad.status}, same-origin ${good.status}`);
    if (bad.status !== 403) throw new Error("cross-origin upgrade not refused");
    if (good.status !== 101) throw new Error("same-origin upgrade refused");
    s.p.kill();
  });
  await step("--bind 0.0.0.0 with a token but no TLS is refused; MOONKALE_INSECURE_HTTP=1 allows it", async () => {
    const r = await start(["--port", String(port()), "--bind", "0.0.0.0", "--token-stdin"], {}, { expectExit: true });
    console.log("\n  " + r.log().trim().split("\n").pop());
    if (r.code !== 2 || !/plain HTTP/.test(r.log())) throw new Error("plain-HTTP bind not refused");
    const ok = await start(["--port", String(port()), "--bind", "0.0.0.0", "--token-stdin"], { MOONKALE_INSECURE_HTTP: "1" });
    ok.p.kill();
  });
  await step("MOONKALE_TLS_CERT/KEY: https answers, plain http on that port does not", async () => {
    const dir = mkdtempSync(join(tmpdir(), "moonkale-tls-"));
    execFileSync("openssl", ["req", "-x509", "-newkey", "ec", "-pkeyopt", "ec_paramgen_curve:prime256v1", "-nodes", "-keyout", join(dir, "key.pem"), "-out", join(dir, "cert.pem"), "-days", "1", "-subj", "/CN=localhost"], { stdio: "ignore" });
    const P = port();
    const t = await start(["--port", String(P), "--bind", "127.0.0.1", "--token-stdin"], { MOONKALE_TLS_CERT: join(dir, "cert.pem"), MOONKALE_TLS_KEY: join(dir, "key.pem") });
    process.env.NODE_TLS_REJECT_UNAUTHORIZED = "0";
    const ok = await post(`https://127.0.0.1:${P}`, "/api/sources/list", { authorization: `Bearer ${TOKEN}` });
    const no = await post(`https://127.0.0.1:${P}`, "/api/sources/list");
    let plain = "refused";
    try { await post(`http://127.0.0.1:${P}`, "/api/sources/list", { authorization: `Bearer ${TOKEN}` }, { signal: AbortSignal.timeout(3000) }); plain = "answered"; } catch {}
    console.log(`\n  https bearer ${ok.status}, https no token ${no.status}, plain http ${plain}`);
    if (ok.status !== 200 || no.status !== 401 || plain !== "refused") throw new Error("tls check failed");
    t.p.kill();
  });
  console.log("\nSERVER E2E: PASS");
} catch (e) { console.log("\nFAIL:", e.message); process.exitCode = 1; } finally { for (const p of procs) { try { p.kill(); } catch {} } }
