import { chromium } from "playwright";
const browser = await chromium.launch();
const context = await browser.newContext();
const errors = [];
const url = `http://127.0.0.1:${process.env.PORT ?? "8099"}/`;
const pair = async () => {
  const a = await context.newPage(), b = await context.newPage();
  for (const page of [a, b]) {
    page.on("pageerror", error => errors.push(error.message));
    await page.goto(url, { waitUntil: "networkidle" });
    await page.locator(".primary [data-editor=rust]").waitFor();
  }
  await b.click("#integration-close-main");
  await a.click("#integration-connect");
  await b.click("#integration-connect");
  for (const page of [a, b]) await page.waitForFunction(() => Number(document.querySelector("#session-state")?.dataset.peers) > 0);
  return [a, b];
};
try {
  let [a, b] = await pair();
  await a.click("#integration-drag");
  await b.waitForFunction(() => document.querySelector("#session-state")?.dataset.drop === "true");
  await b.click("#integration-accept");
  await b.locator(".primary [data-editor=rust]").waitFor();
  await a.locator(".primary").waitFor({ state: "detached" });
  if (await b.locator("#active-canonical").textContent() !== "fn main() {\n    // 😀中 Unicode\n}\n") throw Error("Moved document lost source text");
  await a.close(); await b.close();
  console.log("ok: two browser windows exchange a real session offer and move a clean native tab");
  [a, b] = await pair();
  await a.locator(".primary .mk-editor-core-input-sink").focus();
  await a.keyboard.press("Control+Home");
  await a.keyboard.type("unsaved ");
  await a.click("#integration-drag");
  await b.waitForTimeout(300);
  if (await b.locator("#session-state").getAttribute("data-drop") !== "false") throw Error("Unsaved document was offered through a saved-source-only transport");
  if (await a.locator(".primary").count() !== 1) throw Error("Unsaved origin closed");
  await a.close(); await b.close();
  [a, b] = await pair();
  await a.click("#integration-drag");
  await b.waitForFunction(() => document.querySelector("#session-state")?.dataset.drop === "true");
  await a.locator(".primary .mk-editor-core-input-sink").focus();
  await a.keyboard.press("Control+Home");
  await a.keyboard.type("late edit ");
  await b.click("#integration-accept");
  await b.locator(".primary [data-editor=rust]").waitFor();
  await a.waitForTimeout(300);
  if (!(await a.locator("#active-canonical").textContent()).startsWith("late edit ")) throw Error("Move acknowledgement discarded a newer unsaved edit");
  if (await a.locator(".primary").count() !== 1) throw Error("Move acknowledgement closed the edited origin");
  await a.close(); await b.close();
  console.log("ok: edits made after a move offer survive its acknowledgement");
  if (errors.length) throw Error(errors.join("\n"));
  console.log("ok: unsaved native tabs remain in their origin window");
} finally {
  await browser.close();
}
