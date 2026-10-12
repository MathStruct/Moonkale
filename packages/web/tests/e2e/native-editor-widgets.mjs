// View-only block controls: focus/input isolation, height changes, source return and history.
import { chromium } from "playwright";
import assert from "node:assert/strict";
const browser=await chromium.launch();
const page=await browser.newPage({viewport:{width:1200,height:1000}});
page.setDefaultTimeout(15000);
const errors=[];page.on("pageerror",error=>errors.push(error.message));
const root=page.locator(".primary");
const sink=root.locator(".mk-editor-core-input-sink");
const widget=root.locator(".mk-preview-widget");
const toggle=widget.locator(".mk-preview-widget-toggle");
const note=widget.locator(".mk-preview-widget-note");
const control=id=>page.locator(`#${id}`).evaluate(button=>button.click());
const source=()=>page.locator(".canonical").textContent();
const selection=()=>root.locator(".mk-crust").evaluate(el=>[el.dataset.selectionAnchor,el.dataset.selectionHead]);
const aligned=()=>page.waitForFunction(()=>{
 const block=document.querySelector('.primary .mk-native-block-widget')?.getBoundingClientRect();
 const row=document.querySelector('.primary [data-line="1"]')?.getBoundingClientRect();
 return block&&row&&Math.abs(block.bottom-row.top)<1;
});
try {
 await page.goto(`http://127.0.0.1:${process.env.PORT??"8099"}/`);
 await root.locator("[data-editor=rust]").waitFor();await control("layout-document");await control("layout-widget");await widget.waitFor();await aligned();
 const original=await source();
 await sink.focus();await page.keyboard.press("Control+Home");await page.keyboard.insertText("X");
 await page.waitForFunction(expected=>document.querySelector(".canonical").textContent===expected,"X"+original);
 const caret=await selection();
 const before=await root.locator(".mk-native-block-widget").boundingBox();
 await toggle.click();await note.waitFor();await aligned();
 assert.equal(await toggle.getAttribute("aria-expanded"),"true");
 assert(await root.locator(".mk-native-block-widget").evaluate(el=>el.getBoundingClientRect().height)>before.height);
 await note.fill("local 😀中 <safe>");await page.keyboard.press("End");await page.keyboard.press("ArrowLeft");await page.keyboard.type("!");
 for(const key of ["Control+z","Control+Shift+z","Control+.","F2","ArrowDown","Home","End"])await page.keyboard.press(key);
 assert.equal(await source(),"X"+original);assert.deepEqual(await selection(),caret);
 // Clipboard commands belong to the widget; the editor must not prevent them or write source.
 for(const type of ["copy","cut","paste"]){
  assert.equal(await note.evaluate((el,type)=>{const data=new DataTransfer();data.setData("text/plain","CANARY");const event=new ClipboardEvent(type,{bubbles:true,cancelable:true,clipboardData:data});el.dispatchEvent(event);return event.defaultPrevented;},type),false);
 }
 assert.equal(await source(),"X"+original);
 await note.evaluate(el=>el.dispatchEvent(new KeyboardEvent("keydown",{key:"Escape",code:"Escape",isComposing:true,bubbles:true,cancelable:true})));
 assert(await note.evaluate(el=>document.activeElement===el));
 const local=await note.inputValue();
 await toggle.focus();await page.keyboard.press("Space");await note.waitFor({state:"hidden"});await aligned();
 await page.keyboard.press("Enter");await note.waitFor();assert.equal(await note.inputValue(),local);
 assert.deepEqual(await selection(),caret);assert.equal(await source(),"X"+original);
 assert.equal(await widget.locator("safe").count(),0);
 console.log("ok: native controls own focus, typing, navigation, clipboard and composition Escape; content and source stay separate");
 await control("duplicate");await page.locator(".duplicate .mk-preview-widget").waitFor();
 const sibling=page.locator(".duplicate .mk-preview-widget");
 assert.notEqual(await toggle.getAttribute("aria-controls"),await sibling.locator(".mk-preview-widget-toggle").getAttribute("aria-controls"));
 await sibling.locator(".mk-preview-widget-toggle").click();await sibling.locator(".mk-preview-widget-note").fill("sibling only");
 assert.equal(await note.inputValue(),local);assert.deepEqual(await selection(),caret);assert.equal(await source(),"X"+original);
 await control("duplicate");await sibling.waitFor({state:"detached"});
 console.log("ok: duplicate panes have independent widget state and unique accessibility targets");
 await note.focus();await page.keyboard.press("Escape");
 await page.waitForFunction(()=>document.activeElement===document.querySelector('.primary .mk-editor-core-input-sink'));
 await page.waitForFunction(anchor=>document.querySelector('.primary .mk-native-surface').dataset.cursorOffset===anchor,await root.locator(".mk-native-block-widget").getAttribute("data-source-anchor"));
 await page.keyboard.insertText("Y");await page.waitForFunction(expected=>document.querySelector(".canonical").textContent===expected, "X"+original.replace("// row 1", "Y// row 1"));
 await page.keyboard.press("Control+z");await page.waitForFunction(expected=>document.querySelector(".canonical").textContent===expected,"X"+original);
 await page.keyboard.press("Control+z");await page.waitForFunction(expected=>document.querySelector(".canonical").textContent===expected,original);
 console.log("ok: Escape returns to the declared source anchor and document undo ignores widget interactions");
 await control("layout-delay");await toggle.click();await note.waitFor();await toggle.click();await note.waitFor({state:"hidden"});
 await aligned();await page.waitForTimeout(450);await aligned();
 const compact=await root.locator(".mk-native-block-widget").boundingBox();
 await toggle.click();await note.waitFor();await aligned();
 await root.evaluate(el=>el.style.width="480px");await aligned();
 assert(await root.locator(".mk-native-block-widget").evaluate(el=>el.getBoundingClientRect().height)>compact.height);
 const viewport=root.locator(".mk-editor-core-viewport");await viewport.evaluate(el=>el.scrollTop=30);await toggle.click();await aligned();
 assert(Math.abs(await viewport.evaluate(el=>el.scrollTop)-30)<1);
 await widget.locator(".mk-preview-widget-source").click();await page.waitForFunction(()=>document.activeElement===document.querySelector('.primary .mk-editor-core-input-sink'));
 assert.equal(await source(),original);
 console.log("ok: content resize, delayed toggle measurements and partial scrolling preserve source layout; Edit source restores editor focus");
 await control("layout-uniform");await widget.waitFor({state:"detached"});await page.waitForTimeout(450);assert.equal(await root.locator(".mk-native-block-widget").count(),0);assert.equal(await source(),original);
 if(errors.length)throw Error(errors.join("\n"));
} finally {await browser.close();}
