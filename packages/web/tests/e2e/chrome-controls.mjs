// Reach a folded control through its real disclosure, menu button or rail More.
// This never changes density, grants chrome or uses DOM clicks/visibility overrides.
export async function revealControl(target) {
  const page = target.page();
  const description = target.toString();
  if (!await target.count()) {
    if (/mk-menu-button|name: ['"](?:File|Edit|View|Help|Git)['"]/.test(description)) {
      const menu = page.locator('.mk-main-menu');
      if (await menu.isVisible() && await menu.getAttribute('aria-expanded') === 'false') await menu.click();
    }
    if (/mk-rail-/.test(description)) {
      const more = page.locator('#mk-rail-more');
      if (await more.isVisible()) await more.click();
    }
  }
  await target.waitFor({state: 'attached'});
  if (await target.locator('xpath=self::summary').count()) return;
  const ancestors = target.locator('xpath=ancestor::details');
  for (let i = await ancestors.count() - 1; i >= 0; i--) {
    const details = ancestors.nth(i);
    if (await details.getAttribute('open') === null) await details.locator(':scope > summary').click();
  }
}
export async function control(target, method, ...args) {
  if (typeof target.goto === 'function') {
    // Page selector actions historically use the first match; Locator actions stay strict.
    target = target.locator(args.shift()).first();
  }
  // Content ElementHandles retain their original action semantics.
  if (typeof target.page !== 'function') return target[method](...args);
  await revealControl(target);
  return target[method](...args);
}
