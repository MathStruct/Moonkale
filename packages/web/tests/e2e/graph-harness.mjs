import { chromium } from 'playwright';
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
export async function enginePage() {
  const repo = process.env.MOONKALE_REPO;
  if (!repo) throw new Error('MOONKALE_REPO required');
  const assets = process.env.MOONKALE_GRAPH_ASSETS || resolve(repo, 'packages/graph-assets/assets');
  const server = createServer(async (req, res) => {
    try {
      if (req.url === '/') {
        res.setHeader('Content-Type', 'text/html');
        res.end('<style>body{margin:0}canvas{position:absolute;left:0;top:0;width:1200px;height:800px}</style><canvas id="gpu"></canvas><canvas id="overlay"></canvas>');
      } else if (req.url === '/graph_render.js' || req.url === '/graph_render_bg.wasm') {
        res.setHeader('Content-Type', req.url.endsWith('.js') ? 'text/javascript' : 'application/wasm');
        res.end(await readFile(resolve(assets, req.url.slice(1))));
      } else { res.writeHead(404).end(); }
    } catch { res.writeHead(500).end(); }
  });
  await new Promise(r => server.listen(0, '127.0.0.1', r));
  let browser;
  try {
    browser = await chromium.launch({args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist']});
    const page = await browser.newPage({ viewport: { width: 1200, height: 800 } });
    await page.goto(`http://127.0.0.1:${server.address().port}/`);
    await page.evaluate(async () => {
      const mod = await import('/graph_render.js'); await mod.default();
      window.events = [];
      window.view = await mod.create(document.querySelector('#gpu'), document.querySelector('#overlay'), e => events.push(e), 'gl');
      view.resize(1200, 800, 1);
    });
    return {page, async close() { await browser.close(); await new Promise(r => server.close(r)); }};
  } catch (error) {
    if (browser) await browser.close();
    await new Promise(r => server.close(r));
    throw error;
  }
}
