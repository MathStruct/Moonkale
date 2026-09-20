// Spec 008: png/jpg/svg open in an image viewer (data URL through fetch_bytes), with zoom and
// an SVG "Source" button that opens the text.
import { firefox } from "playwright";
import fs from "node:fs";
const S = process.env.M1_SHOTS ?? ".";
const PORT = process.env.PORT ?? 8080;
const ROOT = process.env.M1_ROOT;
const browser = await firefox.launch();
const page = await browser.newPage({ viewport: { width: 1500, height: 900 } });
const logs = [];
page.on("console", (m) => { const t = m.text(); if (!t.includes("session[")) logs.push(`[${m.type()}] ${t.slice(0, 300)}`); });
const step = async (n, f) => { process.stdout.write(`- ${n} … `); await f(); console.log("ok"); };
// A 4×3 red PNG built here (zlib + CRC), and a small SVG, written into the fixture.
import zlib from "node:zlib";
const crcTable = [...Array(256)].map((_, n) => { let c = n; for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1; return c >>> 0; });
const crc32 = (buf) => { let c = 0xffffffff; for (const b of buf) c = crcTable[(c ^ b) & 0xff] ^ (c >>> 8); return (c ^ 0xffffffff) >>> 0; };
const chunk = (type, data) => { const len = Buffer.alloc(4); len.writeUInt32BE(data.length); const td = Buffer.concat([Buffer.from(type), data]); const crc = Buffer.alloc(4); crc.writeUInt32BE(crc32(td)); return Buffer.concat([len, td, crc]); };
const W = 4, H = 3;
const raw = Buffer.concat([...Array(H)].map(() => Buffer.concat([Buffer.from([0]), Buffer.from([...Array(W)].flatMap(() => [255, 0, 0]))])));
const ihdr = Buffer.alloc(13); ihdr.writeUInt32BE(W, 0); ihdr.writeUInt32BE(H, 4); ihdr[8] = 8; ihdr[9] = 2; ihdr[10] = 0; ihdr[11] = 0; ihdr[12] = 0;
const PNG = Buffer.concat([Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]), chunk("IHDR", ihdr), chunk("IDAT", zlib.deflateSync(raw)), chunk("IEND", Buffer.alloc(0))]);
try {
  await step("open folder; pic.png opens in the image viewer with its dimensions", async () => {
    fs.mkdirSync(`${ROOT}/pics`, { recursive: true });
    fs.writeFileSync(`${ROOT}/pics/pic.png`, PNG);
    fs.writeFileSync(`${ROOT}/pics/logo.svg`, `<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><rect width="40" height="20" fill="#4fb3e8"/></svg>`);
    await page.goto(`http://127.0.0.1:${PORT}/`, { waitUntil: "networkidle" });
    await page.waitForSelector(".wb-workspace");
    await page.click(".mk-explorer-open button[type=submit]");
    await page.waitForFunction(() => document.querySelector(".wb-status-bar").textContent.includes("index:"), null, { timeout: 30000 });
    await page.click(".mk-tree-row[title='pics']");
    await page.click(".mk-tree-file >> text=pic.png");
    await page.waitForSelector(".mk-image img", { timeout: 15000 });
    await page.waitForFunction(() => /4 × 3/.test(document.querySelector(".mk-image .mk-editor-meta")?.textContent || ""), null, { timeout: 10000 });
    const src = await page.$eval(".mk-image img", (i) => i.src.slice(0, 22));
    console.log("\n  meta:", await page.$eval(".mk-image .mk-editor-meta", (e) => e.textContent), "src:", src);
    if (!src.startsWith("data:image/png")) throw new Error("not a png data url");
    if (await page.$(".cm-content")) throw new Error("a text editor opened for the image");
  });
  await step("zoom: + goes to 150 %, 100% sets 4×3 px, Fit returns", async () => {
    await page.click(".mk-image button:has-text('100%')");
    await page.waitForFunction(() => document.querySelector(".mk-image")?.dataset.zoom === "100%", null, { timeout: 5000 });
    const w = await page.$eval(".mk-image img", (i) => i.getBoundingClientRect().width);
    if (Math.round(w) !== 4) throw new Error("100% width is " + w);
    await page.click(".mk-image button[title^='Zoom in']");
    await page.waitForFunction(() => document.querySelector(".mk-image")?.dataset.zoom === "150%", null, { timeout: 5000 });
    await page.click(".mk-image button:has-text('Fit')");
    await page.waitForFunction(() => document.querySelector(".mk-image")?.dataset.zoom === "Fit", null, { timeout: 5000 });
    await page.screenshot({ path: `${S}/m10-image.png` });
  });
  await step("logo.svg renders as an image; Source opens its text in the code editor", async () => {
    await page.click(".mk-tree-file >> text=logo.svg");
    await page.waitForFunction(() => [...document.querySelectorAll(".mk-image img")].some((i) => i.src.startsWith("data:image/svg+xml")), null, { timeout: 15000 });
    await page.click(".mk-image button:has-text('Source'):visible");
    await page.waitForFunction(() => /<svg/.test([...document.querySelectorAll(".cm-content")].map((c) => c.textContent).join("")), null, { timeout: 10000 });
  });
  console.log("\nIMAGE E2E: PASS");
} catch (e) { console.log("\nFAIL:", e.message); console.log(logs.slice(-10).join("\n")); await page.screenshot({ path: `${S}/m10-image-fail.png` }); process.exitCode = 1; } finally { await browser.close(); }
