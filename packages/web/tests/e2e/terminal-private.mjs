import assert from 'node:assert/strict';
import { firefox } from 'playwright';
import { control } from './chrome-controls.mjs';
const browser = await firefox.launch();
let page;
try {
 for (const implementation of ['xterm','native']) {
  const context = await browser.newContext({viewport:{width:1400,height:900}});
  await context.addInitScript(implementation=>localStorage.setItem('moonkale.settings',JSON.stringify({language:'en',extensions:{enabled:implementation==='native'?['dev.moonkale.editor-terminal-native']:[]},terminal:{implementation}})),implementation);
  page = await context.newPage();
  const terminal=implementation==='native'?'.mk-tn-screen:not(.mk-tn-hidden)':'.mk-term-session:not(.mk-term-hidden)';
  const screen=implementation==='native'?'.mk-tn-grid':'.xterm-rows';
  const sharing=implementation==='native'?'.mk-tn-sharing':'.mk-term-sharing';
  const command=async text=>{await control(page,'click',terminal);await page.keyboard.type(text);await page.keyboard.press('Enter');};
  await page.goto(`http://127.0.0.1:${process.env.PORT ?? 8080}/`,{waitUntil:'networkidle'});
  await page.waitForSelector('.wb-workspace');
  await control(page,'click','.mk-explorer-open button[type=submit]');
  await page.waitForFunction(()=>document.querySelector('.wb-status-bar')?.textContent.includes('index:'));
  await control(page,'click',".mk-menu-button:has-text('View')");
  await control(page,'click',".mk-menu-item:has-text('New Terminal')");
  await page.waitForSelector(`${terminal} ${screen}`);
  await command("stty -echo; printf 'Password: '; read password; unset password; stty echo; printf '\\nfinished\\n'");
  await page.waitForSelector('.mk-term-private');
  assert.match(await page.locator('.mk-term-private').textContent(),/🔒/);
  assert.equal(await page.locator('.mk-term-private').getAttribute('aria-label'),'Password prompt');
  // Paste is intercepted before either input encoder can send it.
  await page.locator(terminal).evaluate(el=>{
    const data=new DataTransfer();data.setData('text/plain','private_paste_71');
    el.dispatchEvent(new ClipboardEvent('paste',{bubbles:true,cancelable:true,clipboardData:data}));
  });
  await page.getByRole('alertdialog').waitFor();
  await page.getByRole('alertdialog').getByRole('button',{name:'Cancel',exact:true}).click();
  await page.locator(terminal).evaluate(el=>{
    const data=new DataTransfer();data.setData('text/plain','accepted_paste_71');
    el.dispatchEvent(new ClipboardEvent('paste',{bubbles:true,cancelable:true,clipboardData:data}));
  });
  await page.getByRole('alertdialog').getByRole('button',{name:'Paste',exact:true}).click();

  await control(page,'click',terminal);
  await page.keyboard.type('private_marker_71');
  await page.keyboard.press('Control+Alt+Space');
  await page.waitForSelector('.mk-overview');
  assert.match(await page.locator('.mk-overview').textContent(),/Password prompt/);
  assert.doesNotMatch(await page.locator('.mk-overview').textContent(),/private_marker_71|private_paste_71|accepted_paste_71|Password: /);
  await page.screenshot({path:`${process.env.M1_SHOTS??'.'}/terminal-private-${implementation}.png`});
  await page.keyboard.press('Escape');
  await control(page,'click',terminal);await page.keyboard.press('Enter');
  await page.waitForFunction(()=>!document.querySelector('.mk-term-private'));
  assert.doesNotMatch(await page.locator(`${terminal} ${screen}`).textContent(),/private_marker_71|private_paste_71|accepted_paste_71/);

  // Ending the prompt revokes pending consent rather than pasting into a shell.
  await command("stty -echo; printf 'Temporary private prompt: '; sleep 3; stty echo");
  await page.waitForSelector('.mk-term-private');
  await page.locator(terminal).evaluate(el=>{
    const data=new DataTransfer();data.setData('text/plain','stale_paste_71');
    el.dispatchEvent(new ClipboardEvent('paste',{bubbles:true,cancelable:true,clipboardData:data}));
  });
  await page.getByRole('alertdialog').waitFor();
  await page.getByRole('alertdialog').waitFor({state:'hidden'});
  assert.doesNotMatch(await page.locator(`${terminal} ${screen}`).textContent(),/stale_paste_71/);
  // A prompt starting while Overview is already open revokes the snapshot.
  await command("sleep 1; stty -echo; printf 'Hidden prompt: '; read password; unset password; stty echo");
  await page.keyboard.press('Control+Alt+Space');
  await page.waitForSelector('.mk-overview');
  await page.waitForFunction(()=>document.querySelector('.mk-overview')?.textContent.includes('Password prompt'));
  await page.keyboard.press('Escape');await control(page,'click',terminal);await page.keyboard.press('Enter');
  await page.waitForFunction(()=>!document.querySelector('.mk-term-private'));
  await control(page,'click',sharing);
  await page.keyboard.press('Control+Alt+Space');await page.waitForSelector('.mk-overview');
  assert.match(await page.locator('.mk-overview').textContent(),/Sharing paused/);
  console.log(`private terminal ${implementation}: prompt, cancelled paste, revoked snapshot and pause pass`);
  await context.close();
 }
 console.log('TERMINAL PRIVATE E2E: PASS');
} catch(e) {await page?.screenshot({path:`${process.env.M1_SHOTS??'.'}/terminal-private-failure.png`}).catch(()=>{});throw e;} finally {await browser.close();}
