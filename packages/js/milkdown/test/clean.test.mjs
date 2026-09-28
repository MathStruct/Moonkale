// Run: npm test (bundles src/clean.ts to test/clean.mjs, which git ignores).
import { test } from "node:test"
import assert from "node:assert/strict"
import { dropEmptyLineBreaks as clean } from "./clean.mjs"

test("drops <br /> lines and collapses blank runs", () => {
  assert.equal(clean("a\n\n<br />\n\n<br />\n\nb"), "a\n\nb")
  assert.equal(clean("a\n<br>\nb"), "a\nb")
  assert.equal(clean("a\n  <BR/>  \nb"), "a\nb")
})

test("keeps <br /> inside a line and inside fenced code", () => {
  assert.equal(clean("one<br />two"), "one<br />two")
  const code = "```html\n<br />\n\n\n\nx\n```\n\n<br />\n\nafter"
  assert.equal(clean(code), "```html\n<br />\n\n\n\nx\n```\n\nafter")
  const tilde = "~~~\n<br />\n~~~"
  assert.equal(clean(tilde), tilde)
})

test("a longer fence is closed only by a fence at least as long", () => {
  const md = "````\n```\n<br />\n```\n````\n<br />"
  assert.equal(clean(md), "````\n```\n<br />\n```\n````")
})

test("text without <br /> is unchanged", () => {
  const md = "# T\n\npara\n\n- a\n- b\n"
  assert.equal(clean(md), md)
})
