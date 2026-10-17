// WP-69: commands live behind the Rust panel's disclosure. Use real UI activation.
export async function clickNativeCommand(command) {
  await command.waitFor({ state: "attached" });
  const disclosure = command.locator('xpath=ancestor::details[contains(@class,"mk-native-more")]');
  if (await disclosure.count() && await disclosure.getAttribute('open') === null) {
    await disclosure.locator('summary').click();
  }
  await command.click();
}
