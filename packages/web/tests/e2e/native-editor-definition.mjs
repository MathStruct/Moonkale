import { chromium } from "playwright";
const browser=await chromium.launch();
const page=await browser.newPage({viewport:{width:1200,height:800}});page.setDefaultTimeout(15000);
const errors=[];page.on("pageerror",error=>errors.push(error.message));
const root=page.locator(".primary [data-editor=rust]");
const target=page.locator(".definition-target [data-editor=rust]");
const input=()=>root.locator(".mk-editor-core-input-sink");
const mode=value=>page.locator(`[data-definition-mode="${value}"]`).evaluate(button=>button.click());
const log=async()=>JSON.parse(await page.locator(".lsp-log").textContent()).map(JSON.parse);
const requests=async()=>(await log()).filter(entry=>entry.method==="textDocument/definition");
const count=async()=>(await requests()).length;
const waitCount=count=>page.waitForFunction(count=>JSON.parse(document.querySelector(".lsp-log").textContent).map(JSON.parse).filter(entry=>entry.method==="textDocument/definition").length>=count,count);
const cancel=id=>page.waitForFunction(id=>JSON.parse(document.querySelector(".lsp-log").textContent).map(JSON.parse).some(entry=>entry.method==="$/cancelRequest"&&entry.params.id===id),id);
const release=()=>page.locator("#definition-release").evaluate(button=>button.click());
const status=text=>page.waitForFunction(text=>document.querySelector(".definition-status")?.textContent.includes(text),text);
const trigger=async()=>{await input().focus();await page.keyboard.press("F12");};
const origin=async()=>{await page.locator("#definition-origin").evaluate(button=>button.click());await input().focus();await page.keyboard.press("Control+Home");};
const close=async()=>{await page.locator("#definition-close").evaluate(button=>button.click());await target.waitFor({state:"detached"});await origin();};
const selected=(where,head)=>page.waitForFunction(({where,head})=>document.querySelector(`${where} .mk-crust`)?.dataset.selectionHead===`${head}`,{where,head});
const step=async(name,fn)=>{await fn();console.log(`ok: ${name}`);};
try{
 await page.goto(`http://127.0.0.1:${process.env.PORT??"8099"}/`);await root.waitFor();
 await page.waitForFunction(()=>JSON.parse(document.querySelector(".lsp-log").textContent).map(JSON.parse).some(entry=>entry.method==="textDocument/didOpen"));
 const original=await page.locator(".canonical").textContent();
 await step("definition works at the freshly mounted caret",async()=>{
  await mode("empty");const before=await count();await trigger();await waitCount(before+1);await status("No definition");
 });
 await step("opening a sibling pane does not restart definition",async()=>{
  await mode("slow");const before=await count();await trigger();await waitCount(before+1);const id=(await requests()).at(-1).id;
  await page.click("#python");await page.waitForTimeout(150);
  if(await count()!==before+1)throw Error("Opening a sibling restarted definition");
  await input().focus();await page.keyboard.press("Escape");await cancel(id);await release();await page.click("#python");
  if(await target.count())throw Error("Canceled sibling-pane request opened a target");
 });
 await step("F12 and toolbar navigate in-file with Unicode coordinates and no edits",async()=>{
  await mode("same");await input().focus();await page.keyboard.press("Control+Home");await page.keyboard.press("ArrowDown");await page.keyboard.press("Home");for(let n=0;n<9;n++)await page.keyboard.press("ArrowRight");
  await trigger();await selected(".primary",21);
  if((await requests()).at(-1).params.position.character!==10)throw Error("Request was not UTF-16 after the emoji");
  if(await page.locator(".canonical").textContent()!==original)throw Error("Definition navigation edited text");
  await origin();await root.locator(".mk-native-definition").click();await selected(".primary",21);
  await input().evaluate(input=>{if(document.activeElement!==input)throw Error("Definition did not restore focus");});
  await mode("array");await origin();await trigger();await selected(".primary",21);
 });
 await step("cross-file resolution uses the correct folder and decodes URI paths",async()=>{
  await mode("cross");await origin();await trigger();await target.waitFor();await selected(".definition-target",13);
  if(await page.locator(".definition-source").textContent()!=="folder:/tmp/native-fixture")throw Error("Opened the wrong folder's target");
  if(await page.locator(".definition-canonical").textContent()!=="// target\r\n😀value\r\n")throw Error("Loaded the wrong file");
  await target.locator(".mk-editor-core-input-sink").evaluate(input=>{if(document.activeElement!==input)throw Error("Cross-file definition did not focus target");});
  await page.keyboard.press("Control+End");await page.keyboard.type("x");
  const fetches=(await log()).filter(entry=>entry.method==="fixture/fetchDefinition").length;
  await mode("link");await origin();await trigger();await selected(".definition-target",13);
  if(!(await page.locator(".definition-canonical").textContent()).endsWith("x"))throw Error("Reopening overwrote dirty target text");
  if((await log()).filter(entry=>entry.method==="fixture/fetchDefinition").length!==fetches)throw Error("Dirty target was fetched again");
  await close();
 });
 await step("same-file definition reveals folded targets",async()=>{
  await page.click("#folds");await root.locator('[data-fold-line="0"]').waitFor();await root.locator('[data-fold-line="0"]').click();
  await mode("same");await origin();await trigger();
  await page.waitForFunction(()=>document.querySelector('.primary [data-fold-line="0"]')?.getAttribute("aria-expanded")==="true");
 });
 await step("errors, missing files and out-of-folder targets leave navigation unchanged",async()=>{
  for(const [value,message] of [["empty","No definition"],["error","definition error"],["outside","outside the folder"],["malformed","outside the folder"],["missing","not found"]]){
   await mode(value);await origin();await trigger();await status(message);if(await target.count())throw Error(`${value} opened a target`);
  }
 });
 await step("late replies are canceled after Escape, cursor movement, edits, tab changes and unmount",async()=>{
  for(const action of ["escape","cursor","edit","tab","unmount"]){
   await mode("slow");await origin();const before=await count();await trigger();await waitCount(before+1);const id=(await requests()).at(-1).id;
   if(action==="escape")await page.keyboard.press("Escape");
   if(action==="cursor")await page.keyboard.press("ArrowRight");
   if(action==="edit")await page.keyboard.type("x");
   if(action==="tab")await page.locator('[data-language-fixture="nested.html"]').click();
   if(action==="unmount"){await page.locator("#mount").evaluate(button=>button.click());await root.waitFor({state:"detached"});}
   await cancel(id);await release();
   if(action==="unmount"){await page.locator("#mount").evaluate(button=>button.click());await root.waitFor();}
   if(action==="tab")await page.locator('[data-language-fixture="nested.html"]').click();
   if(await target.count())throw Error(`Stale target opened after ${action}`);
   if(action==="edit"){await origin();await page.click("#undo");}
  }
 });
 await step("a canceled file load cannot open a target; timeout remains recoverable",async()=>{
  await close();await page.locator("#definition-hold-load").evaluate(button=>button.click());await mode("cross");const fetches=(await log()).filter(entry=>entry.method==="fixture/fetchDefinition").length;await trigger();
  await page.waitForFunction(count=>JSON.parse(document.querySelector(".lsp-log").textContent).map(JSON.parse).filter(entry=>entry.method==="fixture/fetchDefinition").length>count,fetches);
  await page.keyboard.press("ArrowRight");await page.locator("#definition-release-load").evaluate(button=>button.click());await page.waitForTimeout(100);
  if(await target.count())throw Error("Canceled file load opened a target");
  await mode("slow");await origin();const before=await count();await trigger();await waitCount(before+1);const id=(await requests()).at(-1).id;await status("timed out");await cancel(id);await release();if(await target.count())throw Error("Timed-out definition resurfaced");
  await mode("cross");await trigger();await target.waitFor();await selected(".definition-target",13);
 });
 if(errors.length)throw Error(errors.join("\n"));
}finally{await browser.close();}
