// WP-59: real panels and input; renderer inspection is read-only.
import { chromium } from 'playwright';
import assert from 'node:assert/strict';
import { control } from './chrome-controls.mjs';
const browser = await chromium.launch({args:['--use-angle=swiftshader','--enable-unsafe-swiftshader','--ignore-gpu-blocklist']});
const page = await browser.newPage({viewport:{width:1800,height:1100}});
page.setDefaultTimeout(30000);
const errors=[];
page.on('pageerror',e=>errors.push(e.message));
await page.addInitScript(()=>localStorage.setItem('moonkale.settings',JSON.stringify({language:'en',editor:{implementation:'codemirror',markdown_rich:false}})));
const tab=id=>page.locator(`#wb-tab-${id.replaceAll(':','-')}`);
const groupTabs=()=>page.locator('.wb-tab[data-link-group]');
// Marks are reapplied one animation frame after the workbench re-renders.
const marked=n=>page.waitForFunction(n=>document.querySelectorAll('.wb-tab[data-link-group]').length===n,n,{timeout:5000}).catch(()=>{});
const groups=()=>page.locator('.wb-tab[id^="wb-tab-graph-"]').evaluateAll(tabs=>tabs.map(t=>({panel:t.id.replace('wb-tab-graph-','graph:'),group:t.dataset.linkGroup,number:t.textContent.match(/[①②③④⑤⑥⑦⑧]/)?.[0]})));
const lead=group=>page.locator(`.wb-tab[id^="wb-tab-lead-"][data-link-group="${group}"]`);
const panel=group=>page.locator(`#wb-panel-lead-${group}`);
const open=async name=>{await page.keyboard.press('Control+p');await page.locator('#mk-palette-input').fill(name);await page.locator(`.mk-palette-item[data-key="${name}"]`).click();await page.waitForFunction(name=>document.querySelector('.mk-titlebar-title')?.textContent.includes(name),name);await page.locator('.cm-content:visible').first().waitFor();};
const openContext=async name=>{await open(name);await page.keyboard.press('Control+Alt+Enter');await page.waitForFunction(n=>document.querySelectorAll('.wb-tab[id^="wb-tab-graph-"]').length===n,++count);};
const switcher=()=>page.getByRole('dialog',{name:'Sessions',exact:true});
const switchTo=async name=>{await page.locator('.mk-session-switch').click();await switcher().getByRole('button',{name:new RegExp(`^${name}`)}).first().click();await switcher().waitFor({state:'hidden'});};
const graphState=group=>page.evaluate(group=>{const v=window.moonkale?.graphViews?.['mk-graph-panel-'+group];return v?{ids:Object.keys(JSON.parse(v.node_positions())).sort(),camera:Array.from(v.camera_state())}:null;},group);
const settle=group=>page.waitForFunction(group=>{const v=window.moonkale?.graphViews?.['mk-graph-panel-'+group];return v&&v.node_count()>0&&v.layout_state()[0]===0;},group,{timeout:60000});
const persisted=()=>page.waitForFunction(()=>Object.keys(localStorage).filter(k=>k.startsWith('moonkale.state/sessions/')).some(k=>{try {const bytes=localStorage[k].match(/../g).map(h=>parseInt(h,16));const data=JSON.parse(new TextDecoder().decode(new Uint8Array(bytes))).data;return data.name==='Linked test'&&data.link_groups?.length===2;}catch{return false;}}));
let count=0;
try {
 await page.goto(`http://127.0.0.1:${process.env.PORT??8117}/`,{waitUntil:'networkidle'});
 await page.locator('.mk-session-switch').click();await page.locator('#mk-session-name').fill('Linked test');await switcher().getByRole('button',{name:'New session',exact:true}).click();await switcher().waitFor({state:'hidden'});
 await page.keyboard.press('Control+o');await page.locator('#mk-explorer-path').fill(process.env.M1_ROOT);await page.locator('#mk-explorer-path').press('Enter');await page.waitForFunction(()=>document.querySelector('.wb-status-bar')?.textContent.includes('index:'));
 await openContext('Home.md');await openContext('Alpha.md');
 await page.waitForFunction(()=>[...document.querySelectorAll('.wb-tab[id^="wb-tab-graph-"]')].every(t=>t.dataset.linkGroup));
 const [one,two]=(await groups()).sort((a,b)=>a.number.localeCompare(b.number));assert(one.group&&two.group&&one.group!==two.group);assert.equal(one.number,'①');assert.equal(two.number,'②');
 assert.equal(await groupTabs().count(),4);
 for(const g of [one,two]) {await settle(g.group);assert.match(await lead(g.group).getAttribute('aria-label'),/linked with.*group/);}
 await lead(one.group).hover();assert(await tab(one.panel).evaluate(el=>el.classList.contains('mk-link-hover')));assert(!(await tab(two.panel).evaluate(el=>el.classList.contains('mk-link-hover'))));
 // Reopen an existing group lead from the keyboard while Settings covers it.
 await page.keyboard.press('Control+,');
 await page.waitForFunction(()=>document.getElementById('wb-tab-settings')?.getAttribute('aria-selected')==='true');
 await open('Home.md');
 await page.waitForFunction(group=>document.getElementById('wb-tab-lead-'+group)?.getAttribute('aria-selected')==='true',one.group);
 await panel(one.group).locator('.cm-content').waitFor({state:'visible'});
 console.log('PASS: keyboard quick open reveals an existing linked lead behind Settings');
 const beforeTwo=await graphState(two.group);
 await lead(one.group).click();await control(page,'click',`#wb-panel-lead-${one.group} .mk-md-modes button:has-text('Rich')`);
 await panel(one.group).locator('.mk-wikilink[data-target="notes/Beta"]:not(.mk-wiki-bracket)').first().click();
 await page.waitForFunction(group=>document.querySelector('#wb-tab-lead-'+group)?.textContent.includes('Beta.md'),one.group);
 await settle(one.group);assert.equal((await lead(two.group).textContent()).includes('Alpha.md'),true);assert.deepEqual(await graphState(two.group),beforeTwo);
 console.log('PASS: two marked groups; wiki navigation only moves its own graph');
 // Find Home by its real hover label, then use the renderer's ordinary pointer route.
 await tab(one.panel).click();await settle(one.group);
 const ids=(await graphState(one.group)).ids;let found=false;
 for(const id of ids) {
   const point=await page.evaluate(({group,id})=>{const p=window.moonkale.graphViews['mk-graph-panel-'+group].node_screen_position(id);const r=document.getElementById('mk-graph-panel-'+group+'-ov').getBoundingClientRect();return p?{x:r.x+p[0],y:r.y+p[1]}:null;},{group:one.group,id});
   if(!point)continue;await page.mouse.move(point.x,point.y);await page.waitForTimeout(50);
   if(await page.locator('.mk-graph-popup-title').filter({hasText:/^Home(?:\.md)?$/}).count()) {await page.mouse.dblclick(point.x,point.y,{delay:70});found=true;break;}
 }
 assert(found,'Home must be reachable in the group graph');await page.waitForFunction(group=>document.querySelector('#wb-tab-lead-'+group)?.textContent.includes('Home.md'),one.group);
 await page.keyboard.press('Alt+ArrowLeft');await page.waitForFunction(group=>document.querySelector('#wb-tab-lead-'+group)?.textContent.includes('Beta.md'),one.group);
 console.log('PASS: graph double-click steers the group and Back returns');
 await persisted();await switchTo('Default');assert.equal(await groupTabs().count(),0);await switchTo('Linked test');await page.waitForFunction(()=>document.querySelectorAll('.wb-tab[data-link-group]').length===4);
 await persisted();await page.reload({waitUntil:'domcontentloaded'});await page.waitForFunction(()=>document.querySelectorAll('.wb-tab[data-link-group]').length===4);await settle(one.group);await settle(two.group);
 await panel(one.group).locator('.cm-content').waitFor({state:'visible'});
 await panel(two.group).locator('.cm-content').waitFor({state:'visible'});
 assert((await lead(one.group).textContent()).includes('Beta.md'));assert((await lead(two.group).textContent()).includes('Alpha.md'));
 await page.screenshot({path:`${process.env.M1_SHOTS}/linked-panels.png`});
 console.log('PASS: groups survive a session switch and reload');
 await lead(one.group).locator('..').locator('.wb-tab-close').click();await tab(one.panel).waitFor({state:'detached'});await marked(2);assert.equal(await groupTabs().count(),2);
 // Keyboard-only tab menu -> Link to -> pin inside its current group.
 await tab(two.panel).focus();await page.keyboard.press('Shift+F10');
 const linkDialog=page.getByRole('dialog',{name:'Link to…',exact:true});
 await linkDialog.getByRole('button',{name:'Link to…',exact:true}).click();
 await linkDialog.getByRole('checkbox').check();
 await linkDialog.getByRole('button',{name:/② Alpha.md/}).click();
 await lead(two.group).locator('..').locator('.wb-tab-close').click();
 await lead(two.group).waitFor({state:'detached'});await tab(two.panel).waitFor();
 await marked(1);assert.equal(await groupTabs().count(),1,'pinned graph keeps its marking after lead closure');
 // Mouse-only unlink retains the graph as an independent view.
 await tab(two.panel).click({button:'right'});await linkDialog.getByRole('button',{name:'Unlink',exact:true}).click();
 await page.waitForFunction(id=>!document.getElementById('wb-tab-'+id.replaceAll(':','-'))?.dataset.linkGroup,two.panel);
 assert.equal(await groupTabs().count(),0);await tab(two.panel).waitFor();
 console.log('PASS: keyboard pin, pinned close rule and mouse unlink retain the context view');
 assert.deepEqual(errors,[]);console.log('LINKED PANELS E2E: PASS');
} catch(e) {console.error('LINKED PANELS FAIL',e);console.log('views',await page.evaluate(()=>Object.fromEntries(Object.entries(window.moonkale?.graphViews||{}).map(([id,v])=>[id,{nodes:v.node_count(),layout:Array.from(v.layout_state())}]))).catch(()=>null));await page.screenshot({path:`${process.env.M1_SHOTS}/linked-panels-failure.png`,timeout:5000}).catch(()=>{});console.log(await page.locator('.wb-tab').allTextContents().catch(()=>[]));console.log(errors);throw e;} finally {await browser.close();}
