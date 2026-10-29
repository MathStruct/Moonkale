// WP-60: real keymap, retained tabs, lazy previews, grouping and close routes.
import { chromium } from 'playwright';
import assert from 'node:assert/strict';
import { writeFile, unlink, appendFile } from 'node:fs/promises';
const browser = await chromium.launch({args:['--use-angle=swiftshader','--enable-unsafe-swiftshader','--ignore-gpu-blocklist']});
const page = await browser.newPage({viewport:{width:1800,height:1100}});
page.setDefaultTimeout(30000);
const errors=[];
page.on('pageerror',e=>errors.push(e.message));
await page.addInitScript(()=>localStorage.setItem('moonkale.settings',JSON.stringify({language:'en',editor:{implementation:'codemirror',markdown_rich:false},keymap:'left-hand'})));
const input=()=>page.locator('#mk-palette-input');
const cards=()=>page.locator('.mk-overview-card');
const names=['Alpha.md',...Array.from({length:11},(_,i)=>`Overview${String(i+1).padStart(2,'0')}.md`)];
const performanceFiles=Array.from({length:50},(_,i)=>`OverviewPerf${String(i+1).padStart(2,'0')}.md`);
const generated=[...names.slice(1),...performanceFiles];
const open=async name=>{await page.keyboard.press('Control+p');await input().fill(name);await page.locator(`.mk-palette-item[data-key="${name}"]`).click();await page.waitForFunction(name=>document.querySelector('.mk-titlebar-title')?.textContent.includes(name),name);};
const overview=async()=>{await page.keyboard.press('Control+Alt+Space');await page.locator('.mk-overview').waitFor();await input().waitFor();};
try {
 for(const name of generated) await writeFile(`${process.env.M1_ROOT}/${name}`,`# ${name}\n\nA bounded schematic for the overview.\n`);
 await page.goto(`http://127.0.0.1:${process.env.PORT??8117}/`,{waitUntil:'networkidle'});
 await page.keyboard.press('Control+o');await page.locator('#mk-explorer-path').fill(process.env.M1_ROOT);await page.locator('#mk-explorer-path').press('Enter');await page.waitForFunction(()=>document.querySelector('.wb-status-bar')?.textContent.includes('index:'));
 for(const name of names) { await open(name); console.log(`opened ${name}`); }
 // Retain only the twelve document tabs, then split through the overview.
 for(const id of await page.locator('.wb-tab').evaluateAll(tabs=>tabs.filter(t=>!t.textContent.includes('.md')).map(t=>t.id))) {
   const tab=page.locator(`[id="${id}"]`);await tab.locator('..').locator('.wb-tab-close').click({force:true});
 }
 await overview();assert.equal(await cards().count(),12);
 await input().press('ArrowRight');assert.equal(await page.locator('#mk-palette-option-1').getAttribute('aria-selected'),'true');
 await input().press('ArrowLeft');assert.equal(await page.locator('#mk-palette-option-0').getAttribute('aria-selected'),'true');
 await input().press('Escape');
 await page.evaluate(()=>document.activeElement.dispatchEvent(new KeyboardEvent('keydown',{key:'F13',code:'F13',bubbles:true,cancelable:true})));
 await page.locator('.mk-overview').waitFor();assert.equal(await cards().count(),12);
 await input().fill('Overview11');await input().press('Shift+Enter');await page.locator('.mk-overview').waitFor({state:'hidden'});
 await page.waitForFunction(()=>[...document.querySelectorAll('.wb-tile')].filter(t=>t.querySelector('.wb-tab')).length===2);
 // Measure from native DOM keydown capture to the frame containing all cards.
 await page.evaluate(()=>{
   window.__overviewStart=0;window.__overviewMs=null;
   const key=e=>{if(e.code==='Space'&&e.ctrlKey&&e.altKey) {window.__overviewStart=performance.now();window.removeEventListener('keydown',key,true);}};
   window.addEventListener('keydown',key,true);
   const observer=new MutationObserver(()=>{if(document.querySelectorAll('.mk-overview-card').length===12&&window.__overviewStart){requestAnimationFrame(()=>{window.__overviewMs=performance.now()-window.__overviewStart;observer.disconnect();});}});
   observer.observe(document.body,{subtree:true,childList:true});
 });
 await overview();await page.waitForFunction(()=>window.__overviewMs!==null);
 const ms=await page.evaluate(()=>window.__overviewMs);assert(ms<100,`twelve cards took ${ms.toFixed(2)}ms`);console.log(`PASS: twelve tabs in two tiles, overview painted in ${ms.toFixed(2)} ms`);await appendFile(`${process.env.M1_SHOTS}/overview-measurements.log`,JSON.stringify({at:new Date().toISOString(),tabs:12,tiles:2,keyToPaintMs:ms})+"\n");
 await input().fill('alp');assert.equal(await cards().count(),1);assert.match(await cards().textContent(),/Alpha.md/);
 await input().press('Enter');await page.waitForFunction(()=>document.querySelector('.mk-titlebar-title')?.textContent.includes('Alpha.md'));
 console.log('PASS: type alp filters to Alpha.md; Enter switches');
 await page.keyboard.press('Control+Alt+Enter');await page.waitForSelector('.wb-tab[data-link-group]');
 await page.waitForFunction(()=>Object.values(window.moonkale?.graphViews||{}).some(v=>v.node_count()>0));
 await overview();
 const group=await page.locator('.wb-tab[data-link-group]').first().getAttribute('data-link-group');
 const pair=page.locator(`.mk-overview-card[data-link-group="${group}"]`);assert.equal(await pair.count(),2);
 const positions=await pair.evaluateAll(els=>els.map(e=>({x:e.getBoundingClientRect().x,y:e.getBoundingClientRect().y,text:e.textContent})));
 assert.equal(positions[0].y,positions[1].y);assert(positions.every(p=>/[①②③④⑤⑥⑦⑧]/.test(p.text)));
 await page.waitForSelector('.mk-overview-preview canvas[data-snapshot-nodes]');
 assert(await page.locator('.mk-overview-preview canvas').first().evaluate(canvas=>canvas.getContext('2d').getImageData(0,0,256,120).data.some((value,i)=>i%4===3&&value>0)), 'graph schematic must contain painted pixels');
 await page.screenshot({path:`${process.env.M1_SHOTS}/overview.png`});
 console.log('PASS: linked note and graph are adjacent with group markers and an engine preview');
 await input().press('Home');await input().press('Tab');
 assert.equal(await page.locator('.mk-overview-card[aria-selected="true"]').getAttribute('data-link-group'),'');
 await input().fill('Overview10');await input().press('Delete');await page.waitForFunction(()=>![...document.querySelectorAll('.wb-tab')].some(t=>t.textContent.includes('Overview10.md')));
 assert.equal(await cards().count(),0);await input().press('Escape');
 console.log('PASS: Delete closes the selected tab');
 await overview();await input().fill('Overview08');await cards().click({button:'middle'});
 await page.waitForFunction(()=>![...document.querySelectorAll('.wb-tab')].some(t=>t.textContent.includes('Overview08.md')));await input().press('Escape');
 // Dirty deletion asks before losing text, with keyboard cancellation.
 await open('Overview09.md');await page.locator('.cm-content:visible').first().click();await page.keyboard.type('dirty');
 await overview();await input().fill('Overview09');await input().press('Delete');
 await page.getByRole('alertdialog').waitFor();await page.keyboard.press('Escape');assert.equal(await page.getByRole('alertdialog').count(),0);assert.equal(await cards().count(),1);
 await cards().locator('.mk-overview-close').click();await page.getByRole('alertdialog').getByRole('button',{name:'Close tab',exact:true}).click();await page.waitForFunction(()=>![...document.querySelectorAll('.wb-tab')].some(t=>t.textContent.includes('Overview09.md')));await input().press('Escape');
 console.log('PASS: dirty close confirmation, keyboard cancellation and mouse completion');
 // Same command is discoverable from the title bar and command palette.
 await page.locator('.mk-titlebar-search').click();await input().fill('Overview');await page.locator('[data-command-id="view.overview"]').click();await page.locator('.mk-overview').waitFor();await input().press('Escape');
 // The design's larger performance target, in addition to twelve-tab acceptance.
 for(const name of performanceFiles) {
   if(await page.locator('.wb-tab').count()>=50) break;
   await open(name);
 }
 const total=await page.locator('.wb-tab').count();assert.equal(total,50);
 await page.evaluate(total=>{
   window.__overviewStart=0;window.__overviewMs=null;
   const key=e=>{if(e.code==='Space'&&e.ctrlKey&&e.altKey){window.__overviewStart=performance.now();window.removeEventListener('keydown',key,true);}};
   window.addEventListener('keydown',key,true);
   const observer=new MutationObserver(()=>{if(document.querySelectorAll('.mk-overview-card').length===total&&window.__overviewStart){requestAnimationFrame(()=>{window.__overviewMs=performance.now()-window.__overviewStart;observer.disconnect();});}});
   observer.observe(document.body,{subtree:true,childList:true});
 },total);
 await overview();await page.waitForFunction(()=>window.__overviewMs!==null);
 const fiftyMs=await page.evaluate(()=>window.__overviewMs);assert(fiftyMs<100,`fifty cards took ${fiftyMs.toFixed(2)}ms`);
 await appendFile(`${process.env.M1_SHOTS}/overview-measurements.log`,JSON.stringify({at:new Date().toISOString(),tabs:50,keyToPaintMs:fiftyMs})+"\n");
 await page.waitForFunction(()=>document.querySelectorAll('.mk-overview-preview pre').length>0&&document.querySelectorAll('.mk-overview-preview pre').length<40);
 await page.locator('#mk-palette-results').evaluate(el=>{el.scrollTop=el.scrollHeight;});
 await page.waitForFunction(()=>document.querySelectorAll('.mk-overview-preview pre').length<40);
 await input().press('Escape');
 console.log(`PASS: fifty tabs painted in ${fiftyMs.toFixed(2)} ms; offscreen previews unmount`);
 assert.deepEqual(errors,[]);console.log('OVERVIEW E2E: PASS');
} catch(e) { console.error('OVERVIEW FAIL',e); console.log(errors);await page.screenshot({path:`${process.env.M1_SHOTS}/overview-failure.png`}).catch(()=>{});throw e; }
finally {for(const name of generated) await unlink(`${process.env.M1_ROOT}/${name}`).catch(()=>{});await browser.close();}
