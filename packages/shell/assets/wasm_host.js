"use strict";(()=>{var x=`
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
`,p=new Map;function B(r){let n=p.get(r);return n||(n=fetch(r,{credentials:"same-origin"}).then(t=>{if(!t.ok)throw new Error(`module ${r}: ${t.status}`);return t.arrayBuffer()}),p.set(r,n)),n}function u(){return typeof SharedArrayBuffer<"u"&&globalThis.crossOriginIsolated===!0&&typeof Worker<"u"}async function q(r,n,t,w,b){if(!u())throw new Error("browser wasm runtime unavailable (not cross-origin isolated)");let h=await B(r),i=new SharedArrayBuffer(8388608),s=new Int32Array(i,0,2),f=new Uint8Array(i,8),a=new Worker(URL.createObjectURL(new Blob([x],{type:"text/javascript"}))),k=new TextEncoder;return new Promise((A,g)=>{let c=o=>{a.terminate(),o()};a.onerror=o=>c(()=>g(new Error(o.message))),a.onmessage=async o=>{let e=o.data;if(e.kind==="log")b?.(e.text);else if(e.kind==="done")c(()=>A(e.reply));else if(e.kind==="error")c(()=>g(new Error(e.error)));else if(e.kind==="call"){let l;try{l=await w(e.json)}catch(y){l=JSON.stringify({ok:!1,error:String(y?.message??y)})}let m=k.encode(l);m.length>f.length?Atomics.store(s,1,-1):(f.set(m),Atomics.store(s,1,m.length)),Atomics.store(s,0,1),Atomics.notify(s,0)}},a.postMessage({bytes:h,request:JSON.stringify({command:n,args:t}),sab:i})})}var d=window;d.moonkale=d.moonkale??{};d.moonkale.wasmHost={available:u,run:q};})();
