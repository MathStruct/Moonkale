// AU-10/11/13/14: independent users, real transports, no position frames when off.
import assert from 'node:assert/strict';
import { chromium } from 'playwright';
import { control } from './chrome-controls.mjs';
const browser = await chromium.launch({args:['--use-angle=swiftshader','--enable-unsafe-swiftshader','--ignore-gpu-blocklist']});
const root = process.env.M1_ROOT;
let last;
let step="setup", activeImplementation;
const throttle=Number(process.env.MOONKALE_CPU_THROTTLE || 1);
const pages=[];
async function wire(user,name,document,predicate=()=>true) {
  step=`${name}: received position for ${document}`;
  const end=Date.now()+30000;
  while(Date.now()<end) {
    const member=user.received.findLast(f=>f.kind==='members')?.members.find(m=>m.name===name);
    const cursor=member?.sharing?.cursor;
    if(cursor?.document===document && predicate(cursor)) return cursor;
    await pause(50);
  }
  throw Error(`missing remote position: ${name} ${document}; latest=${JSON.stringify(user.received.at(-1))}`);
}
const pause = ms => new Promise(resolve => setTimeout(resolve, ms));
async function open(name, implementation) {
  const ctx = await browser.newContext({viewport:{width:1700,height:1000}});
  const page = await ctx.newPage(); last=page; pages.push(page);
  if(throttle>1) await (await ctx.newCDPSession(page)).send("Emulation.setCPUThrottlingRate",{rate:throttle});
  const errors=[];
  page.on('pageerror', error=>{errors.push(error.message);console.log(name+' pageerror: '+error.message);});
  const frames=[], received=[];
  page.on('websocket', socket => { if(socket.url().includes('/api/presence')) { socket.on('framesent', e => {try { frames.push(JSON.parse(e.payload.toString())); } catch {} }); socket.on('framereceived', e=>{try {received.push(JSON.parse(e.payload.toString()));}catch{}}); } });
  await page.addInitScript(({name,implementation}) => localStorage.setItem('moonkale.settings',JSON.stringify({language:'en',user_name:name,editor:{implementation,markdown_rich:false},agent:{open_views:'always'}})),{name,implementation});
  step=`${implementation}: open ${name}`;
  await page.goto(`http://127.0.0.1:${process.env.PORT??8117}/`,{waitUntil:'networkidle'});
  await page.keyboard.press('Control+o');
  await page.locator('#mk-explorer-path').fill(root);
  await page.locator('#mk-explorer-path').press('Enter');
  step='folder indexed';
  await page.waitForFunction(()=>document.querySelector('.wb-status-bar')?.textContent.includes('index:'));
  await file(page,'README.md');
  step='global graph ready';
  await page.waitForFunction(()=>window.moonkale?.graphViews?.['mk-graph-panel']?.node_count()>0);
  return {ctx,page,frames,received,errors,editor:implementation==='native'?'.mk-editor-core-input-sink':'.cm-content'};
}
async function file(page,key) {
  const tab=page.locator('.wb-tab').filter({hasText:key}).first();
  if (await tab.count()) await tab.click();
  else {
    await page.keyboard.press('Control+p');
    await page.locator('#mk-palette-input').fill(key);
    await page.locator(`.mk-palette-item[data-key="${key}"]`).click();
  }
  step=`document selected: ${key}`;
  await page.waitForFunction(key=>[...document.querySelectorAll('.wb-tab[aria-selected="true"]')].some(tab=>tab.textContent.includes(key)),key);
}
async function graphView(page) {
  await page.locator('.wb-tab').filter({hasText:/^Graph/}).first().click();
  step='graph canvas visible';
  await page.locator('#mk-graph-panel-ov').waitFor({state:'visible'});
  return page.locator('#mk-graph-panel-ov').boundingBox();
}
async function command(page,id) { await control(page.locator(`.mk-sharing [data-command="sharing.${id}"]`),'click'); }
async function caret(user,keys) {
  await user.page.locator(`${user.editor}:visible`).last().focus();
  for(const key of keys) await user.page.keyboard.press(key);
}
try {
 for(const implementation of ['codemirror','native']) {
  activeImplementation=implementation;
  const a=await open('Ada',implementation), b=await open('Bob',implementation);
  assert.ok(a.frames.some(frame=>frame.kind==='join'), 'presence wire was not observed');
  assert.equal(await a.page.locator('.mk-sharing').getAttribute('data-sharing'),'false');
  await pause(200);
  const off=a.frames.length;
  await caret(a,['Control+Home','ArrowDown','ArrowRight','Shift+ArrowRight']);
  await pause(200);
  assert.equal(a.frames.length,off,'caret movement emitted presence while off');
  const graph=await graphView(a.page);
  await pause(200);
  const pointerOff=a.frames.length;
  await a.page.mouse.move(graph.x+graph.width/2,graph.y+graph.height/2);
  await pause(200);
  assert.equal(a.frames.length,pointerOff,'pointer movement emitted presence while off');
  for(const frame of a.frames) {
    assert.equal(frame.member?.sharing,undefined,'off leaked positions');
    assert.equal(frame.member?.line,undefined,'off leaked line');
  }
  await file(a.page,'README.md');
  console.log(implementation+': off-wire checks passed');
  await command(a.page,'cursor');
  await command(b.page,'cursor');
  await caret(a,['Control+Home','ArrowDown','ArrowRight','Shift+ArrowRight']);
  step='Ada caret received';
  await wire(b,'Ada','README.md',c=>c.anchor!==c.head);
  step='Ada README caret mounted';
  await b.page.locator('.mk-remote-caret[data-participant="Ada"]').first().waitFor();
  step='Ada selection received';
  await b.page.locator('.mk-remote-selection').first().waitFor();
  assert.equal(await a.page.locator('.mk-shared-pointer').count(),0,'cursor sharing must work alone');
  await caret(b,['Control+Home','ArrowRight']);
  step='Bob caret received';
  await a.page.locator('.mk-remote-caret[data-participant="Bob"]').first().waitFor();
  console.log(implementation+': bidirectional carets passed');
  await command(a.page,'pointer');
  await graphView(a.page); await graphView(b.page);
  await a.page.mouse.move(graph.x+graph.width*.6,graph.y+graph.height*.55);
  step='Ada graph pointer received';
  await b.page.locator('.mk-shared-pointer[data-participant="Ada"]').waitFor();
  assert.equal(await a.page.locator('.mk-sharing').getAttribute('data-sharing'),'true');
  step='pointer projection before zoom';
  await b.page.waitForFunction(()=>{
    const node=document.querySelector('.mk-shared-pointer[data-participant="Ada"]'); const canvas=document.getElementById('mk-graph-panel-ov');
    const [scale,cx,cy]=window.moonkale.graphViews['mk-graph-panel'].camera_state(); const box=canvas.getBoundingClientRect(), n=node.getBoundingClientRect();
    return Math.abs(n.left-box.left-((Number(node.dataset.worldX)-cx)*scale+box.width/2))<2;
  });
  // Change the receiver zoom, keeping the world pointer in the right place.
  const otherGraph=await b.page.locator('#mk-graph-panel-ov').boundingBox();
  await b.page.mouse.move(otherGraph.x+otherGraph.width/2,otherGraph.y+otherGraph.height/2);
  await b.page.mouse.wheel(0,-120);
  await pause(150);
  step='pointer projection after zoom';
  await b.page.waitForFunction(()=>{
    const node=document.querySelector('.mk-shared-pointer[data-participant="Ada"]'),canvas=document.getElementById('mk-graph-panel-ov');
    const [scale,cx,cy]=window.moonkale.graphViews['mk-graph-panel'].camera_state();
    return Math.abs(node.getBoundingClientRect().left-canvas.getBoundingClientRect().left-((Number(node.dataset.worldX)-cx)*scale+canvas.getBoundingClientRect().width/2))<2;
  });
  console.log(implementation+': world pointer and zoom passed');
  await file(a.page,'README.md');
  // Have both documents open before choosing the participant to follow.
  await file(b.page,'Alpha.md'); await file(b.page,'README.md');
  await graphView(b.page);
  await control(b.page.locator('.mk-sharing-follow').filter({hasText:'Ada'}),'click');
  step='follow README selected';
  await b.page.waitForFunction(()=>[...document.querySelectorAll('.wb-tab[aria-selected="true"]')].some(e=>e.textContent.includes('README.md')));
  await file(a.page,'Alpha.md');
  await caret(a,['Control+Home','ArrowRight']);
  step='follow Alpha selected';
  await wire(b,'Ada','Alpha.md',c=>c.column===1);
  step='follow Alpha tab selected';
  await b.page.waitForFunction(()=>[...document.querySelectorAll('.wb-tab[aria-selected="true"]')].some(e=>e.textContent.includes('Alpha.md')));
  assert.equal(await b.page.locator('.wb-tab').filter({hasText:'Home.md'}).count(),0,'new follow target was already open');
  await file(a.page,'Home.md');
  await caret(a,['Control+Home']);
  await a.page.keyboard.insertText('UNSAVED_SHARING_SENTINEL');
  step='unsaved sentinel in provider';
  await a.page.waitForFunction(()=>[...document.querySelectorAll('.mk-editor')].some(el=>el.offsetParent!==null && el.textContent.includes('UNSAVED_SHARING_SENTINEL')));
  step='follow Home selected';
  await b.page.waitForFunction(()=>[...document.querySelectorAll('.wb-tab[aria-selected="true"]')].some(e=>e.textContent.includes('Home.md')));
  assert.doesNotMatch(await b.page.locator('.mk-editor:visible').textContent(),/UNSAVED_SHARING_SENTINEL/,'follow sent the provider buffer instead of reading the authorized source');
  await b.page.keyboard.press('Escape');
  assert.equal(await b.page.locator('.mk-sharing-follow').filter({hasText:'Ada'}).getAttribute('aria-pressed'),'false');
  await file(a.page,'README.md'); await pause(180);
  assert.equal(await b.page.locator('.mk-sharing-follow').filter({hasText:'Ada'}).getAttribute('aria-pressed'),'false');
  await control(b.page.locator('.mk-sharing-follow').filter({hasText:'Ada'}),'click');
  await file(b.page,'Alpha.md');
  assert.equal(await b.page.locator('.mk-sharing-follow').filter({hasText:'Ada'}).getAttribute('aria-pressed'),'false','own navigation did not cancel follow');
  console.log(implementation+': follow cancellation passed');
  await command(b.page,'announce');
  step='announcement includes Ada';
  await b.page.waitForFunction(()=>document.querySelector('.mk-sharing-announcement')?.textContent.includes('Ada'));
  const announced=await b.page.locator('.mk-sharing-announcement').textContent();
  const beforeAnnouncement=a.frames.findLast(f=>f.member?.sharing?.cursor)?.member.sharing.cursor;
  await caret(a,['ArrowDown']);
  await wire(b,'Ada','README.md',c=>c.line!==beforeAnnouncement.line);
  assert.equal(await b.page.locator('.mk-sharing-announcement').textContent(),announced,'cursor movement was announced continuously');
  await command(b.page,'announce');
  step='requested announcement changed';
  await b.page.waitForFunction(previous=>document.querySelector('.mk-sharing-announcement')?.textContent!==previous,announced);
  await graphView(a.page); await graphView(b.page);
  await a.page.mouse.move(graph.x+graph.width*.6,graph.y+graph.height*.55);
  step='pointer before Stop';
  await b.page.locator('.mk-shared-pointer[data-participant="Ada"]').waitFor();
  const beforeStop=a.frames.findLast(frame=>frame.member?.sharing?.pointer)?.member;
  assert.ok(beforeStop?.sharing?.cursor && beforeStop?.sharing?.pointer,'both channels must be live before Stop');
  await a.page.keyboard.press('Control+Shift+Escape');
  step='Stop indicator off';
  await a.page.waitForFunction(()=>document.querySelector('.mk-sharing')?.dataset.sharing==='false');
  step='pointer revoked';
  await b.page.locator('.mk-shared-pointer').waitFor({state:'detached'});
  step='Ada caret revoked';
  await b.page.locator('.mk-remote-caret[data-participant="Ada"]').waitFor({state:'detached'});
  assert.equal(a.frames.findLast(frame=>frame.member)?.member.sharing,undefined,'Stop did not revoke both channels on the wire');
  await file(a.page,'README.md');
  await pause(100);
  const stopped=a.frames.length;
  await caret(a,['ArrowRight','ArrowDown']); await pause(200);
  assert.equal(a.frames.length,stopped,'stop left queued movement');
  // The agent is an independently opted-in participant, using the approved reveal tool.
  await file(a.page,'README.md'); await file(b.page,'README.md');
  await command(a.page,'agentCursor');
  const node=await a.page.locator('[id^="wb-tab-editor-"]').filter({hasText:'README.md'}).first().getAttribute('id').then(id=>id.slice('wb-tab-editor-'.length));
  await a.page.locator('.wb-tab').filter({hasText:/^Agent$/}).click();
  const previous=await a.page.locator('.mk-agent-tool').count();
  await a.page.locator('.mk-agent-input').fill(`/tool ui.reveal ${JSON.stringify({source:`folder:${root}`,node,line:0,column:2})}`);
  await a.page.locator('.mk-agent-compose button').click();
  step='agent tool completed';
  await a.page.waitForFunction(previous=>document.querySelectorAll('.mk-agent-tool').length>previous && !document.querySelector('.mk-agent-compose button')?.disabled,previous);
  const started=Date.now();
  while(!a.frames.some(f=>f.member?.sharing?.agent) && Date.now()-started<10000) await pause(100);
  const agent=a.frames.findLast(f=>f.member?.sharing?.agent)?.member.sharing.agent;
  assert.ok(agent,'approved agent reveal did not publish its position');
  step='agent caret received';
  await b.page.locator(`.mk-remote-caret[data-participant="${agent[0]}"]`).first().waitFor();
  assert.ok(a.frames.filter(f=>f.member?.sharing?.agent).every(f=>!f.member.sharing.cursor),'agent switch enabled human cursor');
  await command(a.page,'stop');
  step='agent caret revoked';
  await b.page.locator(`.mk-remote-caret[data-participant="${agent[0]}"]`).waitFor({state:'detached'});
  await b.page.screenshot({path:`${process.env.M1_SHOTS??'.'}/sharing-${implementation}.png`});
  assert.deepEqual(a.errors,[]); assert.deepEqual(b.errors,[]);
  await a.ctx.close(); await b.ctx.close();
  console.log(`${implementation}: caret, selection, pointer, follow, revoke and off-wire checks pass`);
 }
 console.log('SHARING E2E: PASS');
} catch(error) {
 console.error('SHARING E2E: FAIL implementation='+activeImplementation+' step='+step,error);
 for(let i=0;i<pages.length;i++) if(!pages[i].isClosed()) {
   console.log('page '+i+' tabs',await pages[i].locator('.wb-tab').evaluateAll(tabs=>tabs.map(t=>({text:t.textContent,selected:t.getAttribute('aria-selected')}))));
   await pages[i].screenshot({path:`${process.env.M1_SHOTS??'.'}/sharing-fail-${i}.png`}).catch(()=>{});
 }
 await last?.screenshot({path:`${process.env.M1_SHOTS??'.'}/sharing-fail.png`}).catch(()=>{});
 process.exitCode=1;
} finally { await browser.close(); }
