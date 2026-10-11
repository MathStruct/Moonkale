// E3 of the editor-extensibility RFC: does a markdown file survive the editors byte for byte?
// For every file in corpus/ and every mode (Source = CodeMirror, Rich = Milkdown):
//   1. open it: the tab must stay clean and the file unchanged;
//   2. append " EDIT" to its "Sentinel paragraph." (in the middle of the file) and save:
//      the file must equal the original with exactly that edit — every other byte kept.
// A measurement first: it prints a table and writes $M1_SHOTS/e3-roundtrip.{json,md}. It fails
// on harness errors, and on fidelity failures in the modes named by E3_STRICT (default "source";
// E3_STRICT= to only measure).
// E3_MODES=source,rich (default) and E3_ONLY=<substring of a file name> narrow the run.
import { firefox } from "playwright";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
const S = process.env.M1_SHOTS ?? ".";
const PORT = process.env.PORT ?? 8080;
const ROOT = process.env.M1_ROOT;
const REPO = process.env.MOONKALE_REPO ?? path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../../..");
const CORPUS = `${REPO}/packages/web/tests/e2e/corpus`;
const MODES = (process.env.E3_MODES ?? "source,rich").split(",").filter(Boolean);
const STRICT = (process.env.E3_STRICT ?? "source").split(",").filter(Boolean);
const ONLY = process.env.E3_ONLY ?? "";
const SENTINEL = "Sentinel paragraph.";
const HANG_S = Number(process.env.E3_HANG_S ?? 90);
const EDIT = " EDIT";
const ok = (r) => r.openClean && r.openUnchanged && r.exact;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// What changed between two texts, in words a reader can act on.
function differences(want, got) {
  const notes = [];
  if (want.startsWith("﻿") && !got.startsWith("﻿")) notes.push("BOM dropped");
  if (want.includes("\r\n") && !got.includes("\r\n")) notes.push("CRLF → LF");
  if (/\n$/.test(want) !== /\n$/.test(got)) notes.push(/\n$/.test(got) ? "final newline added" : "final newline dropped");
  const a = want.replace(/\r\n/g, "\n").split("\n");
  const b = got.replace(/\r\n/g, "\n").split("\n");
  const count = (xs) => xs.reduce((m, x) => m.set(x, (m.get(x) ?? 0) + 1), new Map());
  const ca = count(a), cb = count(b);
  let lost = 0, added = 0;
  for (const [l, n] of ca) lost += Math.max(0, n - (cb.get(l) ?? 0));
  for (const [l, n] of cb) added += Math.max(0, n - (ca.get(l) ?? 0));
  const i = a.findIndex((l, k) => l !== b[k]);
  const first = i < 0 ? null : { line: i + 1, want: a[i]?.slice(0, 100), got: b[i]?.slice(0, 100) };
  return { notes, lost, added, first };
}

// `line` (1-based) puts the cursor there: quick open takes `path:line`.
async function openFile(page, rel, mode, line) {
  await page.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
  await page.waitForSelector(".wb-workspace");
  await page.click(".mk-explorer-open button[type=submit]");
  await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
  await page.keyboard.press("Control+P");
  await page.waitForSelector(".mk-palette-input", { timeout: 10000 });
  await page.type(".mk-palette-input", `${rel}:${line}`);
  // An item shows the file name and its folder as separate parts.
  await page.waitForFunction(([dir, name]) => {
    const t = document.querySelector(".mk-palette-item")?.textContent ?? "";
    return t.includes(name) && t.includes(dir);
  }, [path.dirname(rel), path.basename(rel)], { timeout: 10000 });
  await page.keyboard.press("Enter");
  await page.waitForSelector(".mk-md .cm-content:visible", { timeout: 15000 });
  if (mode === "rich") {
    await page.click(".mk-md-modes button:has-text('Rich'):visible");
    await page.waitForSelector(".mk-rich-host:visible .ProseMirror", { timeout: 30000 });
  }
  await sleep(800);
}

async function editSentinel(page, mode) {
  // Source: quick open left the cursor on the sentinel's line (CodeMirror renders only the
  // lines in view, so clicking it is not an option). Rich: ProseMirror renders everything.
  if (mode === "rich") await page.click(`.mk-rich-host:visible .ProseMirror p:has-text("${SENTINEL}")`);
  await page.keyboard.press("End");
  await page.keyboard.type(EDIT);
  await page.waitForSelector(".mk-tab-dirty", { timeout: 10000 });
  if (mode === "source") await page.keyboard.press("Control+S");
  else await page.click(".mk-rich button:has-text('Save'):visible");
  await page.waitForFunction(() => !document.querySelector(".mk-tab-dirty"), null, { timeout: 15000 });
}

const files = fs.readdirSync(CORPUS).filter((f) => f.endsWith(".md") && f.includes(ONLY)).sort();
let browser = await firefox.launch();
const rows = [];
let harness = 0;
try {
  for (const mode of MODES) {
    for (const f of files) {
      const rel = `e3/${mode}/${f}`;
      const abs = `${ROOT}/${rel}`;
      const original = fs.readFileSync(`${CORPUS}/${f}`, "utf8");
      fs.mkdirSync(path.dirname(abs), { recursive: true });
      fs.writeFileSync(abs, original);
      const row = { mode, file: f };
      process.stdout.write(`- ${mode} ${f} … `);
      // A fresh context per file: a reused page restores the previous file's tab.
      const ctx = await browser.newContext({ viewport: { width: 1500, height: 900 } });
      const page = await ctx.newPage();
      const line = original.slice(0, original.indexOf(SENTINEL)).split("\n").length;
      const run = async () => {
        await openFile(page, rel, mode, line);
        row.openClean = !(await page.$(".mk-tab-dirty"));
        row.openUnchanged = fs.readFileSync(abs, "utf8") === original;
        await editSentinel(page, mode);
        const want = original.replace(SENTINEL, SENTINEL + EDIT);
        const got = fs.readFileSync(abs, "utf8");
        row.exact = got === want;
        if (!row.exact) Object.assign(row, differences(want, got));
      };
      // A page whose script never yields also stalls Playwright's own timeouts (P-164: the CRLF
      // resync loop): a watchdog on the Node side, and a new browser after a hang.
      let timer;
      const watchdog = new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(`hung: no progress in ${HANG_S} s`)), HANG_S * 1000); });
      try {
        await Promise.race([run(), watchdog]);
        console.log(ok(row) ? "exact" : `DIFFERS ${JSON.stringify({ ...row, mode: undefined, file: undefined })}`);
      } catch (e) {
        row.error = String(e.message ?? e).split("\n")[0];
        // A hang is the app's failure (counted against the mode), anything else the harness's.
        if (!row.error.startsWith("hung")) harness++;
        console.log(`ERROR ${row.error}`);
        if (!row.error.startsWith("hung")) await page.screenshot({ path: `${S}/e3-${mode}-${f}.png` }).catch(() => {});
      }
      clearTimeout(timer);
      if (row.error?.startsWith("hung")) {
        await Promise.race([browser.close(), sleep(10000)]).catch(() => {});
        browser = await firefox.launch();
      } else {
        await ctx.close();
      }
      rows.push(row);
    }
  }
} finally {
  await browser.close();
}

const cell = (r) => r.error ? `error: ${r.error}` : ok(r) ? "exact" :
  [!r.openClean && "dirty on open", !r.openUnchanged && "changed on open", !r.exact && `-${r.lost}/+${r.added} lines`, ...(r.notes ?? [])].filter(Boolean).join(", ");
const md = ["| file | " + MODES.join(" | ") + " |", "|---|" + MODES.map(() => "---").join("|") + "|"];
for (const f of files) md.push(`| ${f} | ` + MODES.map((m) => cell(rows.find((r) => r.mode === m && r.file === f))).join(" | ") + " |");
for (const m of MODES) md.push("", `**${m}**: ${rows.filter((r) => r.mode === m && ok(r)).length}/${files.length} exact`);
fs.writeFileSync(`${S}/e3-roundtrip.json`, JSON.stringify(rows, null, 1));
fs.writeFileSync(`${S}/e3-roundtrip.md`, md.join("\n") + "\n");
console.log("\n" + md.join("\n"));
const strictFail = rows.filter((r) => STRICT.includes(r.mode) && !ok(r));
if (harness || strictFail.length) {
  console.log(`FAIL: ${harness} harness errors, ${strictFail.length} strict failures`);
  process.exit(1);
}
console.log("PASS (measurement; strict modes: " + (STRICT.join(",") || "none") + ")");
