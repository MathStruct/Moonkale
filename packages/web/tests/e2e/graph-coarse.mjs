// Hermetic lazy 1,000,501-node source, production GraphPanel and shipped engine.
// Start tests/fixtures/graph-coarse with dx serve --platform web --port 8112.
import assert from 'node:assert/strict';
import {chromium} from 'playwright';
import {decode} from './pnglit.mjs';
const browser=await chromium.launch({args:['--use-angle=swiftshader','--enable-unsafe-swiftshader','--ignore-gpu-blocklist']});
const page=await browser.newPage({viewport:{width:1600,height:1000}});
try {
 await page.goto(`http://127.0.0.1:${process.env.COARSE_PORT||8112}/`,{waitUntil:'networkidle'});
 await page.waitForFunction(()=>Object.values(window.moonkale?.graphViews||{}).length===1);
 const start=Date.now();
 await page.click('#coarse-open');
 await page.waitForFunction(()=>document.querySelector('.mk-graph-tier')?.dataset.tier==='coarse'&&Object.values(window.moonkale.graphViews)[0].node_count()===500&&Object.values(window.moonkale.graphViews)[0].load_stats()[5]>0);
 const elapsed=Date.now()-start;
 console.log("COARSE_FIRST_PICTURE",elapsed);
 assert.ok(elapsed<2000,`coarse picture took ${elapsed} ms`);
 assert.equal(await page.$eval('.mk-graph-tier',e=>e.dataset.sourceNodes),'1000501');
 const png=decode(await page.screenshot()); let pixels=0;
 for(let i=0;i<png.data.length;i+=png.bpp) if(Math.max(...png.data.subarray(i,i+3))-Math.min(...png.data.subarray(i,i+3))>15)pixels++;
 assert.ok(pixels>100,'coarse nodes must produce visible coloured pixels');
 await page.waitForFunction(()=>Object.values(window.moonkale.graphViews)[0].layout_state()[0]===0);
 const camera=await page.evaluate(()=>Array.from(Object.values(window.moonkale.graphViews)[0].camera_state()));
 const id=await page.evaluate(()=>Object.keys(JSON.parse(Object.values(window.moonkale.graphViews)[0].node_positions()))[0]);
 const point=await page.evaluate(id=>Array.from(Object.values(window.moonkale.graphViews)[0].node_screen_position(id)),id);
 const box=await page.locator('.mk-graph-overlay').boundingBox();
 await page.mouse.dblclick(box.x+point[0],box.y+point[1]);
 await page.waitForFunction(()=>Object.values(window.moonkale.graphViews)[0].node_count()===2499);
 assert.deepEqual(await page.evaluate(()=>Array.from(Object.values(window.moonkale.graphViews)[0].camera_state())),camera);
 assert.ok(await page.locator('.mk-graph-coarse-collapse').isVisible());
 await page.waitForFunction(()=>Object.values(window.moonkale.graphViews)[0].layout_state()[0]===0);
 // Measure real animation frames during continuous pan after expansion.
 await page.evaluate(()=>{window.frameSamples=[];window.frameLast=performance.now();window.frameRecord=true;function frame(t){if(!window.frameRecord)return;window.frameSamples.push(t-window.frameLast);window.frameLast=t;requestAnimationFrame(frame);}requestAnimationFrame(frame);});
 await page.mouse.move(box.x+box.width*.9,box.y+box.height*.9);
 await page.mouse.down();
 for(let i=0;i<40;i++) {await page.mouse.move(box.x+box.width*.9-i*3,box.y+box.height*.9-i, {steps:1});await page.waitForTimeout(16);}
 await page.mouse.up();
 const frames=await page.evaluate(()=>{window.frameRecord=false;return window.frameSamples.slice(1).sort((a,b)=>a-b);});
 const p95=frames[Math.floor(frames.length*.95)];
 assert.ok(frames.length>20,'pan must produce sustained frames');
 console.log("COARSE_PAN",JSON.stringify({frames:frames.length,p95_ms:p95,median_ms:frames[Math.floor(frames.length/2)]}));
 assert.ok(p95<=50,`pan p95 ${p95} ms exceeds 50 ms`);
 await page.locator('.mk-graph-coarse-collapse').click();
 await page.waitForFunction(()=>Object.values(window.moonkale.graphViews)[0].node_count()===500);
 assert.equal(await page.evaluate(()=>Object.values(window.moonkale.graphViews)[0].edge_count()),499);
 await page.locator('.mk-graph-reload').click();
 await page.waitForFunction(()=>Object.values(window.moonkale.graphViews)[0].node_count()===500);
 // Explicit detail override uses the existing 100k cap; auto restores coarsening.
 await page.selectOption('.mk-graph-tier-choice','detailed');
 await page.waitForFunction(()=>document.querySelector('.mk-graph-tier')?.dataset.tier==='approximate'&&Object.values(window.moonkale.graphViews)[0].node_count()===100000, null,{timeout:30000});
 await page.selectOption('.mk-graph-tier-choice','auto');
 await page.waitForFunction(()=>document.querySelector('.mk-graph-tier')?.dataset.tier==='coarse'&&Object.values(window.moonkale.graphViews)[0].node_count()===500);
 console.log(`PASS graph-coarse: first picture ${elapsed} ms; expanded pan p95 ${p95.toFixed(1)} ms`);
} catch(error) {
 await page.screenshot({path:`${process.env.M1_SHOTS||'.'}/graph-coarse-failure.png`}).catch(()=>{});
 throw error;
} finally {await browser.close();}
