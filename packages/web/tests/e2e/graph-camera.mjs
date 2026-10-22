// WP-04: camera mapping/coalescing and embedded rectangles on committed wasm.
import assert from 'node:assert/strict';
import { enginePage } from './graph-harness.mjs';
const host = await enginePage();
const { page } = host;
try {
  await page.evaluate(() => {
    view.set_graph_fresh(JSON.stringify({nodes:[{id:'card',label:'hidden label',kind:'file',embed:true,symbol:'M0 0L80 40',icon:'X',caption:'hidden caption',pos:[0,0],width:40,height:20,pinned:true}],edges:[]}));
    view.set_interaction('edit');
  });
  await page.waitForTimeout(400);
  const check = async () => {
    const {event, point} = await page.evaluate(() => ({event:events.filter(e=>e.kind==='camera').at(-1),point:Array.from(view.node_screen_position('card'))}));
    assert.ok(event); assert.equal(event.threeD,false);
    const pos = await page.evaluate(() => JSON.parse(view.node_positions()).card);
    assert.ok(Math.abs(point[0]-(pos[0]*event.scale+event.offset[0]))<0.01);
    assert.ok(Math.abs(point[1]-(pos[1]*event.scale+event.offset[1]))<0.01);
  };
  await check();
  // Synchronous camera changes are collapsed into one next-frame report.
  const count = await page.evaluate(async () => {
    await new Promise(requestAnimationFrame);
    events.length=0;
    for(let i=0;i<20;i++) view.resize(1100+i,700+i,1);
    return new Promise(resolve=>requestAnimationFrame(()=>resolve(events.filter(e=>e.kind==='camera').length)));
  });
  assert.equal(count,1);
  await check();
  await page.mouse.move(80,80); await page.mouse.down(); await page.mouse.move(160,120,{steps:8}); await page.mouse.up();
  await page.waitForTimeout(100); await check();
  await page.mouse.wheel(0,350); await page.waitForTimeout(150); await check();
  // Embed suppresses all overlay ink, including hovered/selected labels.
  const p = await page.evaluate(() => Array.from(view.node_screen_position('card')));
  await page.mouse.click(p[0],p[1]); await page.waitForTimeout(100);
  assert.equal(await page.evaluate(() => {
    return JSON.parse(view.selection_state()).nodes[0];
  }), 'card');
  await page.mouse.move(10,10); await page.keyboard.press('Escape'); await page.waitForTimeout(100);
  const ink = await page.evaluate(() => {
    const c=document.querySelector('#overlay'); const d=c.getContext('2d').getImageData(0,0,c.width,c.height).data;
    let n=0; for(let i=3;i<d.length;i+=4) if(d[i]) n++; return n;
  });
  assert.equal(ink,0,'embedded node drew label, symbol, icon or caption');
  await page.evaluate(() => view.set_mode('3d'));
  await page.waitForFunction(() => events.filter(e=>e.kind==='camera').at(-1)?.threeD === true);
  await page.evaluate(() => view.set_mode('2d'));
  await page.waitForFunction(() => events.filter(e=>e.kind==='camera').at(-1)?.threeD === false);
  await check();
  console.log('PASS graph-camera');
} finally { await host.close(); }
