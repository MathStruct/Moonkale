// @moonkale/wasm-host — the JSON-ABI extension runtime for the browser.
//
// The ABI (packages/ext-host/src/abi.rs) gives the guest a *synchronous*
// `call(ptr, len) -> packed` import. In the browser the host answers with
// async work (server round trips), so the module runs in a Worker that
// blocks on `Atomics.wait` over a SharedArrayBuffer mailbox while the main
// thread resolves the call. Needs a cross-origin-isolated page (COOP/COEP).

type HostCall = (json: string) => Promise<string>

const MAILBOX_BYTES = 8 * 1024 * 1024

// Worker source, kept as text so the bundle is a single file.
const WORKER_SRC = `
let mailbox, flag, data;
const dec = new TextDecoder(), enc = new TextEncoder();
function hostCall(memory, ptr, len, instance) {
  const req = dec.decode(new Uint8Array(memory.buffer, ptr, len));
  const bytes = enc.encode(req);
  // Ask the main thread and wait for the reply in the mailbox.
  Atomics.store(flag, 0, 0);
  postMessage({ kind: "call", json: req });
  Atomics.wait(flag, 0, 0);
  const n = Atomics.load(flag, 1);
  const reply = n >= 0 ? dec.decode(data.slice(0, n)) : '{"ok":false,"error":"reply too large"}';
  const out = enc.encode(reply);
  const rp = instance.exports.alloc(out.length);
  new Uint8Array(memory.buffer, rp, out.length).set(out);
  return (BigInt(rp) << 32n) | BigInt(out.length);
}
onmessage = async (e) => {
  const { bytes, request, sab } = e.data;
  mailbox = sab; flag = new Int32Array(sab, 0, 2); data = new Uint8Array(sab, 8);
  let instance, memory;
  try {
    const imports = { moonkale: {
      log: (ptr, len) => { postMessage({ kind: "log", text: dec.decode(new Uint8Array(memory.buffer, ptr, len)) }); },
      call: (ptr, len) => hostCall(memory, ptr, len, instance),
    } };
    ({ instance } = await WebAssembly.instantiate(bytes, imports));
    memory = instance.exports.memory;
    const req = enc.encode(request);
    const p = instance.exports.alloc(req.length);
    new Uint8Array(memory.buffer, p, req.length).set(req);
    const packed = instance.exports.run(p, req.length);
    const rp = Number(packed >> 32n), rl = Number(packed & 0xffffffffn);
    postMessage({ kind: "done", reply: dec.decode(new Uint8Array(memory.buffer, rp, rl)) });
  } catch (err) {
    postMessage({ kind: "error", error: String(err && err.message || err) });
  }
};
`

const moduleCache = new Map<string, Promise<ArrayBuffer>>()

function moduleBytes(url: string): Promise<ArrayBuffer> {
  let p = moduleCache.get(url)
  if (!p) {
    p = fetch(url, { credentials: "same-origin" }).then((r) => { if (!r.ok) throw new Error(`module ${url}: ${r.status}`); return r.arrayBuffer() })
    moduleCache.set(url, p)
  }
  return p
}

/** Whether this page can run modules here (SharedArrayBuffer + Atomics.wait in a Worker). */
function available(): boolean {
  return typeof SharedArrayBuffer !== "undefined" && (globalThis as any).crossOriginIsolated === true && typeof Worker !== "undefined"
}

/** Run one command: `url` serves the module bytes; `hostCall` answers the guest's host calls
 *  (a JSON `HostCall` in, a JSON `HostReply` out). Resolves with the guest's `RunReply` JSON. */
async function run(url: string, command: string, args: unknown, hostCall: HostCall, onLog?: (t: string) => void): Promise<string> {
  if (!available()) throw new Error("browser wasm runtime unavailable (not cross-origin isolated)")
  const bytes = await moduleBytes(url)
  const sab = new SharedArrayBuffer(MAILBOX_BYTES)
  const flag = new Int32Array(sab, 0, 2)
  const data = new Uint8Array(sab, 8)
  const worker = new Worker(URL.createObjectURL(new Blob([WORKER_SRC], { type: "text/javascript" })))
  const enc = new TextEncoder()
  return new Promise<string>((resolve, reject) => {
    const finish = (f: () => void) => { worker.terminate(); f() }
    worker.onerror = (e) => finish(() => reject(new Error(e.message)))
    worker.onmessage = async (e) => {
      const m = e.data
      if (m.kind === "log") onLog?.(m.text)
      else if (m.kind === "done") finish(() => resolve(m.reply))
      else if (m.kind === "error") finish(() => reject(new Error(m.error)))
      else if (m.kind === "call") {
        let reply: string
        try { reply = await hostCall(m.json) } catch (err) { reply = JSON.stringify({ ok: false, error: String((err as Error)?.message ?? err) }) }
        const out = enc.encode(reply)
        if (out.length > data.length) { Atomics.store(flag, 1, -1) } else { data.set(out); Atomics.store(flag, 1, out.length) }
        Atomics.store(flag, 0, 1)
        Atomics.notify(flag, 0)
      }
    }
    worker.postMessage({ bytes, request: JSON.stringify({ command, args }), sab })
  })
}

const w = window as unknown as { moonkale?: Record<string, unknown> }
w.moonkale = w.moonkale ?? {}
w.moonkale.wasmHost = { available, run }
export {}
