// Two-finger pinch + rotation in the Android WebView over raw CDP (Input.dispatchTouchEvent).
import WebSocket from "ws";
const list = await (await fetch("http://127.0.0.1:9222/json")).json();
const ws = new WebSocket(list[0].webSocketDebuggerUrl);
await new Promise((r) => ws.on("open", r));
let id = 0;
const send = (method, params) => new Promise((resolve) => { const i = ++id; const h = (m) => { const d = JSON.parse(m); if (d.id === i) { ws.off("message", h); resolve(d); } }; ws.on("message", h); ws.send(JSON.stringify({ id: i, method, params })); });
const ev = async (expr) => (await send("Runtime.evaluate", { expression: expr, returnByValue: true, awaitPromise: true })).result?.result?.value;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const cam = () => ev("Array.from(Object.values(window.moonkale.graphViews)[0].camera_state())");
const host = await ev("(() => { const r = document.querySelector('.mk-graph-overlay').getBoundingClientRect(); return { x: r.left, y: r.top, w: r.width, h: r.height }; })()");
const cx = host.x + host.w / 2, cy = host.y + host.h / 2;
const touch = async (type, points) => { const r = await send("Input.dispatchTouchEvent", { type, touchPoints: points.map((p, i) => ({ x: p[0], y: p[1], id: i })) }); if (r.error) throw new Error(JSON.stringify(r.error)); };
const c0 = await cam();
await touch("touchStart", [[cx - 30, cy], [cx + 30, cy]]);
for (let d = 30; d <= 150; d += 15) { await touch("touchMove", [[cx - d, cy], [cx + d, cy]]); await sleep(30); }
await touch("touchEnd", []);
await sleep(300);
const c1 = await cam();
console.log("pinch: scale", c0[0].toFixed(3), "→", c1[0].toFixed(3));
await ev("[...document.querySelectorAll('button')].find(b => b.textContent.trim() === '3D').click()");
await sleep(800);
const c2 = await cam();
const pt = (a, r = 60) => [cx + r * Math.cos(a), cy + r * Math.sin(a)];
await touch("touchStart", [pt(0), pt(Math.PI)]);
for (let a = 0.1; a <= 1.0; a += 0.1) { await touch("touchMove", [pt(a), pt(Math.PI + a)]); await sleep(30); }
await touch("touchEnd", []);
await sleep(300);
const c3 = await cam();
console.log("rotate: mode", await ev("document.querySelector('.mk-graph-info').dataset.mode"), "yaw", c2[3].toFixed(3), "→", c3[3].toFixed(3), "dist", c2[5].toFixed(1), "→", c3[5].toFixed(1));
ws.close();
