// WP-51: production UI, disposable local bare remote, no network or account.
import { firefox } from 'playwright';
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { control } from './chrome-controls.mjs';
const ROOT = process.env.M1_ROOT;
if (!ROOT || !fs.existsSync(path.join(ROOT, '.git')) || !ROOT.includes('e2e')) throw Error('Use the disposable E2E fixture');
const scratch = path.join(ROOT, '.git', 'sync-fixture');
fs.rmSync(scratch, {recursive:true, force:true}); fs.mkdirSync(scratch);
const remote = path.join(scratch, 'remote.git'), other = path.join(scratch, 'other');
const env = {...process.env, GIT_TERMINAL_PROMPT:'0', GIT_CONFIG_NOSYSTEM:'1', GIT_CONFIG_GLOBAL:'/dev/null'};
const git = (root, ...args) => execFileSync('git', ['-C', root, ...args], {encoding:'utf8', env, stdio:['ignore','pipe','pipe']}).trim();
const initial = git(ROOT, 'rev-parse', 'HEAD');
const check = (condition, message) => { if (!condition) throw Error(message); };
const commit = (root, text) => { fs.writeFileSync(path.join(root,'sync.txt'), text); git(root,'add','sync.txt'); git(root,'commit','-qm', 'sync fixture'); };
const body = (word) => `${word}\n${Array.from({length:15},(_,i)=>`unchanged ${i}`).join('\n')}\n${word} end\n`;
const browser = await firefox.launch();
const page = await browser.newPage({viewport:{width:1600,height:1000}});
await page.addInitScript(() => localStorage.setItem('moonkale.settings', JSON.stringify({language:'en',density:'simple',chrome_initialized:true,chrome:{'dev.moonkale.ext-git':true}})));
const errors=[]; page.on('pageerror', e=>errors.push(e.message));
const step = async (name, f) => { process.stdout.write(`- ${name} … `); await f(); console.log('ok'); };
const idle = async () => page.waitForFunction(()=>document.querySelector('.mk-git-sync')?.getAttribute('aria-busy') === 'false');
async function action(id, confirm=true, keyboard=false) {
  await idle();
  if (keyboard) {
    const shortcuts = {'git.fetch':'Control+Alt+f','git.pull':'Control+Alt+l','git.push':'Control+Alt+u','git.push-lease':'Control+Alt+Shift+p','git.publish':'Control+Alt+Shift+u'};
    await page.keyboard.press(shortcuts[id]);
  } else await control(page.locator(`.mk-git-head [data-command="${id}"]`), 'click');
  if (id !== 'git.fetch') {
    await page.locator('#mk-git-confirm').waitFor();
    if (!confirm) return;
    if (keyboard) { await page.waitForFunction(()=>document.activeElement?.id==='mk-git-confirm'); await page.keyboard.press('Enter'); }
    else await page.locator('#mk-git-confirm').click();
  }
  await idle();
}
async function refresh() { await control(page.locator('.mk-git-head button[data-command="git.refresh"]'), 'click'); await page.waitForResponse(r=>r.url().includes('/api/git') && r.ok()); }
async function resolve(op, keyboard=false) {
  await page.locator('.mk-git-conflict-view').waitFor();
  check(await page.locator('.mk-conflict-hunk').count()===2, `${op}: expected two independent hunks`);
  for (const i of [0,1]) {
    const pick=page.locator(`.mk-conflict-hunk[data-hunk="${i}"] button`).nth(i===0?0:2);
    if (keyboard) { await pick.focus(); await page.keyboard.press('Enter'); } else await pick.click();
  }
  const result=await page.locator('.mk-conflict-result').inputValue();
  check(!result.includes('<<<<<<<'), 'markers remain after picks');
  const mark=page.getByRole('button',{name:'Mark resolved',exact:true});
  if (keyboard) { await mark.focus(); await page.keyboard.press('Enter'); } else await mark.click();
  await page.locator('.mk-git-conflict-view').waitFor({state:'detached'});
  await idle();
  const cont=page.locator('.mk-git-sync button[data-command="git.continue"]');
  if (keyboard) { await cont.focus(); await page.keyboard.press('Enter'); } else await cont.click();
  await cont.waitFor({state:'detached'});
  await action('git.push',true,keyboard);
  check(git(ROOT,'rev-parse','HEAD')===git(remote,'rev-parse','main'), `${op}: not pushed`);
}
try {
  git(ROOT,'config','user.name','E2E'); git(ROOT,'config','user.email','e2e@example.invalid');
  git(ROOT,'config','pull.rebase','false'); git(ROOT,'config','pull.ff','true');
  commit(ROOT,body('base'));
  git(scratch,'init','--bare','-q','--initial-branch=main',remote);
  git(ROOT,'remote','add','sync-test',remote); git(ROOT,'push','-u','sync-test','main');
  git(scratch,'clone','-q',remote,other);
  git(other,'config','user.name','E2E'); git(other,'config','user.email','e2e@example.invalid');
  await step('open Changes, Sync is primary in Simple',async()=>{
    await page.goto(`http://127.0.0.1:${process.env.PORT}/`,{waitUntil:'networkidle'});
    await page.locator('.mk-explorer-open button[type=submit]').click();
    await page.locator('.wb-tab').filter({hasText:'Changes'}).click();
    await page.locator('.mk-git-branch').waitFor(); await idle();
    check(await page.locator('.mk-git-head [data-command="git.sync"]').isVisible(),'Sync not primary');
  });
  await step('fetch and pull a remote change using the keyboard',async()=>{
    commit(other,body('remote one')); git(other,'push');
    await action('git.fetch',true,true);
    check(git(ROOT,'rev-parse','sync-test/main')===git(remote,'rev-parse','main'),'fetch failed');
    await action('git.pull',true,true);
    check(fs.readFileSync(path.join(ROOT,'sync.txt'),'utf8')===body('remote one'),'pull failed');
  });
  await step('push; rejected push offers pull; stale lease refuses',async()=>{
    commit(ROOT,body('local push')); await action('git.push');
    check(git(ROOT,'rev-parse','HEAD')===git(remote,'rev-parse','main'),'push failed');
    git(other,'pull','--ff-only'); commit(other,body('other push')); git(other,'push');
    commit(ROOT,body('diverged local')); await action('git.push');
    await page.locator('.mk-git-output').filter({hasText:'rejected'}).waitFor();
    check(await page.locator('.mk-git-sync').getByRole('button',{name:'Pull',exact:true}).isVisible(),'rejection has no Pull action');
    const remoteHead=git(remote,'rev-parse','main');
    await action('git.push-lease');
    check((await page.locator('.mk-git-output').textContent()).includes('stale info'),'lease did not refuse');
    check(git(remote,'rev-parse','main')===remoteHead,'lease overwrote remote');
  });
  await step('merge conflict, pick each hunk, continue and push with the mouse',async()=>{
    await action('git.pull',false);
    await page.getByText('Local and upstream branches have diverged.',{exact:false}).waitFor();
    await page.locator('#mk-git-confirm').click(); await idle(); await resolve('merge');
    await page.screenshot({path:`${process.env.M1_SHOTS}/git-sync-merge.png`});
  });
  await step('rebase conflict, pick each hunk, continue and push with the keyboard',async()=>{
    git(other,'pull','--ff-only'); commit(other,body('rebase remote')); git(other,'push');
    commit(ROOT,body('rebase local')); git(ROOT,'config','pull.rebase','true');
    await action('git.pull',true,true);
    await page.locator('.mk-git-conflict-view').waitFor();
    await page.screenshot({path:`${process.env.M1_SHOTS}/git-sync-rebase.png`});
    await resolve('rebase',true);
  });
  await step('abort restores the pre-pull tree and HEAD',async()=>{
    git(other,'pull','--ff-only'); commit(other,body('abort remote')); git(other,'push');
    commit(ROOT,body('abort local')); const head=git(ROOT,'rev-parse','HEAD');
    await action('git.pull'); await page.locator('.mk-git-conflict-view').waitFor();
    await page.locator('.mk-git-sync button[data-command="git.abort"]').click();
    await page.locator('.mk-git-sync button[data-command="git.abort"]').waitFor({state:'detached'});
    check(git(ROOT,'rev-parse','HEAD')===head,'abort moved HEAD');
    check(fs.readFileSync(path.join(ROOT,'sync.txt'),'utf8')===body('abort local'),'abort changed tree');
  });
  await step('delete/modify conflict keeps deletion and pushes',async()=>{
    git(ROOT,'reset','--hard','sync-test/main');
    git(ROOT,'rm','sync.txt'); git(ROOT,'commit','-qm','delete sync file');
    commit(other,body('modify deleted file')); git(other,'push');
    git(ROOT,'config','pull.rebase','false');
    await action('git.pull');
    await page.getByRole('button',{name:'Keep deletion and mark resolved',exact:true}).click();
    await page.locator('.mk-git-conflict-view').waitFor({state:'detached'}); await idle();
    const cont=page.locator('.mk-git-sync button[data-command="git.continue"]');
    await cont.click(); await cont.waitFor({state:'detached'});
    await action('git.push');
    check(!fs.existsSync(path.join(ROOT,'sync.txt')),'deletion became an empty file');
    check(git(ROOT,'rev-parse','HEAD')===git(remote,'rev-parse','main'),'deletion not pushed');
  });
  await step('publish a new branch with its upstream',async()=>{
    git(ROOT,'checkout','-qb','sync-published'); await refresh();
    await action('git.publish');
    check(git(ROOT,'rev-parse','--abbrev-ref','@{upstream}')==='sync-test/sync-published','publish upstream missing');
  });
  check(!errors.length,errors.join('\n'));
  console.log('GIT SYNC E2E: PASS');
} catch(e) {
  console.error('FAIL:',e.stack); console.error(errors.join('\n'));
  await page.screenshot({path:`${process.env.M1_SHOTS}/git-sync-fail.png`}).catch(()=>{}); process.exitCode=1;
} finally {
  await browser.close();
  for(const op of ['rebase','merge','cherry-pick']) { try{git(ROOT,op,'--abort');}catch{} }
  git(ROOT,'checkout','-f','main'); git(ROOT,'reset','--hard',initial);
  for(const key of ['branch.main.remote','branch.main.merge','pull.rebase','pull.ff']) { try{git(ROOT,'config','--unset',key);}catch{} }
  try{git(ROOT,'branch','-D','sync-published');}catch{}
  git(ROOT,'remote','remove','sync-test');
  fs.rmSync(scratch,{recursive:true,force:true});
}
