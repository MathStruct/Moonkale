// CodeMirror's public history protocol, using the committed bundle.
import { chromium } from 'playwright';
const browser = await chromium.launch();
const page = await browser.newPage();
try {
  await page.setContent('<div id="editor"></div>');
  await page.addScriptTag({path: `${process.env.MOONKALE_REPO}/packages/code-view/assets/codemirror.js`});
  await page.evaluate(() => {
    const cm = window.moonkale.codemirror, el = document.querySelector('#editor');
    window.events = [];
    const mount = () => cm.mount(el, 'old', () => {}, {sessionKey:'test', epoch:0, sequence:0, onGroupHistory:(group,redo) => events.push({group,redo})});
    const request = (phase,direction,text,epoch,token='1') => cm.groupSession('test',{phase,direction,text,epoch,sequence:0,group:'42',token});
    window.check = (ok, message) => { if (!ok) throw Error(message); };
    window.request = request;
    mount();
    check(request('prepare','apply','old',0), 'prepare apply');
    check(request('commit','apply','new',1), 'commit apply');
    request('cancel','apply','new',1);
    check(request('prepare','undo','new',1,'2'), 'prepare undo');
    check(cm.getText(el) === 'new', 'history probe mutated text');
    request('cancel','undo','new',1,'2');
    cm.undo(el);
    check(cm.getText(el) === 'new' && events.length === 1, 'menu partially undid group');
    cm.destroy(el); cm.mount(el,'new',()=>{},{sessionKey:'test',epoch:1,sequence:0,onGroupHistory:(group,redo)=>events.push({group,redo})});
    cm.focus(el);
  });
  await page.keyboard.press('Control+z');
  await page.evaluate(() => {
    const cm=window.moonkale.codemirror, el=document.querySelector('#editor');
    check(events.length === 2 && events.every(e=>e.group==='42' && !e.redo), 'keyboard/remount lost group');
    check(cm.getText(el)==='new','keyboard partially undid group');
    check(request('prepare','undo','new',1,'3'),'prepare remounted undo');
    check(!request('prepare','undo','new',1,'9'),'overlapping hold accepted');
    request('cancel','undo','new',1,'0');
  });
  await page.keyboard.type('lost input');
  await page.keyboard.press('Control+z');
  await page.evaluate(() => {
    const cm=window.moonkale.codemirror, el=document.querySelector('#editor');
    check(cm.getText(el)==='new' && events.length===2,'hold accepted input or stale cancel');
    check(request('commit','undo','old',2,'3'),'commit undo');
    request('cancel','undo','old',2,'3');
    cm.redo(el);
    check(cm.getText(el)==='old' && events.at(-1).redo,'redo partially restored group');
    check(request('prepare','redo','old',2,'4'),'prepare redo');
    check(cm.getText(el)==='old','redo probe mutated text');
    check(request('commit','redo','new',3,'4'),'commit redo');
    request('cancel','redo','new',3,'4');
    request('cancel','undo','new',3,'5');
    check(!request('prepare','undo','new',3,'5'),'late prepare crossed cancellation');
    // Repair a remounted view after a lost commit acknowledgement, while
    // keeping a retired marker so local undo can never restore one file alone.
    request('cancel','apply','recovered',4,'6');
    check(cm.getText(el)==='recovered','lost commit did not resynchronize');
    cm.undo(el);
    check(cm.getText(el)==='recovered' && events.at(-1).group==='42','recovery lost its fail-closed marker');
    cm.destroy(el); cm.dropSession('test');
    check(!request('prepare','undo','new',3),'closed session retained history');
  });
  console.log('GROUP HISTORY E2E: PASS');
} catch (error) {console.error('FAIL:',error); process.exitCode=1;} finally {await browser.close();}
