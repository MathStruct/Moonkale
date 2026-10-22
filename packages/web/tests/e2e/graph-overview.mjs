// P-223: real pointer drags of overview dots pan; visible nodes still click/drag.
import { enginePage } from './graph-harness.mjs';
const host = await enginePage();
const {page} = host;
try {
  await page.evaluate(() => {
    window.v = window.view;
    const nodes = Array.from({length:10000}, (_,i) => ({id:'dense'+i,label:'dense'+i,kind:'file',pos:[(i%100)*200,Math.floor(i/100)*200],pinned:true}));
    v.set_graph_fresh(JSON.stringify({nodes,edges:[]}));
  });
  // Pinned positions isolate gestures from ongoing force-layout motion.
  await page.waitForTimeout(400);
  const snapshot = () => page.evaluate(() => ({camera:Array.from(v.camera_state()),positions:v.node_positions()}));
  const screen = id => page.evaluate(id => {
    const p = v.node_screen_position(id), b = document.querySelector('#overlay').getBoundingClientRect();
    return {x:b.x+p[0],y:b.y+p[1]};
  },id);
  const drag = async p => { await page.mouse.move(p.x,p.y); await page.mouse.down(); await page.mouse.move(p.x+70,p.y+35,{steps:8}); await page.mouse.up(); };
  const before = await snapshot();
  await drag(await screen('dense5050'));
  const after = await snapshot();
  if (before.positions !== after.positions) throw new Error('overview drag moved a node');
  if (JSON.stringify(before.camera) === JSON.stringify(after.camera)) throw new Error('overview drag did not pan');
  // A dot is still a click target, independent of its drag eligibility.
  const dot = await screen('dense5050'); await page.mouse.click(dot.x,dot.y);
  let selected = await page.evaluate(() => JSON.parse(v.selection_state()).nodes);
  if (!selected.includes('dense5050')) throw new Error('overview dot no longer clickable');
  await page.evaluate(() => v.focus(JSON.stringify(['dense5050'])));
  const normalBefore = await snapshot();
  const visible = await screen('dense5050'); await page.mouse.click(visible.x,visible.y);
  selected = await page.evaluate(() => JSON.parse(v.selection_state()).nodes);
  if (!selected.includes('dense5050')) throw new Error('visible node no longer clickable');
  await drag(visible);
  const normalAfter = await snapshot();
  if (normalBefore.positions === normalAfter.positions) throw new Error('visible node did not move');
  if (JSON.stringify(normalBefore.camera) !== JSON.stringify(normalAfter.camera)) throw new Error('visible-node drag panned');
  console.log('PASS graph-overview');
} finally { await host.close(); }
