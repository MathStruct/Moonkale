// Real accessibility tree + keyboard journey; no mouse in the editing flow.
import { chromium } from 'playwright';
import fs from 'node:fs';
const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1500, height: 1000 }, reducedMotion: 'reduce' });
const panel = page.locator('.primary [data-editor=rust]');
const sink = panel.locator('.mk-editor-core-input-sink');
const status = panel.locator('[role=status]');
const canonical = () => page.locator('.canonical').textContent();
const expectSourceFocus = async focused => {
  await page.waitForFunction(focused => {
    const surface = document.querySelector('.primary .mk-native-surface');
    const style = getComputedStyle(surface);
    const visible = parseFloat(style.borderTopWidth) >= 2 && style.borderTopStyle !== 'none'
      && style.borderTopColor !== 'rgba(0, 0, 0, 0)';
    return surface.contains(document.activeElement) === focused && visible === focused;
  }, focused);
};
const tabTo = async (locator, backwards = false) => {
  for (let n = 0; n < 250; n++) {
    if (await locator.evaluate(el => el === document.activeElement)) return;
    await page.keyboard.press(backwards ? 'Shift+Tab' : 'Tab');
  }
  throw Error('keyboard could not reach target');
};
const waitSource = text => page.waitForFunction(text => document.querySelector('.canonical').textContent === text, text);
try {
  await page.goto(`http://127.0.0.1:${process.env.PORT ?? 8099}/`);
  await sink.waitFor();
  const original = await canonical();
  await tabTo(sink);
  await expectSourceFocus(true);
  await page.waitForFunction(() => document.querySelector('.primary [role=status]')?.textContent.includes('Line 1, column 1'));
  const snapshot = await panel.ariaSnapshot();
  if (!snapshot.includes('textbox "Code editor') || !snapshot.includes('main.rs')) throw Error(`missing named input: ${snapshot}`);
  if (await panel.getByRole('textbox').count() !== 1) throw Error('duplicate editable accessibility roles');
  if (!snapshot.includes('Tab indents') || !snapshot.includes('fn main()')) throw Error(`missing source context: ${snapshot}`);
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('End');
  await page.keyboard.press('Shift+ArrowLeft');
  await page.waitForFunction(() => document.querySelector('.primary [role=status]').textContent.includes('Selection from line 2'));
  await page.keyboard.type('Q');
  await panel.locator('.mk-editor-dirty').waitFor();
  await page.keyboard.press('Control+z');
  await waitSource(original);
  await page.keyboard.press('Control+m');
  await page.waitForFunction(() => document.querySelector('.primary [role=status]').textContent.includes('Tab moves focus'));
  await page.keyboard.press('Tab');
  if (await sink.evaluate(el => el === document.activeElement)) throw Error('keyboard trapped in source');
  await expectSourceFocus(false);
  await page.keyboard.press('Shift+Tab');
  if (!await sink.evaluate(el => el === document.activeElement)) throw Error('Shift+Tab did not return to source');
  await expectSourceFocus(true);
  await page.keyboard.press('Control+m');
  await page.keyboard.press('Control+Home');
  await page.keyboard.press('Tab');
  await panel.locator('.mk-editor-dirty').waitFor();
  await page.keyboard.press('Control+z');
  await waitSource(original);
  await page.keyboard.press('Control+f');
  const search = panel.locator('.mk-native-search input').first();
  await page.waitForFunction(() => document.activeElement === document.querySelector('.primary .mk-native-search input'));
  await page.keyboard.type('Unicode');
  await page.keyboard.press('Enter');
  await page.keyboard.press('Escape');
  await page.waitForFunction(() => document.activeElement?.classList.contains('mk-editor-core-input-sink'));
  await page.keyboard.press('Control+m');
  await tabTo(panel.locator('.mk-native-more > summary'), true);
  await page.keyboard.press('Enter');
  await tabTo(panel.getByRole('button', { name: 'Show hover information', exact: true }));
  await page.keyboard.press('Enter');
  await panel.getByRole('tooltip').waitFor();
  await tabTo(panel.getByRole('tooltip'));
  await expectSourceFocus(true);
  await page.keyboard.press('Escape');
  await page.waitForFunction(() => document.activeElement?.classList.contains('mk-editor-core-input-sink'));
  await page.keyboard.press('Control+m');
  await page.keyboard.press('F2');
  await panel.locator('.mk-editor-rename input').waitFor();
  await page.waitForFunction(() => document.activeElement === document.querySelector('.primary .mk-editor-rename input'));
  await page.keyboard.press('Escape');
  await page.waitForFunction(() => document.activeElement?.classList.contains('mk-editor-core-input-sink'));
  const focus = await panel.locator('.mk-native-surface').evaluate(el => {
    const style = getComputedStyle(el);
    return { width: style.borderTopWidth, style: style.borderTopStyle, color: style.borderTopColor };
  });
  if (parseFloat(focus.width) < 2 || focus.style === 'none' || focus.color === 'rgba(0, 0, 0, 0)') throw Error('source focus is not visible');
  if (await canonical() !== original) throw Error('keyboard navigation changed source');
  // Actual platform accessibility tree, including descriptions (not just DOM attributes).
  const cdp = await page.context().newCDPSession(page);
  const tree = await cdp.send('Accessibility.getFullAXTree');
  const input = tree.nodes.find(node => node.role?.value === 'textbox' && node.name?.value.includes('Code editor'));
  if (!input?.description?.value.includes('toggles whether Tab')) throw Error('input help absent from accessibility tree');
  if (!tree.nodes.some(node => node.role?.value === 'status')) throw Error('live context absent from accessibility tree');
  const palette = fs.readFileSync(`${process.env.MOONKALE_REPO}/packages/shell/src/theme.rs`, 'utf8');
  const tokens = [...palette.matchAll(/\(\s*"([^"]+)",\s*"([^"]+)",\s*"([^"]+)",?\s*\)/g)].map(([, key, dark, light]) => ({ key, dark, light }));
  if (!tokens.some(token => token.key === 'code-bg')) throw Error('production theme tokens not loaded');
  const luminance = color => color.match(/[\d.]+/g).slice(0, 3).map(Number).map(v => v / 255).map(v => v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4).reduce((sum, v, index) => sum + v * [0.2126, 0.7152, 0.0722][index], 0);
  const contrast = (a, b) => { const x = luminance(a), y = luminance(b); return (Math.max(x,y) + 0.05) / (Math.min(x,y) + 0.05); };
  const themes = {};
  for (const theme of ['light', 'dark']) {
    await page.evaluate(({ tokens, theme }) => { for (const token of tokens) document.documentElement.style.setProperty(`--mk-${token.key}`, token[theme]); document.querySelector(`#accessibility-${theme}`).click(); }, { tokens, theme });
    await page.waitForFunction(theme => document.querySelector('.primary .mk-native-surface').className.includes(theme === 'light' ? 'github' : 'tokyo'), theme);
    themes[theme] = await panel.locator('.mk-native-surface').evaluate(el => {
      const bg = getComputedStyle(el.querySelector('.mk-editor-core-viewport')).backgroundColor;
      return [...el.querySelectorAll('.mk-editor-core-cell, .mk-editor-core-gutter')].map(cell => {
        const style = getComputedStyle(cell);
        return { text: cell.textContent, color: style.color, background: style.backgroundColor === 'rgba(0, 0, 0, 0)' ? bg : style.backgroundColor, animation: style.animationName, scroll: style.scrollBehavior };
      });
    });
    for (const sample of themes[theme]) {
      const ratio = contrast(sample.color, sample.background);
      if (ratio < 4.5) throw Error(`${theme} source contrast ${ratio.toFixed(2)}: ${JSON.stringify(sample)}`);
      if (sample.animation !== 'none' || sample.scroll !== 'auto') throw Error('reduced motion not respected');
    }
  }
  fs.writeFileSync(`${process.env.M1_SHOTS ?? '.'}/native-editor-accessibility.json`, JSON.stringify({ snapshot, input, focus, themes }, null, 2));
  console.log('NATIVE EDITOR ACCESSIBILITY: PASS (Chromium AX tree and keyboard flow; not speech-engine acceptance)');
} finally { await browser.close(); }
