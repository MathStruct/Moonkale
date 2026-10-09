import { chromium } from "playwright";
const browser = await chromium.launch();
const page = await browser.newPage();
const errors = [];
page.on("pageerror", error => errors.push(error.message));
const badge = page.locator(".primary .mk-native-presence");
try {
  await page.goto(`http://127.0.0.1:${process.env.PORT ?? "8099"}/`, { waitUntil: "networkidle" });
  await page.locator(".primary [data-editor=rust]").waitFor();
  await page.click("#replace");
  await page.click("#presence");
  await page.waitForFunction(() => document.querySelector(".primary .mk-native-presence")?.dataset.presenceLine === "1");
  if (await badge.count() !== 1 || await badge.textContent() !== "AB" || await badge.getAttribute("title") !== "Alice Bob") throw Error("Presence must exclude this window and other files");
  await page.click("#presence-move");
  await page.waitForFunction(() => document.querySelector(".primary .mk-native-presence")?.dataset.presenceLine === "0");
  await page.click("#mount");
  await page.click("#mount");
  await badge.waitFor();
  await page.locator('[data-language-fixture="nested.html"]').click();
  if (await page.locator(".language .mk-native-presence").count()) throw Error("Presence leaked between documents");
  await page.click("#presence-clear");
  await badge.waitFor({ state: "detached" });
  if (errors.length) throw Error(errors.join("\n"));
  console.log("ok: live presence filters members, moves lines, survives remount, and clears");
} finally {
  await browser.close();
}
