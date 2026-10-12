// Canonical UTF-16 coordinates across the merged CodeMirror CRLF boundary.
import {chromium} from 'playwright';
import assert from 'node:assert/strict';
import {resolve} from 'node:path';
const browser=await chromium.launch();
const page=await browser.newPage();
try {
  await page.setContent('<div id="editor"></div>');
  await page.addScriptTag({path:process.env.CM_BUNDLE??resolve('packages/code-view/assets/codemirror.js')});
  for(const separator of ['\n','\r\n']) {
    const source=['a','😀b','c'].join(separator);
    await page.evaluate(source=>{
      window.cursor=null;window.splices=[];
      const el=document.querySelector('#editor');
      window.moonkale.codemirror.mount(el,source,(changes,length)=>window.splices.push({changes,length}),{onCursor:(line,col,anchor,head)=>{window.cursor={line,col,anchor,head};}});
      window.moonkale.codemirror.setCursor(el,2,0);
    },source);
    const offset=source.indexOf('c');
    await page.waitForFunction(offset=>window.cursor?.anchor===offset,offset);
    assert.deepEqual(await page.evaluate(()=>window.cursor),{line:2,col:0,anchor:offset,head:offset});
    await page.keyboard.insertText('X');
    const inserted=source.slice(0,offset)+'X'+source.slice(offset);
    assert.equal(await page.evaluate(()=>window.moonkale.codemirror.getText(document.querySelector('#editor'))),inserted);
    assert.deepEqual(await page.evaluate(()=>window.splices[0]),{changes:[{from:offset,to:offset,insert:'X'}],length:inserted.length});
    await page.keyboard.press('Control+z');
    assert.equal(await page.evaluate(()=>window.moonkale.codemirror.getText(document.querySelector('#editor'))),source);
  }
  console.log('PASS: CodeMirror canonical LF/CRLF selection, Unicode splice offsets and undo');
} finally {await browser.close();}
