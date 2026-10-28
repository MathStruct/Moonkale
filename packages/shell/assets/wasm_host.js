"use strict";(()=>{var S=`
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
  } catch {
    postMessage({ kind: "error" });
  }
};
`,f=new Map,m=new Set,g=new Map;function O(e){let n=g.get(e);return n||(n=fetch(e,{credentials:"same-origin"}).then(r=>{if(!r.ok)throw new Error(`module ${e}: ${r.status}`);return r.arrayBuffer()}),g.set(e,n)),n}function k(){return typeof SharedArrayBuffer<"u"&&globalThis.crossOriginIsolated===!0&&typeof Worker<"u"}async function R(e,n,r,A,x){if(!k())throw new Error("browser wasm runtime unavailable (not cross-origin isolated)");if((f.get(e)??0)>=4)throw new Error("extension disabled after repeated crashes");if(m.has(e))throw new Error("extension already running");m.add(e);try{let B=await O(e),a=new SharedArrayBuffer(8388608),o=new Int32Array(a,0,2),p=new Uint8Array(a,8),w=URL.createObjectURL(new Blob([S],{type:"text/javascript"})),s=new Worker(w);URL.revokeObjectURL(w);let U=new TextEncoder;return await new Promise((q,E)=>{let u=!1,h=c=>{u||(u=!0,clearTimeout(M),s.terminate(),c())},i=()=>h(()=>{f.set(e,(f.get(e)??0)+1),g.delete(e),E(new Error("extension stopped; retry is limited to three fresh instances"))}),M=setTimeout(i,3e4);s.onerror=i,s.onmessage=async c=>{let t=c.data;if(t.kind==="log")x?.(t.text);else if(t.kind==="done")h(()=>q(t.reply));else if(t.kind==="error")i();else if(t.kind==="call"){let l;try{l=await A(t.json)}catch(b){l=JSON.stringify({ok:!1,error:String(b?.message??b)})}let d=U.encode(l);d.length>p.length?Atomics.store(o,1,-1):(p.set(d),Atomics.store(o,1,d.length)),Atomics.store(o,0,1),Atomics.notify(o,0)}},s.postMessage({bytes:B,request:JSON.stringify({command:n,args:r}),sab:a})})}finally{m.delete(e)}}var y=window;y.moonkale=y.moonkale??{};y.moonkale.wasmHost={available:k,run:R};})();
