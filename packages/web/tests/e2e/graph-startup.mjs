// P-224: isolate the shipped engine and production graph-panel payload from app startup.
// MOONKALE_GRAPH_PAYLOAD may name the JSON emitted by profile_store_payload.
import { readFile } from 'node:fs/promises';
import { litPixels } from './pnglit.mjs';
import { enginePage } from './graph-harness.mjs';
const host = await enginePage();
const {page} = host;
try {
  const payload = process.env.MOONKALE_GRAPH_PAYLOAD ? JSON.parse(await readFile(process.env.MOONKALE_GRAPH_PAYLOAD, 'utf8')) : null;
  await page.evaluate(payload => {
    const n = 100000;
    if (!payload) {
      payload = {nodes: [], edges: []};
      for (let i = 0; i < n; i++) payload.nodes.push({id: 'n'+i, label: 'node '+i, kind:'symbol', key:'definition:'+i});
      for (let i = 1; i < n; i++) payload.edges.push({a:i, b:(i/7)|0});
    }
    window.startupPayload = payload;
  }, payload);
  const samples = [];
  for (let run = 0; run < Number(process.env.GRAPH_STARTUP_RUNS || 1); run++) {
  const result = await page.evaluate(async () => {
    const payload = window.startupPayload;
    const start = performance.now();
    const json = JSON.stringify(payload);
    const stringify = performance.now() - start;
    view.set_graph_fresh(json);
    while (!view.load_stats()[5]) await new Promise(requestAnimationFrame);
    const initial_layout_iterations = view.layout_state()[2];
    // One further rAF gives the submitted frame a presentation opportunity.
    await new Promise(requestAnimationFrame);
    return {initial_layout_iterations, nodes: payload.nodes.length, edges: payload.edges.length, bytes:json.length, stringify_ms:stringify, engine_ms:Array.from(view.load_stats()), picture_opportunity_ms:performance.now()-start};
  });
  samples.push(result);
  console.log('STARTUP', JSON.stringify(result));
  if (result.initial_layout_iterations !== Number(process.env.GRAPH_EXPECT_INITIAL_LAYOUT_ITERATIONS || 0)) throw new Error('layout ran before the first picture');
  if (process.env.GRAPH_STARTUP_GATE === '1' && result.picture_opportunity_ms > 3000) throw new Error('first picture exceeds 3 s');
  }
  const screenshot = await page.screenshot(process.env.M1_SHOTS ? {path: `${process.env.M1_SHOTS}/graph-startup.png`} : {});
  const pixels = litPixels(screenshot).lit.length;
  if (pixels < 100) throw new Error('100k graph did not produce visible pixels');
  console.log('PIXELS', pixels, 'samples', samples.length);
  console.log('PASS graph-startup');
} finally { await host.close(); }
