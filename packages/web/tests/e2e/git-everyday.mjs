// WP-52: production controls, disposable fixture repository, local CLI assertions.
import { chromium } from 'playwright';
import { control } from './chrome-controls.mjs';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import assert from 'node:assert/strict';
const root=process.env.M1_ROOT, port=process.env.PORT;
if(!root?.includes('e2e') || !fs.existsSync(`${root}/.git`) || !fs.lstatSync(`${root}/.git`).isDirectory()) throw Error('Use the disposable E2E fixture');
const git=(...args)=>execFileSync('git',['-C',root,...args],{encoding:'utf8'}).trim();
if(git('show','-s','--format=%s',git('rev-list','--max-parents=0','HEAD'))!=='fixture') throw Error('Expected the disposable fixture root commit');
const browser=await chromium.launch({args:['--use-angle=swiftshader','--enable-unsafe-swiftshader','--ignore-gpu-blocklist']});
let page=await browser.newPage({viewport:{width:1600,height:1000}});
const throttle=Number(process.env.MOONKALE_CPU_THROTTLE || 1);
if(throttle>1) await (await page.context().newCDPSession(page)).send('Emulation.setCPUThrottlingRate',{rate:throttle});
const logs=[];page.on('pageerror',e=>logs.push(e.stack));page.on('console',m=>{if(!m.text().includes('session['))logs.push(m.text().slice(0,500));});page.on('requestfailed',r=>logs.push(`${r.url()}: ${r.failure()?.errorText}`));
let step='setup';
const until=async fn=>{const end=Date.now()+15000;let last;while(Date.now()<end){try{if(await fn())return;}catch(error){last=error;}await page.waitForTimeout(150);}throw Error('condition timed out',{cause:last});};
const palette=async id=>{
  await page.keyboard.press('Control+Shift+p');
  await page.locator('.mk-palette-input').fill(id.replace('git.',''));
  const item=page.locator(`.mk-palette-item[data-key='${id}']`);
  if(!await item.isVisible()) {await page.locator('.mk-palette-input').fill('');}
  await item.click();
};
// Wait for the status UI, then the exact requested diff, rather than using
// disk state or visibility as a proxy for which mode the person opened.
const diff=async(path,mode)=>{
  await page.locator(`.mk-git-entry[data-path="${path}"][data-mode="${mode}"] .mk-git-path`).click();
  const view=page.locator(`.mk-git-diff[data-path="${path}"][data-mode="${mode}"]`);
  await view.waitFor({state:'visible'});
  await view.locator('.mk-git-diff-body').waitFor();
  await until(()=>view.locator('.mk-git-stage-selection').first().isEnabled());
  return view;
};
const tools=()=>page.locator('.mk-git-everyday');
const button=name=>tools().getByRole('button',{name,exact:true});
const confirm=async keyboard=>{
  await page.locator('#mk-git-everyday-confirm').waitFor();
  if(keyboard){await page.waitForFunction(()=>document.activeElement?.id==='mk-git-everyday-confirm');await page.keyboard.press('Enter');}
  else await page.locator('#mk-git-everyday-confirm').click();
  await page.locator('#mk-git-everyday-confirm').waitFor({state:'detached'});
};
const refresh=async()=>{await control(page.locator(".mk-git-head [data-command='git.refresh']"),'click');await page.waitForTimeout(400);};
try{
  // Restore only this disposable fixture's test state, including earlier failures.
  git('switch','--discard-changes','main');
  git('reset','--hard',git('rev-list','--max-parents=0','HEAD'));
  for(const name of ['topic','renamed','recovered']) if(git('branch','--list',name)) git('branch','-D',name);
  while(git('stash','list')) git('stash','drop','stash@{0}');
  // Prepare two owned commits; all subsequent mutations use UI controls.
  fs.writeFileSync(`${root}/everyday.txt`,'one\ntwo\nthree\n');git('add','everyday.txt');git('commit','-qm','everyday base');
  const base=git('rev-parse','HEAD');
  fs.writeFileSync(`${root}/everyday.txt`,'ONE\ntwo\nthree\n');git('add','everyday.txt');git('commit','-qm','everyday tip');
  const tip=git('rev-parse','HEAD');
  await page.goto(`http://127.0.0.1:${port}/`,{waitUntil:'networkidle'});
  await page.locator('.wb-workspace').waitFor({timeout:90000});
  await page.locator('.mk-explorer-open button[type=submit]').click();
  await page.waitForFunction(()=>document.querySelector('.wb-status-bar')?.textContent.includes('index:'));
  await page.locator('.wb-tab').filter({hasText:'Changes'}).click();
  await page.locator('.mk-git-branch').waitFor();

  step='branch create rename upstream compare';
  await control(page.locator(".mk-git-head [data-command='git.branches']"),'click');
  await tools().locator('.mk-git-branch-name').fill('topic');
  await tools().locator('.mk-git-commit-select').selectOption(tip);
  await button('Create branch').click();await confirm(true);await until(()=>git('branch','--list','topic')==='topic');
  await tools().locator('#mk-git-everyday-first').selectOption('topic');
  await tools().locator('.mk-git-branch-name').fill('renamed');
  await button('Rename branch').click();await confirm();await until(()=>git('branch','--list','renamed')==='renamed');
  await tools().locator('#mk-git-everyday-first').selectOption('renamed');
  await tools().locator('.mk-git-upstream-select').selectOption('refs/heads/main');
  await button('Set upstream').click();await confirm();assert.equal(git('rev-parse','--abbrev-ref','renamed@{upstream}'),'main');
  await tools().locator('.mk-git-compare-select').selectOption('main');await button('Compare branches').click();

  step='dirty switch refuses and offers stash';
  fs.writeFileSync(`${root}/everyday.txt`,'dirty\n');fs.writeFileSync(`${root}/loose.txt`,'loose\n');await refresh();
  await button('Switch branch').click();await confirm(true);
  await until(()=>tools().locator('.mk-git-everyday-output').innerText().then(t=>t.includes('Working tree has changes')));
  assert.equal(git('branch','--show-current'),'main');
  await tools().getByRole('checkbox').check();await button('Switch branch').click();await confirm();await until(()=>git('branch','--show-current')==='renamed');
  assert(!fs.existsSync(`${root}/loose.txt`));assert.equal(git('stash','list').split('\n').length,1);

  step='stash list show apply pop push drop';
  await palette('git.stash');await tools().locator('.mk-git-stash-select option').first().waitFor({state:'attached'});
  await button('Show stash').click();await until(()=>tools().locator('.mk-git-everyday-output').innerText().then(t=>t.includes('loose')));
  await button('Apply stash…').click();await confirm();await until(()=>fs.existsSync(`${root}/loose.txt`));
  assert.equal(fs.readFileSync(`${root}/everyday.txt`,'utf8'),'dirty\n');
  await tools().locator('.mk-git-stash-message').fill('roundtrip');await tools().locator('.mk-git-stash-untracked').check();
  await button('Push stash…').click();await confirm();await until(()=>!fs.existsSync(`${root}/loose.txt`));
  await tools().locator('.mk-git-stash-select').selectOption({index:0});await button('Pop stash…').click();await confirm();await until(()=>fs.existsSync(`${root}/loose.txt`));
  await tools().locator('.mk-git-stash-select').selectOption({index:0});await button('Drop stash…').click();await confirm();await until(()=>!git('stash','list'));

  step='soft mixed and hard reset require named review; reflog restore';
  await palette('git.undo');await tools().locator('.mk-git-commit-select').selectOption(base);
  await tools().locator('.mk-git-reset-mode').selectOption('Soft');await button('Reset branch…').click();await confirm(true);assert.equal(git('rev-parse','HEAD'),base);assert.equal(fs.readFileSync(`${root}/everyday.txt`,'utf8'),'dirty\n');
  await tools().locator('.mk-git-reset-mode').selectOption('Mixed');await button('Reset branch…').click();await confirm();assert.equal(fs.readFileSync(`${root}/everyday.txt`,'utf8'),'dirty\n');
  await tools().locator('.mk-git-reset-mode').selectOption('Hard');await button('Reset branch…').click();
  await page.locator('#mk-git-everyday-confirm').waitFor();
  assert((await tools().innerText()).includes('LOSE'));assert((await tools().innerText()).includes('everyday.txt'));
  await button('Cancel').click();assert.equal(fs.readFileSync(`${root}/everyday.txt`,'utf8'),'dirty\n');
  await button('Reset branch…').click();await confirm();assert.equal(fs.readFileSync(`${root}/everyday.txt`,'utf8'),'one\ntwo\nthree\n');
  await tools().locator('.mk-git-commit-select').selectOption(tip);await tools().locator('.mk-git-branch-name').fill('recovered');
  await button('Restore as branch').click();await confirm();assert.equal(git('rev-parse','recovered'),tip);

  step='cherry pick and revert';
  await button('Cherry-pick commit…').click();await confirm();await until(()=>fs.readFileSync(`${root}/everyday.txt`,'utf8').startsWith('ONE'));
  await tools().locator('.mk-git-commit-select').selectOption(git('rev-parse','HEAD'));await button('Revert commit…').click();await confirm();await until(()=>fs.readFileSync(`${root}/everyday.txt`,'utf8').startsWith('one'));

  step='blame hover previous commit shared gutters';
  await page.locator('.wb-tab').filter({hasText:'Sources'}).click();await page.locator('.mk-tree-row[title="everyday.txt"]').click();await page.locator('.cm-content').waitFor();
  await palette('git.blame');await tools().locator('.mk-git-blame-path').fill('everyday.txt');await button('Blame').click();
  await page.locator('.cm-line-annotation').first().waitFor();assert((await page.locator('.cm-line-annotation').first().getAttribute('title')).includes('Revert'));
  await tools().getByRole('button',{name:'Commit before this one',exact:true}).first().click();await until(()=>tools().locator('.mk-git-blame-row').first().innerText().then(t=>t.includes('everyday tip')));
  await button('Close').click();
  step='stage and unstage lines and hunks';
  let view;
  fs.writeFileSync(`${root}/everyday.txt`,'one\nTWO\nthree\nFOUR\n');await refresh();
  view=await diff('everyday.txt','worktree');await page.locator('.mk-git-diff-body').waitFor();
  await view.locator('.mk-diff-add').filter({hasText:'+FOUR'}).locator('button').click();await until(()=>git('show',':everyday.txt').includes('FOUR'));
  assert(git('show',':everyday.txt').includes('two'));
  view=await diff('everyday.txt','staged');
  await view.locator('.mk-diff-add').filter({hasText:'+FOUR'}).locator('button').click();await until(()=>!git('show',':everyday.txt').includes('FOUR'));
  view=await diff('everyday.txt','worktree');
  const keyboardHunk=view.locator('.mk-git-stage-selection[data-kind=hunk]').first();
  await until(()=>keyboardHunk.isEnabled());
  await page.waitForFunction(()=>document.activeElement?.closest('.mk-git-diff-head'));
  await page.keyboard.press('Tab');
  await page.waitForFunction(()=>document.activeElement?.matches('.mk-git-stage-selection[data-kind=hunk]'));
  await page.keyboard.press('Enter');await until(()=>git('diff','--','everyday.txt')==='');
  view=await diff('everyday.txt','staged');await view.locator('.mk-git-stage-selection[data-kind=hunk]').first().click();await until(()=>git('diff','--cached','--','everyday.txt')==='');

  step='partial new and deleted text files';
  fs.writeFileSync(`${root}/fresh.txt`,'first\nsecond\n');await refresh();
  view=await diff('fresh.txt','worktree');
  await view.locator('.mk-diff-add').filter({hasText:'+second'}).locator('button').click();
  await until(()=>git('show',':fresh.txt')==='second');
  view=await diff('fresh.txt','staged');
  await view.locator('.mk-diff-add').filter({hasText:'+second'}).locator('button').click();
  await until(()=>git('ls-files','--','fresh.txt')==='');
  view=await diff('fresh.txt','worktree');
  await view.locator('.mk-git-stage-selection[data-kind=hunk]').first().click();
  await until(()=>git('show',':fresh.txt')==='first\nsecond');
  view=await diff('fresh.txt','staged');
  await view.locator('.mk-diff-add').filter({hasText:'+second'}).locator('button').click();
  await until(()=>git('show',':fresh.txt')==='first');
  await view.locator('.mk-git-stage-selection[data-kind=hunk]').first().click();
  await until(()=>git('ls-files','--','fresh.txt')==='');
  fs.rmSync(`${root}/everyday.txt`);await refresh();
  view=await diff('everyday.txt','worktree');
  await view.locator('.mk-diff-del').filter({hasText:'-two'}).locator('button').click();
  await until(()=>git('show',':everyday.txt')==='one\nthree');
  view=await diff('everyday.txt','staged');
  await view.locator('.mk-diff-del').filter({hasText:'-two'}).locator('button').click();
  await until(()=>git('show',':everyday.txt')==='one\ntwo\nthree');
  assert(!fs.existsSync(`${root}/everyday.txt`));
  fs.writeFileSync(`${root}/everyday.txt`,'one\ntwo\nthree\n');

  step='delete branch confirmation';
  await palette('git.branches');await tools().locator('#mk-git-everyday-first').selectOption('main');
  await tools().getByRole('checkbox').check();await button('Switch branch').click();await confirm();await until(()=>git('branch','--show-current')==='main');
  // recovered points to a commit retained by main, so deletion is safe.
  await tools().locator('#mk-git-everyday-first').selectOption('recovered');await button('Delete branch…').click();await confirm();await until(()=>!git('branch','--list','recovered'));
  await page.screenshot({path:`${process.env.M1_SHOTS}/git-everyday.png`});
  step='Rust editor shares blame gutter API';
  const native=await browser.newContext({viewport:{width:1600,height:1000}});
  await native.addInitScript(()=>localStorage.setItem('moonkale.settings',JSON.stringify({extensions:{enabled:['dev.moonkale.editor-code-native']},editor:{implementation:'native',markdown_rich:false}})));
  page=await native.newPage();
  await page.goto(`http://127.0.0.1:${port}/`,{waitUntil:'networkidle'});
  await page.locator('.wb-workspace').waitFor({timeout:90000});
  await page.locator('.mk-explorer-open button[type=submit]').click();
  await page.locator('.mk-tree-row[title="everyday.txt"]').click();await page.locator('.mk-native-surface').waitFor();
  await palette('git.blame');await tools().locator('.mk-git-blame-path').fill('everyday.txt');await button('Blame').click();
  await page.locator('.mk-native-gutter-mark').first().waitFor();
  assert((await page.locator('.mk-native-gutter-mark').first().getAttribute('title')).includes('everyday tip'));
  await page.screenshot({path:`${process.env.M1_SHOTS}/git-everyday-native-blame.png`});
  await page.locator('.mk-editor-core-input-sink').focus();await page.keyboard.press('Control+End');await page.keyboard.type('dirty');
  await page.locator('.mk-native-gutter-mark').first().waitFor({state:'detached'});
  step='server refuses a jailed worktree with outside Git metadata';
  const outside=fs.mkdtempSync(path.join(os.tmpdir(),'moonkale-git-outside-'));
  const pointer=`${root}/pointer-repo`;
  try {
    execFileSync('git',['init','-q','-b','main',outside]);
    fs.mkdirSync(pointer,{recursive:true});fs.writeFileSync(`${pointer}/.git`,`gitdir: ${outside}/.git\n`);
    const response=await page.request.post(`http://127.0.0.1:${port}/api/git`,{data:{root:pointer,req:'Status'}});
    assert(response.ok());assert.match((await response.json()).Err,/outside MOONKALE_ROOT/);
  }finally{fs.rmSync(pointer,{recursive:true,force:true});fs.rmSync(outside,{recursive:true,force:true});}
  console.log('GIT EVERYDAY E2E: PASS');
}catch(e){console.error(`FAIL ${step}: ${e.stack ?? e}`,logs.slice(-8));await page.screenshot({path:`${process.env.M1_SHOTS}/git-everyday-fail.png`});process.exitCode=1;}
finally{
  await browser.close();
  try {
    git('switch','--discard-changes','main');
    git('reset','--hard',git('rev-list','--max-parents=0','HEAD'));
    for(const name of ['topic','renamed','recovered']) if(git('branch','--list',name)) git('branch','-D',name);
    while(git('stash','list')) git('stash','drop','stash@{0}');
  } catch(error) { console.error('fixture cleanup:', error); process.exitCode=1; }
}
