import { chromium } from "playwright";
const browser=await chromium.launch();
const page=await browser.newPage({viewport:{width:1400,height:900}});page.setDefaultTimeout(15000);
const errors=[];page.on("pageerror",error=>errors.push(error.message));
const root=page.locator(".primary [data-editor=rust]");
const input=()=>root.locator(".mk-editor-core-input-sink");
const prompt=()=>root.locator(".mk-native-rename-prompt");
const canonical=()=>page.locator(".canonical").textContent();
const log=async()=>JSON.parse(await page.locator(".lsp-log").textContent()).map(JSON.parse);
const requests=async()=>(await log()).filter(entry=>entry.method==="textDocument/rename");
const mode=value=>page.locator(`[data-rename-mode="${value}"]`).evaluate(button=>button.click());
const control=id=>page.locator(`#${id}`).evaluate(button=>button.click());
const waitText=text=>page.waitForFunction(text=>document.querySelector(".canonical").textContent===text,text);
const status=text=>page.waitForFunction(text=>document.querySelector(".definition-status").textContent.includes(text),text);
const waitCount=count=>page.waitForFunction(count=>JSON.parse(document.querySelector(".lsp-log").textContent).map(JSON.parse).filter(entry=>entry.method==="textDocument/rename").length>=count,count);
const cancel=id=>page.waitForFunction(id=>JSON.parse(document.querySelector(".lsp-log").textContent).map(JSON.parse).some(entry=>entry.method==="$/cancelRequest"&&entry.params.id===id),id);
const origin=async()=>{await control("definition-origin");await input().focus();await page.keyboard.press("Control+Home");for(let n=0;n<3;n++)await page.keyboard.press("ArrowRight");};
const fixture=async()=>{await control("rename-fixture");await waitText("😀old old\r\n");await origin();};
const open=async(toolbar=false)=>{if(toolbar)await root.locator(".mk-native-rename").click();else{await input().focus();await page.keyboard.press("F2");}await prompt().waitFor();};
const submit=async(name,mouse=false)=>{await prompt().locator("input").fill(name);if(mouse)await prompt().locator(".mk-native-rename-confirm").click();else await prompt().locator("input").press("Enter");};
const step=async(name,fn)=>{await fn();console.log(`ok: ${name}`);};
try{
 await page.goto(`http://127.0.0.1:${process.env.PORT??"8099"}/`);await root.waitFor();
 await page.waitForFunction(()=>JSON.parse(document.querySelector(".lsp-log").textContent).map(JSON.parse).some(entry=>entry.method==="textDocument/didOpen"));
 await fixture();
 await step("F2 prefilled accessible prompt, validation, Escape and unchanged-name dismissal",async()=>{
  await mode("normal");const before=(await requests()).length;await open();
  if(await prompt().locator("input").inputValue()!=="old")throw Error("Prompt did not use symbol at UTF-16 caret");
  await prompt().locator("input").evaluate(input=>{if(document.activeElement!==input)throw Error("Prompt did not receive focus");});
  await prompt().locator("input").fill(" ");if(!await prompt().locator(".mk-native-rename-confirm").isDisabled())throw Error("Blank name allowed");
  await prompt().locator("input").press("Enter");if((await requests()).length!==before)throw Error("Blank name sent request");
  await prompt().locator("input").press("Escape");await prompt().waitFor({state:"detached"});
  await input().evaluate(input=>{if(document.activeElement!==input)throw Error("Escape did not restore focus");});
  await open(true);await submit("old");await prompt().waitFor({state:"detached"});if((await requests()).length!==before)throw Error("Unchanged name sent request");
 });
 await step("Unicode rename requests and grouped CRLF edits undo/redo",async()=>{
  await open();await submit("longer");await waitText("😀longer longer\r\n");await prompt().waitFor({state:"detached"});
  const request=(await requests()).at(-1);if(request.params.position.line!==0||request.params.position.character!==4||request.params.newName!=="longer")throw Error("Incorrect rename position or name");
  await page.waitForFunction(()=>document.querySelector('.primary .mk-crust')?.dataset.selectionHead==="8");
  await input().focus();await page.keyboard.press("Control+z");await waitText("😀old old\r\n");await page.keyboard.press("Control+Shift+z");await waitText("😀longer longer\r\n");
  await fixture();await mode("versioned");await open(true);await submit("name",true);await waitText("😀name name\r\n");await status("Applied edits to 1 file");
 });
 await step("all target edits validate before changing any document",async()=>{
  for(const [value,message] of [["invalid","column outside line"],["overlap","overlapping"],["outside","outside the folder"],["malformed","invalid text edit"],["stale-version","version is stale"],["missing","not found"],["error","rename error"],["empty","Nothing to rename"]]){
   await fixture();await mode(value);await open();await submit("name");await status(message);await prompt().waitFor({state:"detached"});
   if(await canonical()!=="😀old old\r\n")throw Error(`${value} partially changed origin`);
   if(await page.locator(".rename-target-canonical").textContent()!=="")throw Error(`${value} opened a target`);
  }
 });
 await step("cross-file rename loads the right source as unsaved text and retains per-file undo",async()=>{
  await fixture();await mode("cross");await open(true);await submit("renamed",true);await waitText("😀renamed renamed\r\n");await status("Applied edits to 2 files");
  if(await page.locator(".rename-target-canonical").textContent()!=="// target\r\n😀renamed\r\n")throw Error("Cross-file target incorrect");
  if(await page.locator(".definition-target").count())throw Error("Rename changed active document");
  const targetUri="file:///tmp/native-fixture/defs/target%20%23%C3%A9.rs";
  const opens=(await log()).filter(entry=>entry.method==="textDocument/didOpen"&&entry.params.textDocument.uri===targetUri);
  if(opens.length!==1||opens[0].params.textDocument.text!=="// target\r\n😀renamed\r\n")throw Error("Undisplayed rename target was not synchronized");
  await input().focus();await page.keyboard.press("Control+z");await waitText("😀old old\r\n");
  await control("rename-target-show");const target=page.locator(".definition-target");await target.waitFor();await target.locator(".mk-editor-core-input-sink").focus();await page.keyboard.press("Control+z");
  await page.waitForFunction(()=>document.querySelector(".rename-target-canonical").textContent==="// target\r\n😀value\r\n");
  await page.waitForFunction(uri=>JSON.parse(document.querySelector(".lsp-log").textContent).map(JSON.parse).some(entry=>entry.method==="textDocument/didChange"&&entry.params.textDocument.uri===uri&&entry.params.contentChanges[0].text==="// target\r\n😀value\r\n"),targetUri);
  await page.keyboard.press("Control+End");await page.keyboard.type("x");await origin();await mode("cross");await open();await submit("dirty");await waitText("😀dirty dirty\r\n");
  if(await page.locator(".rename-target-canonical").textContent()!=="// target\r\n😀dirty\r\nx")throw Error("Rename overwrote dirty target");
  await control("definition-close");
  await page.waitForFunction(uri=>JSON.parse(document.querySelector(".lsp-log").textContent).map(JSON.parse).some(entry=>entry.method==="textDocument/didClose"&&entry.params.textDocument.uri===uri),targetUri);
 });
 await step("pending rename cancels on Escape, edits, cursor/tab changes, other-file edits and unmount",async()=>{
  for(const action of ["escape","edit","cursor","tab","other","unmount"]){
   await fixture();await mode("slow");const before=(await requests()).length;await open();await submit("late");await waitCount(before+1);const id=(await requests()).at(-1).id;
   if(action==="escape")await page.keyboard.press("Escape");
   if(action==="edit"){await input().focus();await page.keyboard.type("x");}
   if(action==="cursor"){await input().focus();await page.keyboard.press("ArrowRight");}
   if(action==="tab")await page.locator('[data-language-fixture="nested.html"]').click();
   if(action==="other")await control("rename-other-edit");
   if(action==="unmount"){await control("mount");await root.waitFor({state:"detached"});}
   await cancel(id);await control("rename-release");
   if(action==="unmount"){await control("mount");await root.waitFor();}
   if((await canonical()).includes("late"))throw Error(`Stale rename applied after ${action}`);
  }
 });
 await step("file-load cancellation and timeout cannot apply partial rename; recovery works",async()=>{
  await fixture();await control("definition-hold-load");await mode("cross");const before=(await log()).filter(entry=>entry.method==="fixture/fetchDefinition").length;await open();await submit("late");
  await page.waitForFunction(count=>JSON.parse(document.querySelector(".lsp-log").textContent).map(JSON.parse).filter(entry=>entry.method==="fixture/fetchDefinition").length>count,before);
  await input().focus();await page.keyboard.press("ArrowRight");await control("definition-release-load");await prompt().waitFor({state:"detached"});
  if(await canonical()!=="😀old old\r\n"||await page.locator(".rename-target-canonical").textContent()!=="")throw Error("Canceled load partially renamed");
  await origin();await mode("slow");const count=(await requests()).length;await open();await submit("late");await waitCount(count+1);const id=(await requests()).at(-1).id;await status("timed out");await cancel(id);await control("rename-release");
  if(await canonical()!=="😀old old\r\n")throw Error("Timed-out rename applied");
  await mode("normal");await open();await submit("recovered");await waitText("😀recovered recovered\r\n");
 });
 await step("retained document synchronization keeps completion usable",async()=>{
  await control("completion-unicode");await waitText("😀pri\r\n");await input().focus();await page.keyboard.press("Control+Home");await page.keyboard.press("End");
  await page.locator('[data-completion-mode="normal"]').evaluate(button=>button.click());await page.keyboard.press("Control+Space");await root.locator(".mk-native-completions").waitFor();
  await page.keyboard.press("Enter");await waitText("😀printAlpha\r\n");
 });
 if(errors.length)throw Error(errors.join("\n"));
}finally{await browser.close();}
