// Claude's second review: connection recovery and shared guarded CodeMirror edits.
import { chromium } from "playwright";
const browser = await chromium.launch();
const page = await browser.newPage({viewport:{width:1500,height:1000}});
page.setDefaultTimeout(15000);
const errors=[];page.on("pageerror",error=>errors.push(error.message));
const click=id=>page.locator(`#${id}`).evaluate(button=>button.click());
const log=async()=>JSON.parse(await page.locator(".lsp-log").textContent()).map(JSON.parse);
const count=async method=>(await log()).filter(entry=>entry.method===method).length;
const waitCount=(method,count)=>page.waitForFunction(({method,count})=>JSON.parse(document.querySelector(".lsp-log").textContent).map(JSON.parse).filter(entry=>entry.method===method).length>=count,{method,count});
const waitText=text=>page.waitForFunction(text=>document.querySelector(".canonical").textContent===text,text);
try {
 await page.goto(`http://127.0.0.1:${process.env.PORT??"8099"}/`);
 await page.locator(".primary [data-editor=rust]").waitFor();await waitCount("textDocument/didOpen",1);
 await click("rename-fixture");await waitText("😀old old\r\n");
 await click("tools-diagnostics");await page.locator(".primary .mk-native-diagnostic-message").waitFor();
 console.log("ok: diagnostics from an encoded URI alias match the mounted document");
 for(const backend of ["rust","codemirror"]) {
  if(backend==="codemirror"){await click("editor-backend");await page.locator(".primary .cm-content").waitFor();}
  if(backend==="rust"){await click("duplicate");await page.locator(".duplicate [data-editor=rust]").waitFor();}
  const opened=await count("textDocument/didOpen"), initialized=await count("initialize");
  await click("lsp-crash");await waitCount("initialize",initialized+1);await waitCount("textDocument/didOpen",opened+1);
  if(backend==="rust") {
   const closed=await count("textDocument/didClose");await click("duplicate");await page.locator(".duplicate").waitFor({state:"detached"});
   await page.waitForTimeout(150);if(await count("textDocument/didClose")!==closed)throw Error("Closing a recovered sibling released the surviving editor lease");
  }
  const root=page.locator(".primary");
  const sink=root.locator(backend==="rust"?".mk-editor-core-input-sink":".cm-content");
  await sink.focus();await page.keyboard.press("Control+Home");await page.keyboard.press("ArrowRight");await page.keyboard.press("ArrowRight");
  await page.locator('[data-actions-mode="normal"]').evaluate(button=>button.click());if(backend==="rust")await page.keyboard.press("Control+.");else await click("review-actions");
  await root.locator(backend==="rust"?".mk-native-tool-item":".mk-editor-actions button").first().waitFor();
  if(backend==="rust"){await page.keyboard.press("Escape");}
  else {
   await root.locator(".mk-editor-actions button").filter({hasText:"Direct <safe> fix"}).click();await waitText("😀fixed fixed\r\n");
   await sink.focus();await page.keyboard.press("Control+z");await waitText("😀old old\r\n");
  }
  console.log(`ok: ${backend} reattaches after server closure and serves code actions`);
 }
 await page.locator('[data-actions-mode="stale-version"]').evaluate(button=>button.click());await click("review-actions");
 await page.locator(".primary .mk-editor-actions button").filter({hasText:"Stale version"}).click();
 await page.waitForFunction(()=>document.querySelector(".definition-status").textContent.includes("version is stale"));
 if(await page.locator(".canonical").textContent()!=="😀old old\r\n")throw Error("Stale server version changed origin");
 // Version mismatch and invalid second target must leave every file unchanged.
 for(const mode of ["invalid","cross"]) {
  await page.locator(`[data-actions-mode="${mode}"]`).evaluate(button=>button.click());await click("review-actions");
  await page.locator(".primary .mk-editor-actions button").filter({hasText:"Fix two files"}).click();
  if(mode==="invalid"){
   await page.waitForFunction(()=>document.querySelector(".definition-status").textContent.includes("outside"));
   if(await page.locator(".canonical").textContent()!=="😀old old\r\n")throw Error("Invalid second target partially changed origin");
  } else {
   await waitText("😀fixed fixed\r\n");
   await page.waitForFunction(()=>document.querySelector(".rename-target-canonical").textContent.includes("fixed"));
   await page.locator(".primary .cm-content").focus();await page.keyboard.press("Control+z");await waitText("😀old old\r\n");
  }
 }
 console.log("ok: CodeMirror stages multi-file edits atomically, preserves CRLF, and supports undo");
 if(errors.length)throw Error(errors.join("\n"));
} catch(error) { console.error("canonical:", JSON.stringify(await page.locator(".canonical").textContent())); console.error("status:",await page.locator(".definition-status").textContent()); console.error("recent protocol:",JSON.stringify((await log()).slice(-8))); throw error; } finally {await browser.close();}
