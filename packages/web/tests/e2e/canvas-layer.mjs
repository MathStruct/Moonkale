// WP-05: production Dioxus layer, actual camera gestures, mount LOD and undo.
import { chromium } from 'playwright';
import fs from 'node:fs';
import assert from 'node:assert/strict';
const browser = await chromium.launch({args:['--use-angle=swiftshader','--enable-unsafe-swiftshader','--ignore-gpu-blocklist']});
const page = await browser.newPage({viewport:{width:1500,height:900}});
const pause = () => page.waitForTimeout(200);
const view = fn => page.evaluate(fn);
const positions = () => view(()=>JSON.parse(Object.values(window.moonkale.flowViews)[0].node_positions()));
const box = () => page.locator('.mk-flow-canvas:visible').boundingBox();
const card = () => page.locator('.mk-flow-props:visible').boundingBox();
const screen = id => page.evaluate(id=>Array.from(Object.values(window.moonkale.flowViews)[0].node_screen_position(id)),id);
const selection = () => view(()=>JSON.parse(Object.values(window.moonkale.flowViews)[0].selection_state()));
try {
  await page.goto(`http://127.0.0.1:${process.env.PORT||8196}/`,{waitUntil:'networkidle'});
  await page.click('.mk-explorer-open button[type=submit]');
  await page.waitForFunction(()=>document.querySelector('.wb-status-bar').textContent.includes('index:'));
  await page.click('#mk-rail-extensions');
  for(const name of ['Flow editor','Lux.jl library']) await page.click(`.mk-settings-ext:has(.mk-settings-ext-name:text-is('${name}')) > label input`);
  fs.writeFileSync(`${process.env.M1_ROOT}/canvas.flow.json`,JSON.stringify({version:1,blocks:[
    {id:'dense',kind:'lux/dense',x:100,y:150,params:{out:'16'}},
    {id:'under',kind:'lux/dense',x:350,y:170,params:{out:'8'}},
    {id:'far',kind:'lux/dense',x:1000,y:750,params:{out:'4'}}],wires:[]}));
  await page.click(".mk-tree-row[title='canvas.flow.json']");
  await page.waitForFunction(()=>Object.values(window.moonkale?.flowViews||{})[0]?.node_positions().includes('dense'));
  await page.waitForTimeout(500);
  const b = await box(), p=await screen('dense');
  await page.mouse.click(b.x+p[0],b.y+p[1]);
  await page.waitForSelector('.mk-flow-props');
  assert.equal(await page.locator('.mk-canvas-layer').count(),1);
  const initial = await card();
  const transform = await page.locator('.mk-canvas-layer').getAttribute('style');
  // Empty canvas passes through the DOM layer, preserving selection during pan.
  await page.mouse.move(b.x+10,b.y+10); await page.mouse.down();
  await page.mouse.move(b.x+60,b.y+40,{steps:8}); await page.mouse.up(); await pause();
  const after=await card();
  assert.ok(Math.abs(after.x-initial.x-50)<3,'card did not follow pan in x');
  assert.ok(Math.abs(after.y-initial.y-30)<3,'card did not follow pan in y');
  assert.notEqual(await page.locator('.mk-canvas-layer').getAttribute('style'),transform);
  // Clicking a field over a second node does not change engine selection.
  const sel=await selection();
  const field = page.locator('.mk-flow-props input').first();
  const fieldBox = await field.boundingBox(), under = await screen('under');
  const scale = await view(()=>Object.values(window.moonkale.flowViews)[0].camera_state()[0]);
  assert.ok(Math.abs(fieldBox.x+fieldBox.width/2-(b.x+under[0])) < 46*scale,
    'click-isolation fixture has no underlying node at the input');
  assert.ok(Math.abs(fieldBox.y+fieldBox.height/2-(b.y+under[1])) < 22*scale);
  await field.click();
  assert.deepEqual(await selection(),sel);
  await page.screenshot({path:`${process.env.M1_SHOTS}/canvas-layer.png`});
  // Header capture survives a move outside its initial bounds; one undo step.
  const before=(await positions()).dense;
  const header=await page.locator('.mk-canvas-header').boundingBox();
  await page.mouse.move(header.x+25,header.y+8); await page.mouse.down();
  await page.waitForTimeout(80);
  await page.mouse.move(header.x+90,header.y+42,{steps:10}); await page.mouse.up(); await pause();
  const moved=(await positions()).dense;
  assert.ok(moved[0]>before[0]+20 && moved[1]>before[1]+10,'header did not drag node');
  await page.locator('.mk-flow-ov:visible').focus(); await page.keyboard.press('Control+z'); await pause();
  assert.deepEqual((await positions()).dense,before,'header drag undo was not one step');
  await page.keyboard.press('Control+y'); await pause(); assert.deepEqual((await positions()).dense,moved);
  // Undo/redo preserve the selected card and move it with the block.
  await page.waitForSelector('.mk-flow-props');
  // Zoom below screen-size threshold removes the component DOM.
  await page.mouse.move(b.x+8,b.y+8); await page.mouse.wheel(0,1800);
  await page.waitForFunction(()=>!document.querySelector('.mk-flow-props'));
  assert.ok((await positions()).dense,'unmount removed engine node');
  // Fit brings the selected editor back without another selection event.
  await view(()=>Object.values(window.moonkale.flowViews)[0].fit());
  await page.waitForSelector('.mk-flow-props');
  // Off-screen culling also unmounts, and a reverse pan remounts.
  await page.mouse.move(b.x+8,b.y+8); await page.mouse.down();
  await page.mouse.move(b.x+b.width-10,b.y+8,{steps:15}); await page.mouse.up();
  await page.waitForFunction(()=>!document.querySelector('.mk-flow-props'));
  await view(()=>Object.values(window.moonkale.flowViews)[0].fit());
  await page.waitForSelector('.mk-flow-props');
  // Culling a captured header ends its gesture; the next drag starts fresh.
  const beforeCull = (await positions()).dense;
  const cullHeader = await page.locator('.mk-canvas-header').boundingBox();
  await page.mouse.move(cullHeader.x+20,cullHeader.y+8); await page.mouse.down();
  await page.waitForTimeout(80);
  await page.mouse.move(b.x+b.width+150,cullHeader.y+8,{steps:12}); await page.mouse.up();
  await page.waitForFunction(()=>!document.querySelector('.mk-flow-props'));
  const afterCull = (await positions()).dense;
  assert.notDeepEqual(afterCull,beforeCull);
  await view(()=>Object.values(window.moonkale.flowViews)[0].focus(JSON.stringify(['dense'])));
  await page.waitForSelector('.mk-flow-props');
  const nextHeader = await page.locator('.mk-canvas-header').boundingBox();
  await page.mouse.move(nextHeader.x+20,nextHeader.y+8); await page.mouse.down();
  await page.waitForTimeout(80);
  await page.mouse.move(nextHeader.x+35,nextHeader.y+8,{steps:4}); await page.mouse.up(); await pause();
  const afterNext = (await positions()).dense;
  assert.ok(afterNext[0] > afterCull[0], 'new header gesture reused culled drag origin');
  await page.locator('.mk-flow-ov:visible').focus();
  await page.keyboard.press('Control+z'); await pause();
  assert.deepEqual((await positions()).dense,afterCull);
  await page.keyboard.press('Control+z'); await pause();
  assert.deepEqual((await positions()).dense,beforeCull);
  // Empty-space click reaches the engine and clears selection.
  await page.mouse.click(b.x+5,b.y+5); await pause();
  assert.equal((await selection()).nodes.length,0);
  console.log('PASS canvas-layer');
} catch(error) {
  console.log('diagnostic',JSON.stringify(await page.evaluate(()=>({layers:[...document.querySelectorAll('.mk-canvas-layer')].map(e=>e.getAttribute('style')),views:Object.values(window.moonkale?.flowViews||{}).map(v=>({camera:Array.from(v.camera_state()),selection:v.selection_state(),positions:v.node_positions()}))}))));
  await page.screenshot({path:`${process.env.M1_SHOTS}/canvas-layer-fail.png`});
  throw error;
} finally { await browser.close(); }
