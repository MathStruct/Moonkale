// Syntax highlighting per language id (spec 010). Rust decides the id
// (`Node::language_hint`); this file only maps it to a CodeMirror language
// extension — a *view* concern, the one exception to "no language knowledge
// in JS" (decision P-093: tree-sitter is native-only, so highlighting in the
// webview era comes from Lezer grammars, symbols and semantics stay in Rust).
// Grammars are Lezer where one exists, legacy stream modes otherwise.
import { type Extension } from "@codemirror/state"
import { StreamLanguage, LanguageSupport } from "@codemirror/language"
import { rust } from "@codemirror/lang-rust"
import { javascript } from "@codemirror/lang-javascript"
import { cpp } from "@codemirror/lang-cpp"
import { go } from "@codemirror/lang-go"
import { json } from "@codemirror/lang-json"
import { markdown, markdownLanguage } from "@codemirror/lang-markdown"
import { sql, PostgreSQL, SQLite } from "@codemirror/lang-sql"
import { yaml } from "@codemirror/lang-yaml"
import { python } from "@codemirror/lang-python"
import { css } from "@codemirror/lang-css"
import { html } from "@codemirror/lang-html"
import { julia } from "@codemirror/legacy-modes/mode/julia"
import { toml } from "@codemirror/legacy-modes/mode/toml"
import { cypher } from "@codemirror/legacy-modes/mode/cypher"
import { shell } from "@codemirror/legacy-modes/mode/shell"
import { lua } from "@codemirror/legacy-modes/mode/lua"
import { typst_lezer } from "codemirror-lang-typst/lezer"

const stream = (spec: Parameters<typeof StreamLanguage.define>[0]) => new LanguageSupport(StreamLanguage.define(spec))

/** Language id (Rust's `language_hint`) → extension, or `null` for plain text.
 *  A grammar that throws while being set up degrades to plain text rather
 *  than taking the whole editor down (spec 016). */
export function languageExtension(id: string | null | undefined): Extension | null {
  try {
    return pick(id)
  } catch (e) {
    console.error(`codemirror: grammar for ${id} failed, plain text instead:`, e)
    return null
  }
}

function pick(id: string | null | undefined): Extension | null {
  switch (id) {
    case "rust": return rust()
    case "javascript": return javascript({ jsx: true })
    case "typescript": return javascript({ jsx: true, typescript: true })
    case "c": case "cpp": return cpp()
    case "go": return go()
    case "json": return json()
    case "markdown": return markdown({ base: markdownLanguage, codeLanguages: [] })
    case "sql": return sql({ dialect: SQLite })
    case "postgres": return sql({ dialect: PostgreSQL })
    case "yaml": return yaml()
    case "python": return python()
    case "css": return css()
    case "html": return html()
    case "julia": return stream(julia)
    case "toml": case "pixi": return stream(toml)
    case "cypher": return stream(cypher)
    case "shell": case "bash": return stream(shell)
    case "lua": return stream(lua)
    case "typst": return typst_lezer()
    // No grammar yet (spec 010 / Core Languages): lean, nix, helixql, typeql, graphql.
    default: return null
  }
}
