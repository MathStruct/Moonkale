// Evaluate JS in the phone's WebView through the forwarded DevTools socket.
import WebSocket from "ws";
const list = await (await fetch("http://127.0.0.1:9222/json")).json();
const ws = new WebSocket(list[0].webSocketDebuggerUrl);
await new Promise((r) => ws.on("open", r));
let id = 0;
const send = (method, params) => new Promise((resolve) => { const i = ++id; const h = (m) => { const d = JSON.parse(m); if (d.id === i) { ws.off("message", h); resolve(d.result); } }; ws.on("message", h); ws.send(JSON.stringify({ id: i, method, params })); });
const expr = process.argv[2];
const r = await send("Runtime.evaluate", { expression: expr, returnByValue: true, awaitPromise: true });
console.log(JSON.stringify(r?.result?.value ?? r, null, 1));
ws.close();
