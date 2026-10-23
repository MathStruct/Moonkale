// WP-06 decision evidence: actual 100k-node JSON engine load and CPU hover path.
import assert from 'node:assert/strict';
import {enginePage} from './graph-harness.mjs';
const host=await enginePage();
try {
 const result=await host.page.evaluate(async()=>{
  const graph={nodes:[],edges:[]};
  for(let i=0;i<100000;i++)graph.nodes.push({id:`n${i}`,label:`folder ${i}`,kind:'directory'});
  for(let i=1;i<100000;i++)graph.edges.push({a:i-1,b:i});
  const begin=performance.now(); const json=JSON.stringify(graph); const encode=performance.now()-begin;
  view.set_graph_fresh(json);
  while(!view.load_stats()[5])await new Promise(requestAnimationFrame);
  const first=performance.now()-begin;
  // Stop automatic layout using the existing pinned-node delta path. This
  // measures hit testing rather than the cost of n-body iterations.
  const positions=JSON.parse(view.node_positions());
  view.apply_delta(JSON.stringify({add_nodes:graph.nodes.map(n=>({...n,pos:positions[n.id],pinned:true}))}));
  await new Promise(requestAnimationFrame);
  const overlay=document.querySelector('#overlay');const r=overlay.getBoundingClientRect(); const samples=[];
  for(let i=0;i<100;i++){
   const start=performance.now();
   overlay.dispatchEvent(new PointerEvent('pointermove',{clientX:r.left+(i*37)%r.width,clientY:r.top+(i*53)%r.height,pointerId:1}));
   samples.push(performance.now()-start);
  }
  samples.sort((a,b)=>a-b);
  return {nodes:graph.nodes.length,bytes:json.length,stringify_ms:encode,first_picture_ms:first,engine_ms:Array.from(view.load_stats()),hover_median_ms:samples[50],hover_p95_ms:samples[95]};
 });
 console.log('COARSE_COSTS',JSON.stringify(result));
 assert.ok(result.hover_p95_ms<16.7,`CPU hover p95 ${result.hover_p95_ms} ms exceeds one frame`);
 console.log('PASS graph-coarse-costs');
} finally {await host.close();}
