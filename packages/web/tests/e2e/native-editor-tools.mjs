import { chromium } from "playwright";
const browser=await chromium.launch();
const page=await browser.newPage({viewport:{width:1500,height:950}});page.setDefaultTimeout(15000);
const errors=[];page.on("pageerror",error=>errors.push(error.message));
const root=page.locator(".primary [data-editor=rust]");
const input=()=>root.locator(".mk-editor-core-input-sink");
const menu=()=>root.locator(".mk-native-tools");
const rows=()=>menu().locator(".mk-native-tool-item");
const control=id=>page.locator(`#${id}`).evaluate(button=>button.click());
const mode=(kind,value)=>page.locator(`[data-${kind}-mode="${value}"]`).evaluate(button=>button.click());
const log=async()=>JSON.parse(await page.locator(".lsp-log").textContent()).map(JSON.parse);
const requests=async(method)=>(await log()).filter(entry=>entry.method===method);
const waitCount=(method,count)=>page.waitForFunction(({method,count})=>JSON.parse(document.querySelector(".lsp-log").textContent).map(JSON.parse).filter(entry=>entry.method===method).length>=count,{method,count});
const cancel=id=>page.waitForFunction(id=>JSON.parse(document.querySelector(".lsp-log").textContent).map(JSON.parse).some(entry=>entry.method==="$/cancelRequest"&&entry.params.id===id),id);
const status=text=>page.waitForFunction(text=>document.querySelector(".definition-status").textContent.includes(text),text);
const text=()=>page.locator(".canonical").textContent();
const waitText=text=>page.waitForFunction(text=>document.querySelector(".canonical").textContent===text,text);
const selected=(where,head)=>page.waitForFunction(({where,head})=>document.querySelector(`${where} .mk-crust`)?.dataset.selectionHead===`${head}`,{where,head});
const origin=async()=>{await control("definition-origin");await input().focus();await page.keyboard.press("Control+Home");for(let n=0;n<3;n++)await page.keyboard.press("ArrowRight");};
const fixture=async()=>{await control("rename-fixture");await waitText("😀old old\r\n");await origin();};
const trigger=async(kind,toolbar=false)=>{if(toolbar)await root.locator(kind==="actions"?".mk-native-code-actions":".mk-native-find-references").click();else{await input().focus();await page.keyboard.press(kind==="actions"?"Control+.":"Shift+F12");}};
const loaded=count=>page.waitForFunction(count=>document.querySelector('.primary .mk-native-tools')?.getAttribute("aria-busy")==="false"&&document.querySelector('.primary .mk-native-tools')?.dataset.count===`${count}`,count);
const dismiss=async()=>{if(await menu().count()){await page.keyboard.press("Escape");await menu().waitFor({state:"detached"});}};
const method=kind=>kind==="actions"?"textDocument/codeAction":kind==="references"?"textDocument/references":"codeAction/resolve";
const step=async(name,fn)=>{await fn();console.log(`ok: ${name}`);};
try{
 await page.goto(`http://127.0.0.1:${process.env.PORT??"8099"}/`);await root.waitFor();await waitCount("textDocument/didOpen",1);
 await fixture();
 await step("Mod-. uses the selected UTF-16 range and diagnostics; preferred/disabled actions are keyboard accessible",async()=>{
  await control("tools-diagnostics");await root.locator(".mk-native-diagnostic-message").waitFor();
  await input().focus();await page.keyboard.press("End");for(let n=0;n<7;n++)await page.keyboard.press("Shift+ArrowLeft");
  await mode("actions","normal");await trigger("actions");await loaded(4);
  const request=(await requests(method("actions"))).at(-1);
  const {start,end}=request.params.range; if(start.line!==0||start.character!==2||end.line!==0||end.character!==9)throw Error(`Selection was not normalized UTF-16: ${JSON.stringify(request.params.range)}`);
  if(request.params.context.diagnostics.length!==1||request.params.context.diagnostics[0].message!=="Fix old")throw Error("Diagnostic context missing");
  const labels=await rows().allTextContents();if(!labels[0].includes("Preferred")||!labels[2].includes("<safe>"))throw Error("Preference ordering or escaped labels failed");
  if(!await rows().nth(1).isDisabled())throw Error("Disabled action allowed");
  await rows().nth(0).evaluate(button=>{if(document.activeElement!==button)throw Error("Actions did not receive focus");});
  await page.keyboard.press("ArrowDown");await rows().nth(2).evaluate(button=>{if(document.activeElement!==button)throw Error("Keyboard did not skip disabled action");});
  await page.keyboard.press("Enter");await waitText("😀fixed fixed\r\n");await input().focus();await page.keyboard.press("Control+z");await waitText("😀old old\r\n");await page.keyboard.press("Control+Shift+z");await waitText("😀fixed fixed\r\n");
 });
 await step("switching tools preserves prompt focus and keyboard Cancel does not apply an action",async()=>{
  await fixture();await trigger("actions");await loaded(4);await root.locator(".mk-native-rename").click();await root.locator(".mk-native-rename-prompt").waitFor();
  await page.waitForFunction(()=>document.activeElement===document.querySelector('.primary .mk-native-rename-prompt input'));
  if(await menu().count())throw Error("Rename did not dismiss actions");await page.keyboard.press("Escape");await root.locator(".mk-native-rename-prompt").waitFor({state:"detached"});
  await trigger("actions");await loaded(4);await menu().locator(".mk-native-tools-header .mk-btn").focus();await page.keyboard.press("Enter");await menu().waitFor({state:"detached"});
  if(await text()!=="😀old old\r\n")throw Error("Keyboard Cancel applied an action");
 });
 await step("toolbar action resolution preserves metadata, groups edits, rejects malformed results and accepts declared edits with commands",async()=>{
  await fixture();await mode("resolve","normal");await trigger("actions",true);await loaded(4);await rows().nth(0).click();await waitText("😀resolved resolved\r\n");
  const resolved=(await requests(method("resolve"))).at(-1);if(resolved.params.data.token!==42)throw Error("Resolve metadata was lost");
  await input().focus();await page.keyboard.press("Control+z");await waitText("😀old old\r\n");
  for(const [value,message] of [["error","tools error"],["invalid","invalid text edit"]]){
   await fixture();await mode("resolve",value);await trigger("actions");await loaded(4);await rows().nth(0).click();await status(message);if(await text()!=="😀old old\r\n")throw Error(`${value} partially applied`);
  }
  await fixture();await mode("resolve","command");await trigger("actions");await loaded(4);await rows().first().click();await waitText("😀bad bad\r\n");
  if((await requests("workspace/executeCommand")).length)throw Error("Server command executed");
  await mode("resolve","normal");
 });
 await step("action list scrolling, staged multi-file edits and validation",async()=>{
  await fixture();await mode("actions","many");await trigger("actions");await loaded(30);await page.keyboard.press("End");
  await page.waitForFunction(()=>{const menu=document.querySelector('.primary .mk-native-tools');const item=menu?.querySelectorAll('.mk-native-tool-item')[29];if(!item||document.activeElement!==item)return false;const m=menu.getBoundingClientRect(),i=item.getBoundingClientRect();return i.top>=m.top&&i.bottom<=m.bottom;});
  await page.keyboard.press("Enter");await waitText("😀fixed29 fixed29\r\n");
  await fixture();await mode("actions","invalid");await trigger("actions");await loaded(1);await rows().first().click();await status("column outside line");if(await text()!=="😀old old\r\n"||await page.locator(".rename-target-canonical").textContent()!=="")throw Error("Bad cross-file action partially applied");
  await mode("actions","cross");await trigger("actions");await loaded(1);await rows().first().click();await waitText("😀fixed fixed\r\n");await status("Applied edits to 2 files");
  if(await page.locator(".rename-target-canonical").textContent()!=="// target\r\n😀fixed\r\n")throw Error("Cross-file action opened wrong source");
  if(await page.locator(".definition-target").count())throw Error("Code action changed active file");
  await control("rename-target-external");await fixture();await mode("actions","cross");await trigger("actions");await loaded(1);await rows().first().click();await waitText("😀fixed fixed\r\n");await status("Applied edits to 2 files");
  if(await page.locator(".rename-target-canonical").textContent()!=="// externally refreshed\r\n😀fixed\r\n")throw Error("Stale unmounted editor session blocked or overwrote refreshed target");
  await control("definition-close");
 });
 await step("Shift-F12 lists deduplicated references, then reveals Unicode and cross-file targets",async()=>{
  await fixture();await mode("references","normal");await trigger("references");await loaded(3);
  const request=(await requests(method("references"))).at(-1);if(request.params.position.character!==4||!request.params.context.includeDeclaration)throw Error("Reference request coordinates or declaration context wrong");
  if(!await rows().nth(2).isDisabled())throw Error("Outside-folder reference was navigable");
  if(!(await rows().nth(1).textContent()).includes("target #é.rs:2:3"))throw Error("Reference URI was not decoded for display");
  await page.keyboard.press("Enter");await selected(".primary",2);if(await text()!=="😀old old\r\n")throw Error("Reference navigation edited text");
  await origin();await trigger("references",true);await loaded(3);await page.keyboard.press("ArrowDown");await page.keyboard.press("Enter");await page.locator(".definition-target").waitFor();await selected(".definition-target",13);
  await page.locator(".definition-target .mk-editor-core-input-sink").evaluate(input=>{if(document.activeElement!==input)throw Error("Reference reveal did not focus target");});
  await page.keyboard.press("Control+End");await page.keyboard.type("x");await origin();await trigger("references");await loaded(3);await rows().nth(1).click();await selected(".definition-target",13);
  if(!(await page.locator(".definition-canonical").textContent()).endsWith("x"))throw Error("Reference reveal overwrote dirty target");
  await control("definition-close");
  await control("folds");await root.locator('[data-fold-line="0"]').click();await mode("references","fold");await origin();await trigger("references");await loaded(1);await rows().first().click();
  await page.waitForFunction(()=>document.querySelector('.primary [data-fold-line="0"]')?.getAttribute("aria-expanded")==="true");
 });
 await step("empty/errors/malformed results and missing references keep editing available",async()=>{
  for(const kind of ["actions","references"]){
   await fixture();await mode(kind,"empty");await trigger(kind);await loaded(0);await dismiss();
   await mode(kind,"error");await trigger(kind);await status("tools error");await menu().waitFor({state:"detached"});
  }
  await mode("actions","malformed");await trigger("actions");await status("invalid text edit");if(await text()!=="😀old old\r\n")throw Error("Malformed action edited text");
  await mode("references","missing");await trigger("references");await loaded(1);await rows().first().click();await status("not found");
  await mode("references","malformed");await trigger("references");await loaded(1);if(!await rows().first().isDisabled())throw Error("Malformed URI navigable");await dismiss();
 });
 await step("late action/reference/resolve replies cancel on Escape, cursor/edit/tab/other-file changes and unmount",async()=>{
  for(const kind of ["actions","references","resolve"]){
   for(const action of ["escape","cursor","edit","tab","other","unmount"]){
    await fixture();await mode(kind,"slow");const rpc=method(kind),before=(await requests(rpc)).length;
    if(kind==="resolve"){await mode("actions","normal");await trigger("actions");await loaded(4);await rows().first().click();}else await trigger(kind);
    await waitCount(rpc,before+1);const id=(await requests(rpc)).at(-1).id;
    if(action==="escape")await page.keyboard.press("Escape");
    if(action==="cursor"){await input().focus();await page.keyboard.press("ArrowRight");}
    if(action==="edit"){await input().focus();await page.keyboard.type("x");}
    if(action==="tab")await page.locator('[data-language-fixture="nested.html"]').click();
    if(action==="other")await control("rename-other-edit");
    if(action==="unmount"){await control("mount");await root.waitFor({state:"detached"});}
    await cancel(id);await control("tools-release");
    if(action==="unmount"){await control("mount");await root.waitFor();}
    if(await menu().count()||(await text()).includes("resolved")||(await text()).includes("fixed"))throw Error(`Stale ${kind} result after ${action}`);
   }
  }
  await mode("resolve","normal");
 });
 await step("sibling panes do not restart requests; cancellations guard action/reveal file loading",async()=>{
  await fixture();await mode("actions","slow");const before=(await requests(method("actions"))).length;await trigger("actions");await waitCount(method("actions"),before+1);const id=(await requests(method("actions"))).at(-1).id;
  await control("python");await page.waitForTimeout(150);if((await requests(method("actions"))).length!==before+1)throw Error("Sibling pane restarted action request");await input().focus();await page.keyboard.press("Escape");await cancel(id);await control("tools-release");await control("python");
  for(const kind of ["actions","references"]){
   await fixture();await control("definition-hold-load");await mode(kind,"cross");const before=(await log()).filter(entry=>entry.method==="fixture/fetchDefinition").length;await trigger(kind);await loaded(1);await rows().first().click();
   await page.waitForFunction(count=>JSON.parse(document.querySelector(".lsp-log").textContent).map(JSON.parse).filter(entry=>entry.method==="fixture/fetchDefinition").length>count,before);
   await input().focus();await page.keyboard.press("ArrowRight");await control("definition-release-load");await menu().waitFor({state:"detached"});
   if(await text()!=="😀old old\r\n"||await page.locator(".rename-target-canonical").textContent()!==""||await page.locator(".definition-target").count())throw Error(`Canceled ${kind} load changed workspace`);
  }
 });
 await step("action/reference/resolve timeout recovers without applying late results",async()=>{
  for(const kind of ["actions","references","resolve"]){
   await fixture();await mode(kind,"slow");const rpc=method(kind),before=(await requests(rpc)).length;
   if(kind==="resolve"){await mode("actions","normal");await trigger("actions");await loaded(4);await rows().first().click();}else await trigger(kind);
   await waitCount(rpc,before+1);const id=(await requests(rpc)).at(-1).id;await status("timed out");await cancel(id);await control("tools-release");await menu().waitFor({state:"detached"});if(await text()!=="😀old old\r\n")throw Error(`Timed-out ${kind} edited text`);
  }
  await mode("actions","normal");await mode("resolve","normal");await trigger("actions");await loaded(4);await rows().nth(2).click();await waitText("😀fixed fixed\r\n");
 });
 if(errors.length)throw Error(errors.join("\n"));
}finally{await browser.close();}
