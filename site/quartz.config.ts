import { siteConfig } from "./mathstruct/config"

// The plugins, theme and fixes shared by all MathStruct sites are in the preset
// (https://github.com/MathStruct/quartz, mathstruct/config.ts); this file only says what is
// particular to Moonkale.
export default siteConfig({
  pageTitle: "Moonkale",
  baseUrl: "mathstruct.org/Moonkale",
  // `Prompt*` are the working prompts that drove the design sessions; kept in the vault for
  // provenance, not published
  ignorePatterns: ["Prompt*", "prompts"],
  // notes may mix Typst and LaTeX: each formula is tried with Typst first, anything Typst
  // rejects falls through to KaTeX. Pin a page with `math: typst` or `math: latex`.
  math: ["typst", "katex"],
})
