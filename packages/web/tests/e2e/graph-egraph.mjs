// WP-07: ordinary engine JSON, aggregate deltas and rewrite ancestry on shipped wasm.
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {enginePage} from './graph-harness.mjs';
import {decode} from './pnglit.mjs';
const input=JSON.parse(await readFile(`${process.env.MOONKALE_REPO}/packages/graph-render/fixtures/egraph.json`,'utf8'));
const host=await enginePage(); const {page}=host;
const state=()=>page.evaluate(()=>({nodes:view.node_count(),edges:view.edge_count(),frame:Array.from(view.frame_state()),camera:Array.from(view.camera_state())}));
const ink=async()=>{
 const {data,bpp}=decode(await page.screenshot()); let n=0;
 for(let i=0;i<data.length;i+=bpp) if(Math.max(data[i],data[i+1],data[i+2])-Math.min(data[i],data[i+1],data[i+2])>10)n++;
 return n;
};
try {
 await page.evaluate(json=>view.set_egraph(json),JSON.stringify(input));
 await page.waitForTimeout(500);
 const original=await state();
 assert.equal(original.nodes,input.nodes.length); assert.equal(original.frame[3],4);
 assert.ok(await ink()>1000,'groups and nodes must draw coloured pixels');
 // A real user selection automatically highlights all incoming derivations.
 const p=await page.evaluate(()=>Array.from(view.node_screen_position('a')));
 await page.mouse.click(...p); await page.waitForTimeout(100);
 const chain=await page.evaluate(()=>JSON.parse(view.rewrite_state()));
 assert.deepEqual(chain.edges,['rewrite-assoc','rewrite-congruence','rewrite-div','rewrite-one','rewrite-ratio-use']);
 assert.ok(!chain.edges.includes('rewrite-shift'),'unrelated derivations must stay unhighlighted');
 const before=await state();
 await page.evaluate(()=>view.set_layer_visibility('provenance',false));
 await page.waitForTimeout(100);
 const hidden=await state();
 assert.equal(hidden.edges,original.edges,'visibility must preserve topology');
 assert.ok(hidden.frame[0]<before.frame[0],'provenance strokes must disappear');
 await page.evaluate(()=>view.set_layer_visibility('provenance',true));
 await page.waitForTimeout(100); assert.equal((await state()).frame[0],before.frame[0]);
 await page.evaluate(()=>view.collapse_eclass('result',true));
 await page.waitForTimeout(200);
 const collapsed=await state();
 assert.equal(collapsed.nodes,original.nodes-4); assert.equal(collapsed.edges,original.edges);
 assert.equal(collapsed.frame[3],4); assert.deepEqual(collapsed.camera,before.camera);
 assert.equal(await page.evaluate(()=>view.node_screen_position('a')),undefined);
 assert.deepEqual(await page.evaluate(()=>JSON.parse(view.selection_state()).nodes),['result']);
 assert.ok(await ink()>500,'collapsed classes must still draw');
 await page.evaluate(()=>view.collapse_eclass('result',false));
 await page.waitForTimeout(150); assert.equal((await state()).nodes,original.nodes);
 assert.ok(await page.evaluate(()=>view.node_screen_position('a')));
 await page.evaluate(()=>view.collapse_eclass('',true));
 await page.waitForTimeout(150); assert.equal((await state()).nodes,4);
 await page.evaluate(()=>view.collapse_eclass('',false));
 await page.waitForTimeout(150); assert.equal((await state()).nodes,original.nodes);
 assert.deepEqual((await state()).camera,before.camera);
 // An unmarked representative deterministically chooses the first member.
 delete input.groups[0].representative;
 await page.evaluate(json=>view.set_egraph(json),JSON.stringify(input));
 await page.evaluate(()=>view.collapse_eclass('result',true));
 assert.equal((await state()).nodes,original.nodes-4);
 await assert.rejects(page.evaluate(()=>view.set_egraph('{"nodes":[]}')));
 console.log('PASS graph-egraph');
} finally {await host.close();}
