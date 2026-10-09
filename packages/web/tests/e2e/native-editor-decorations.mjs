import { chromium } from "playwright";
import assert from "node:assert/strict";

const browser = await chromium.launch();
const page = await browser.newPage();
const errors = [];
page.on("pageerror", error => errors.push(error.message));
try {
  await page.goto(`http://127.0.0.1:${process.env.PORT ?? "8099"}/`, { waitUntil: "networkidle" });
  const original = await page.locator(".canonical").textContent();
  await page.click("#diagnostics");
  await page.locator(".primary .mk-native-diagnostic-error").first().waitFor();
  await page.locator(".primary .mk-editor-core-input-sink").focus();
  await page.keyboard.press("Control+f");
  await page.locator(".primary .mk-search-query").fill("😀");
  const combined = page.locator(".primary .mk-native-search-match.mk-native-diagnostic-error");
  await combined.waitFor();
  assert.equal(await combined.textContent(), "😀");
  assert.ok((await combined.getAttribute("title")).includes("<safe>"));
  await page.click("#diagnostics-clear");
  await page.locator(".primary .mk-native-diagnostic-error").waitFor({ state: "detached" });
  assert.equal(await page.locator(".primary .mk-native-search-match").textContent(), "😀");
  assert.equal(await page.locator(".canonical").textContent(), original);
  await page.click("#wiki");
  await page.locator(".language .mk-native-wiki-resolved").first().waitFor();
  const wikiOriginal = await page.locator(".language-canonical").textContent();
  await page.locator(".language .mk-editor-core-input-sink").focus();
  await page.keyboard.press("Control+f");
  await page.locator(".language .mk-search-query").fill("Note é");
  await page.locator(".language .mk-native-search-match.mk-native-wiki-resolved").first().waitFor();
  assert.equal((await page.locator(".language .mk-native-search-match.mk-native-wiki-resolved").allTextContents()).join(""), "Note é");
  assert.equal(await page.locator(".language-canonical").textContent(), wikiOriginal);
  assert.deepEqual(errors, []);
  console.log("PASS: overlapping Unicode search/diagnostics/wiki marks preserve styles, tooltip and canonical source");
} finally {
  await browser.close();
}
