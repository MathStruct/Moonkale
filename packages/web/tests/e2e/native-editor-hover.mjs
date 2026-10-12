// Deterministic production-panel LSP hover and cancellation coverage.
import { chromium } from "playwright";
const browser = await chromium.launch();
const page = await browser.newPage({viewport:{width:1200,height:800}});
page.setDefaultTimeout(15000);
const errors = [];
page.on("pageerror", error => errors.push(error.message));
const root = page.locator(".primary [data-editor=rust]");
const popup = root.locator(".mk-native-hover");
const row = line => root.locator(`.mk-editor-core-row[data-line="${line}"]`);
const cell = (line, text) => row(line).locator(".mk-editor-core-cell").filter({hasText:text}).first();
const requests = async () => JSON.parse(await page.locator(".lsp-log").textContent()).map(JSON.parse).filter(entry=>entry.method==="textDocument/hover");
const count = async () => (await requests()).length;
const waitCount = async expected => page.waitForFunction(expected => JSON.parse(document.querySelector(".lsp-log").textContent).map(JSON.parse).filter(entry=>entry.method==="textDocument/hover").length >= expected, expected);
// Changing fake server behavior must leave the real pointer/hover target in place.
const mode = async value => page.locator(`[data-hover-mode="${value}"]`).evaluate(button=>button.click());
const release = async () => page.locator("#hover-release").evaluate(button=>button.click());
const outside = async () => { await page.mouse.move(1190,790); await popup.waitFor({state:"detached"}); };
const hover = async (line,text) => cell(line,text).hover({position:{x:2,y:10}});
const step = async (name, action) => { await action(); console.log(`ok: ${name}`); };
try {
  await page.goto(`http://127.0.0.1:${process.env.PORT??"8099"}/`);
  await root.waitFor();
  await page.waitForFunction(()=>JSON.parse(document.querySelector(".lsp-log").textContent).map(JSON.parse).some(entry=>entry.method==="textDocument/didOpen"));
  const original = await page.locator(".canonical").textContent();
  await step("Unicode request coordinates, escaped plain text and pointer dismissal", async()=>{
    await hover(1,"😀"); await popup.waitFor();
    if ((await requests()).at(-1).params.position.character!==7) throw Error("Emoji hover coordinate mismatch");
    if (!(await popup.textContent()).includes("<safe> docs") || (await popup.textContent()).includes("```")) throw Error("Hover text format mismatch");
    await hover(1,"中"); await page.waitForFunction(()=>document.querySelector(".primary .mk-native-hover")?.textContent.includes("1:9"));
    if ((await requests()).at(-1).params.position.character!==9) throw Error("Position after emoji was not UTF-16");
    await popup.hover(); await popup.waitFor();
    await outside();
  });
  await step("pointer debounce and keyboard hover/Escape", async()=>{
    const before = await count();
    await hover(0,"f"); await hover(1,"中");
    await popup.waitFor();
    if (await count()!==before+1) throw Error("Pointer movement was not debounced");
    await root.locator(".mk-editor-core-input-sink").focus();
    await page.keyboard.press("Escape"); await popup.waitFor({state:"detached"});
    await page.keyboard.press("Control+Home");
    await root.locator(".mk-native-show-hover").focus(); await page.keyboard.press("Enter");
    await popup.waitFor();
    if (!(await popup.textContent()).includes("0:0")) throw Error("Keyboard hover did not use the caret");
    await popup.focus(); await page.keyboard.press("Escape"); await popup.waitFor({state:"detached"});
  });
  await step("out-of-order replies are canceled and cannot replace the latest hover", async()=>{
    await outside(); await mode("slow");
    const before = await count(); await hover(1,"😀"); await waitCount(before+1);
    const staleId = (await requests()).at(-1).id;
    await mode("normal"); await hover(1,"中"); await popup.waitFor();
    await release();
    await page.waitForFunction(id=>JSON.parse(document.querySelector(".lsp-log").textContent).map(JSON.parse).some(entry=>entry.method==="$/cancelRequest"&&entry.params.id===id),staleId);
    if (!(await popup.textContent()).includes("1:9")) throw Error("Late reply overwrote the current hover");
  });
  await step("empty/error/blank responses and timeout leave editing available", async()=>{
    for(const value of ["empty","error","blank"]){
      await outside(); await mode(value); const before=await count();
      await hover(1,"😀"); await waitCount(before+1); await page.waitForTimeout(75);
      if(await popup.count()) throw Error(`${value} response produced hover content`);
    }
    await outside(); await mode("slow"); const before=await count();
    await hover(1,"😀"); await waitCount(before+1); const id=(await requests()).at(-1).id;
    await page.waitForFunction(id=>JSON.parse(document.querySelector(".lsp-log").textContent).map(JSON.parse).some(entry=>entry.method==="$/cancelRequest"&&entry.params.id===id),id);
    if(await popup.count()) throw Error("Timed-out response produced hover content");
    await release(); if(await popup.count()) throw Error("Late timed-out response resurfaced");
  });
  await step("edits and unmount invalidate pending responses; CRLF positions remain exact", async()=>{
    await outside(); await mode("slow"); const before=await count();
    await hover(1,"😀"); await waitCount(before+1);
    await root.locator(".mk-editor-core-input-sink").focus(); await page.keyboard.press("Control+End"); await page.keyboard.type("x");
    await release(); if(await popup.count()) throw Error("Reply after edit was accepted");
    await page.click("#undo");
    await page.waitForFunction(original=>document.querySelector(".canonical").textContent===original, original);
    await mode("slow"); const next=await count(); await hover(1,"😀"); await waitCount(next+1);
    await page.locator("#mount").evaluate(button=>button.click()); await root.waitFor({state:"detached"}); await release();
    await page.locator("#mount").evaluate(button=>button.click()); await root.waitFor();
    if(await popup.count()) throw Error("Unmounted hover resurfaced");
    await page.click("#hover-crlf"); await mode("normal");
    await hover(1,"中"); await popup.waitFor();
    const source=await page.locator(".canonical").textContent();
    const start=source.split("\n")[1].indexOf("中");
    if((await requests()).at(-1).params.position.character!==start) throw Error("CRLF hover coordinate mismatch");
    await outside();
  });
  if(errors.length) throw Error(errors.join("\n"));
} finally { await browser.close(); }
