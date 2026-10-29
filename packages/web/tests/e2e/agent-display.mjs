// AU-01..07: mock tool calls while a person keeps typing in a document.
import assert from 'node:assert/strict';
import { chromium } from 'playwright';
import { control } from './chrome-controls.mjs';
const browser = await chromium.launch({args:['--use-angle=swiftshader','--enable-unsafe-swiftshader','--ignore-gpu-blocklist']});
const root = process.env.M1_ROOT;
let lastPage;
const graph = {kind:'graph',nodes:[{id:'a',label:'Agent A',style:'accent'},{id:'b',label:'Agent B',style:'success'},{id:'c',label:'Agent C',style:'warning'}],edges:[{from:'a',to:'b'}]};
try {
 for (const mode of ['ask','when_idle','always','server','native']) {
  const context = await browser.newContext({viewport:{width:1800,height:1100}});
  const page = await context.newPage(); lastPage=page;
  page.setDefaultTimeout(30000);
  const errors=[]; page.on('pageerror',e=>errors.push(e.message));
  await page.addInitScript(mode=>localStorage.setItem('moonkale.settings',JSON.stringify({language:'en',agents:[{name:'Second',llm:{provider:'mock',model:'mock'}}],editor:{implementation:mode==='native'?'native':'codemirror',markdown_rich:false},agent:{open_views:['server','native'].includes(mode)?'always':mode,on_server:mode==='server'}})),mode);
  await page.goto(`http://127.0.0.1:${process.env.PORT??8117}/`,{waitUntil:'networkidle'});
  await page.keyboard.press('Control+o');
  await page.locator('#mk-explorer-path').fill(root);
  await page.locator('#mk-explorer-path').press('Enter');
  await page.waitForFunction(()=>document.querySelector('.wb-status-bar')?.textContent.includes('index:'));
  await page.keyboard.press('Control+p');
  await page.locator('#mk-palette-input').fill('Alpha.md');
  await page.locator('.mk-palette-item[data-key="Alpha.md"]').click();
  const editSelector=mode==='native'?'.mk-editor-core-input-sink':'.cm-content';
  await page.locator(editSelector).first().waitFor();
  if(mode!=='server') await page.waitForFunction(()=>document.querySelector('.mk-agent-provider')?.textContent.includes('mock'));
  else await page.locator('.mk-agent-input').waitFor();
  const editor = page.locator(editSelector).first();
  const originalEditor = await editor.elementHandle();
  await page.waitForFunction(()=>window.moonkale?.graphViews?.['mk-graph-panel']?.node_count()>0);
  const personalSelection=await page.evaluate(()=>{
    const view=window.moonkale.graphViews['mk-graph-panel'];
    const id=Object.keys(JSON.parse(view.node_positions()))[0];view.select_nodes(JSON.stringify([id]));
    return JSON.parse(view.selection_state()).nodes;
  });

  const editorText=()=>mode==='native'?editor.evaluate(el=>[...el.closest('.mk-crust').querySelectorAll('.mk-editor-core-cell')].map(c=>c.textContent).join('')):editor.textContent();
  let firstDisplay=true;
  const tools=()=>page.locator('.mk-agent-tool');
  const call=async(name,input,{offer=true,pause=true}={})=>{
    if(pause) await page.waitForTimeout(2100);
    await page.locator(".wb-tab").filter({hasText:/^Agent$/}).click();
    const previous=await tools().count();
    await page.locator('.mk-agent-input').fill(`/tool ${name} ${JSON.stringify(input)}`);
    if(mode==='server' && firstDisplay) await page.evaluate(()=>window.__mkSwitchProfileAfterSend=true);
    firstDisplay=false;
    await page.evaluate(selector=>{
      document.querySelector('.mk-agent-compose button').click();
      if(window.__mkSwitchProfileAfterSend) {
        const profile=document.querySelector('.mk-agent-profile');profile.value='Second';profile.dispatchEvent(new Event('change',{bubbles:true}));
        window.__mkSwitchProfileAfterSend=false;
      }

      document.querySelector(selector).focus();
    },editSelector);
    await page.keyboard.type('person typing ',{delay:15});
    await page.waitForFunction(previous=>document.querySelectorAll('.mk-agent-tool').length>previous && !document.querySelector('.mk-agent-compose button')?.disabled,previous);
    if(offer && ['ask','when_idle'].includes(mode)) {
      await page.locator('.mk-agent-display-offer').last().waitFor();
      assert.equal(await page.evaluate(selector=>document.activeElement?.matches(selector),editSelector),true);
      await page.locator('.mk-agent-display-show').last().evaluate(el=>el.click());
    }
  };
  await call('ui.show_result',{content:graph});
  await page.waitForFunction(()=>Object.entries(window.moonkale?.graphViews||{}).some(([id,v])=>id.includes('agent:')&&v.node_count()===3));
  assert.equal(await page.evaluate(selector=>document.activeElement?.matches(selector),editSelector),true,`${mode}: graph stole focus`);
  assert.match(await page.locator('.wb-tab .mk-link-marker').allTextContents().then(s=>s.join(' ')),/Default/);
  if(mode==='server') {
    assert.doesNotMatch(await page.locator('.wb-tab .mk-link-marker').allTextContents().then(s=>s.join(' ')),/Second/,'a later profile selector change mislabelled the completed turn');
    await page.locator('.mk-agent-profile').evaluate(el=>{el.value='Default';el.dispatchEvent(new Event('change',{bubbles:true}));});
  }

  const graphBox=await page.locator('[id^="mk-graph-panel-agent:"]').first().boundingBox();
  assert.ok(graphBox.width>=250,'agent graph should have a usable adjacent tile');
  await call('ui.select',{nodes:['a','b']});
  await page.waitForFunction(()=>Object.entries(window.moonkale?.graphViews||{}).some(([id,v])=>id.includes('agent:')&&JSON.parse(v.selection_state()).nodes.length===2));
  await page.waitForFunction(()=>[...document.querySelectorAll('[id^="mk-graph-panel-agent:"]')].some(el=>/2.*nodes/.test(el.closest('.mk-graph').querySelector('.mk-graph-live')?.textContent||'')));
  assert.deepEqual(await page.evaluate(()=>JSON.parse(window.moonkale.graphViews['mk-graph-panel'].selection_state()).nodes),personalSelection,'the agent changed the person’s graph selection');

  assert.equal(await page.evaluate(selector=>document.activeElement?.matches(selector),editSelector),true,`${mode}: select stole focus`);
  assert.match(await editorText(),/person typing/);
  await page.screenshot({path:`${process.env.M1_SHOTS??'.'}/agent-display-graph-${mode}.png`});
  const node=mode==='native'?await page.locator('[id^="wb-tab-editor-"]').first().getAttribute('id').then(id=>id.slice('wb-tab-editor-'.length)):await editor.evaluate(el=>el.closest('[id^="mk-editor-"]').id.slice('mk-editor-'.length));
  await call('ui.reveal',{source:`folder:${root}`,node,line:0,column:1,end_line:0,end_column:4});
  await page.waitForFunction(selector=>document.querySelectorAll(selector).length>=2,editSelector);
  assert.equal(await page.evaluate(selector=>document.activeElement?.matches(selector),editSelector),true,`${mode}: reveal stole focus`);
  await page.keyboard.type('AFTER_REVEAL');
  assert.match(await editorText(),/person typing AFTER_REVEAL/);
  await call('editor.open',{source:`folder:${root}`,node,line:1});
  assert.equal(await editor.evaluate((current,original)=>current===original,originalEditor),true,'legacy open replaced the typing view');
  assert.equal(await page.evaluate(selector=>document.activeElement?.matches(selector),editSelector),true,'legacy open took focus');
  if(['always','native'].includes(mode)) {
    await call('graph.query',{source:`folder:${root}`,mode:'children'},{offer:false});
    const replies=await page.locator('.mk-agent-assistant').allTextContents();
    const other=replies.join('\n').match(/([0-9a-f-]{36})\s+File\s+README\.md/);
    assert.ok(other,'mock did not return the second document');
    await call('ui.reveal',{source:`folder:${root}`,node:other[1],line:0,column:0});
    await page.waitForFunction(selector=>document.querySelectorAll(selector).length>=3,editSelector);
    assert.equal(await editor.evaluate((current,original)=>current===original,originalEditor),true,'second reveal replaced the typing view');
    assert.match(await editorText(),/AFTER_REVEAL/);
  }

  console.log(`display ${mode}: selected graph, range and focus preserved`);
  if(mode==='always') {
    // A tight flood: rejected calls consume budget too.
    await call('ui.canvas',{content:{kind:'markdown',text:'# Safe **canvas**\n\n```rust\nfn f<T>() -> Vec<T> { vec![] }\n```'}},{offer:false});
    assert.match(await tools().last().textContent(),/Shown in your marked link group/);
    await page.waitForFunction(()=>document.querySelector('.mk-agent-canvas')?.textContent.includes('fn f<T>() -> Vec<T>'));
    const floodStart=await tools().count();
    for(let i=0;i<8;i++) await call('ui.canvas',{content:{kind:'markdown',text:`flood ${i}`}},{offer:false,pause:false});
    const flood=await tools().allTextContents();
    assert.ok(flood.slice(floodStart).some(text=>text.includes('rate limit')),'display flood was not limited');
    await call('ui.canvas',{content:{kind:'markdown',text:'<script>window.pwned=1</script>'}},{offer:false});
    assert.match(await tools().last().textContent(),/HTML/);
    assert.equal(await page.evaluate(()=>window.pwned),undefined);
    await call('ui.canvas',{content:{kind:'markdown',text:'x'.repeat(65536)}},{offer:false});
    assert.match(await tools().last().textContent(),/64 KiB/);
    await call('ui.canvas',{content:{kind:'table',columns:['Result'],rows:[['safe cell']]}},{offer:false});
    await page.waitForFunction(()=>[...document.querySelectorAll('.mk-table-cell')].some(el=>el.textContent==='safe cell'));
    await call('ui.canvas',{content:{kind:'table',columns:['A','B'],rows:[['one','two'],['three','four']]}},{offer:false});
    await page.waitForFunction(()=>[...document.querySelectorAll('.mk-table-hname')].some(el=>el.textContent==='B'));
    await call('ui.canvas',{content:{kind:'table',columns:['Only'],rows:[['updated cell']]}},{offer:false});
    await page.waitForFunction(()=>[...document.querySelectorAll('.mk-table-cell')].some(el=>el.textContent==='updated cell'));

  }

  if(mode==='server') {
    await page.locator('.wb-tab').filter({hasText:/^Agent$/}).click();
    await control(page.locator('.mk-agent-profile'),'selectOption','Second');
    await page.keyboard.press('Escape');
    await call('ui.canvas',{content:graph},{offer:false});
    await page.waitForFunction(()=>[...document.querySelectorAll('.mk-link-marker')].some(el=>el.textContent.includes('Second')));
    const names=await page.locator('.wb-tab .mk-link-marker').allTextContents().then(s=>s.join(' '));
    assert.match(names,/Default/);assert.match(names,/Second/);
    await page.waitForFunction(()=>new Set([...document.querySelectorAll('.wb-tab[data-link-group]')].map(t=>t.dataset.linkGroup)).size===2);
  }
  assert.deepEqual(errors,[]);
  await page.screenshot({path:`${process.env.M1_SHOTS??'.'}/agent-display-${mode}.png`});
  await context.close();
 }
 console.log('AGENT DISPLAY E2E: PASS');
} catch(e) { await lastPage?.screenshot({path:`${process.env.M1_SHOTS??'.'}/agent-display-failure.png`}).catch(()=>{}); console.log('FAIL:',e.stack); process.exitCode=1; } finally { await browser.close(); }
