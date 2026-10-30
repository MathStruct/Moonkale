// WP-59/60: two real browser windows on one BroadcastChannel session.
import assert from 'node:assert/strict';
import { chromium } from 'playwright';
import { control } from './chrome-controls.mjs';
const browser=await chromium.launch({args:['--use-angle=swiftshader','--enable-unsafe-swiftshader','--ignore-gpu-blocklist']});
const ctx=await browser.newContext({viewport:{width:1800,height:1100}});
await ctx.addInitScript(()=>localStorage.setItem('moonkale.settings',JSON.stringify({language:'en',editor:{implementation:'codemirror',markdown_rich:false}})));
const a=await ctx.newPage(), b=await ctx.newPage(), errors=[];
for(const [name,page] of [['A',a],['B',b]]) { page.setDefaultTimeout(30000); page.on('pageerror',e=>errors.push(`${name}: ${e.message}`)); }
let step='startup';
const url=`http://127.0.0.1:${process.env.PORT??8117}/`;
const shot=async(name,page=a)=>page.screenshot({path:`${process.env.M1_SHOTS}/window-panels-${name}.png`});
const tab=(page,panel)=>page.locator(`#wb-tab-${panel.replaceAll(':','-')}`);
async function file(page,name) {
 await page.bringToFront();
 step=`open ${name}`;
 await page.keyboard.press('Control+p'); await page.locator('#mk-palette-input').fill(name);
 await page.locator(`.mk-palette-item[data-key="${name}"]`).click();
 await page.waitForFunction(name=>document.querySelector('.mk-titlebar-title')?.textContent.includes(name),name);
 await page.locator('.cm-content:visible').first().waitFor();
}
async function overview(page,query='') { await page.bringToFront(); await page.keyboard.press('Control+Alt+Space'); await page.locator('#mk-palette-input').fill(query); }
const cards=(page,remote=true)=>page.locator(`.mk-overview-card${remote?'[data-window]:not([data-window=""])':'[data-window=""]'}`);
const settled=(page,group)=>page.waitForFunction(group=>{const v=window.moonkale?.graphViews?.['mk-graph-panel-'+group];return v&&v.node_count()>0&&v.layout_state()[0]===0;},group,{timeout:60000});
const positions=(page,group)=>page.evaluate(group=>Object.keys(JSON.parse(window.moonkale.graphViews['mk-graph-panel-'+group].node_positions())).sort(),group);
try {
 await a.goto(url,{waitUntil:'networkidle'});
 await a.keyboard.press('Control+o'); await a.locator('#mk-explorer-path').fill(process.env.M1_ROOT); await a.locator('#mk-explorer-path').press('Enter');
 await a.waitForFunction(()=>document.querySelector('.wb-status-bar')?.textContent.includes('index:'));
 await file(a,'Home.md'); await a.keyboard.press('Control+Alt+Enter');
 step='linked group created';
 const graphTab=a.locator('.wb-tab[id^="wb-tab-graph-"]'); await graphTab.waitFor();
 await a.waitForFunction(()=>document.querySelector('.wb-tab[id^="wb-tab-graph-"]')?.dataset.linkGroup);
 const group=await graphTab.getAttribute('data-link-group'), graph=`graph:${group}`,lead=`lead:${group}`;
 const marker=(await graphTab.textContent()).match(/[①②③④⑤⑥⑦⑧]/)[0];
 // A peer learns sources/groups without instantiating A's panels.
 step='peer startup and source handshake';
 await b.goto(url,{waitUntil:'domcontentloaded'}); await b.locator('.wb-workspace').waitFor(); await b.locator('.mk-explorer-source-name').first().waitFor();
 assert.equal(await b.locator('.wb-tab[data-link-group]').count(),0);
 step='graph offered to B';
 await tab(a,graph).dispatchEvent('dragstart'); await b.locator('.mk-drop-target').waitFor();
 await b.locator('.mk-drop-target').dispatchEvent('drop');
 await a.locator('.wb-workspace').dispatchEvent('dragend');
 await tab(b,graph).waitFor(); await tab(a,graph).waitFor({state:'detached'});
 await b.waitForFunction(({panel,group})=>document.getElementById('wb-tab-'+panel.replaceAll(':','-'))?.dataset.linkGroup===group,{panel:graph,group});
 assert((await tab(b,graph).textContent()).includes(marker)); assert((await tab(a,lead).textContent()).includes(marker));
 await tab(b,graph).click(); await settled(b,group); const before=await positions(b,group);
 step='wiki navigation crosses windows';
 await tab(a,lead).click(); await control(a.locator(`#wb-panel-lead-${group} .mk-md-modes button`).filter({hasText:'Rich'}),'click');
 await a.locator(`#wb-panel-lead-${group} .mk-wikilink[data-target="notes/Beta"]:not(.mk-wiki-bracket)`).first().click();
 await a.waitForFunction(group=>document.getElementById('wb-tab-lead-'+group)?.textContent.includes('Beta.md'),group);
 await b.waitForFunction(({group,before})=>{const v=window.moonkale?.graphViews?.['mk-graph-panel-'+group];return v&&JSON.stringify(Object.keys(JSON.parse(v.node_positions())).sort())!==JSON.stringify(before);},{group,before});
 await settled(b,group); await shot('lead-a',a); await shot('graph-b',b);
 console.log('PASS: moved graph retains its marker and follows wiki navigation across windows');
 // Focus the actual retained tab in B, rather than opening a duplicate in A.
 await file(b,'Alpha.md'); await file(b,'README.md');
 await overview(a,'Alpha.md'); step='remote overview catalogue';
 const alpha=cards(a).filter({hasText:'Alpha.md'}); await alpha.waitFor();
 assert.match(await a.locator('.mk-overview-window').last().textContent(),/Window/);
 await shot('overview-a',a);
 await a.locator('#mk-palette-input').press('Enter');
 await b.waitForFunction(()=>[...document.querySelectorAll('.wb-tab[aria-selected="true"]')].some(t=>t.textContent.includes('Alpha.md')));
 console.log('PASS: Enter activates B\'s tab through A\'s labelled overview');
 step='drag remote overview card to current window';
 await overview(a,'Alpha.md'); const remoteAlpha=cards(a).filter({hasText:'Alpha.md'}); await remoteAlpha.waitFor();
 // Begin the real HTML drag before its current-window target mounts.
 const cardBox=await remoteAlpha.boundingBox();
 console.log('drag: mouse down on remote card');
 await a.mouse.move(cardBox.x+20,cardBox.y+20); await a.mouse.down();
 await a.mouse.move(cardBox.x+60,cardBox.y+30,{steps:8});
 console.log('drag: waiting for current-window target');
 await a.locator('.mk-overview-drop[data-dragging="true"]').waitFor();
 const dropBox=await a.locator('.mk-overview-drop').boundingBox();
 await a.mouse.move(dropBox.x+dropBox.width/2,dropBox.y+dropBox.height/2,{steps:12});
 console.log('drag: release on current-window target');
 await a.mouse.up();
 await a.locator('.wb-tab').filter({hasText:'Alpha.md'}).waitFor();
 await a.waitForFunction(()=>document.querySelector('.mk-titlebar-title')?.textContent.includes('Alpha.md'));
 await b.locator('.wb-tab').filter({hasText:'Alpha.md'}).waitFor({state:'detached'});
 console.log('PASS: dropping B\'s card moves its document to A');
 step='move graph back from overview';
 await overview(a,'Graph'); const remoteGraph=cards(a).filter({hasText:marker}); await remoteGraph.waitFor();
 await remoteGraph.locator('.mk-overview-move').click(); await tab(a,graph).waitFor(); await tab(b,graph).waitFor({state:'detached'});
 await a.waitForFunction(group=>document.getElementById('wb-tab-graph-'+group)?.dataset.linkGroup===group,group);
 await shot('returned-a',a);
 step='other named session excludes catalogues';
 await a.locator('.mk-session-switch').click(); await a.locator('#mk-session-name').fill('Other windows isolated');
 await a.getByRole('dialog',{name:'Sessions',exact:true}).getByRole('button',{name:'New session',exact:true}).click();
 await a.getByRole('dialog',{name:'Sessions',exact:true}).waitFor({state:'hidden'});
 await overview(a); assert.equal(await cards(a).count(),0,'other session leaked remote tabs');
 assert.deepEqual(errors,[]); console.log('WINDOW PANELS E2E: PASS');
} catch(error) {
 console.error('WINDOW PANELS E2E: FAIL step='+step,error);
 for(const [name,page] of [['a',a],['b',b]]) { console.log(name,await page.locator('.wb-tab').allTextContents()); await shot('fail-'+name,page).catch(()=>{}); }
 throw error;
} finally { await browser.close(); }
