---
title: "Languages and Themes"
tags: [i18n, themes, ui, spec-030]
---
How the UI speaks English, German and Chinese and how it is coloured — spec [[030]], done 2026-10-03 on the `spec-030` branch.

## Languages
- **Choose** in Settings → Appearance → Language: *System* (the default, empty `Settings.language`), English, Deutsch, 中文 (简体). The change applies live, no restart.
- **System language**: `moonkale_ext_api::i18n::system_language()` (`sys-locale`; its `js` feature reads `navigator.languages` on the web). It is read on the *client* in `load_user_settings` and stored in `SettingsState.system_language`, which starts as `"en"` — the server's SSR and the first client render must agree (hydration), so a German browser sees English for one frame.
- **Strings**: Fluent (`fluent-bundle` 0.16). Every crate with UI text has `locales/{en,de,zh-CN}.ftl` and a `pub(crate) static L: Locales`; look up with `t!(ws, L, "id")` or `t!(ws, L, "id", count = n)`. English is required, a missing id falls back to English, then to the id itself. Ids are keys, not English text.
- **Extensions** bring their own strings through `Extension::locales()`; the Extensions panel shows `extension-name` / `extension-description` from them.
- **Checks**: `tools/check-strings.py` (CI lint) parses every `.ftl`, checks that each id used in Rust exists in English and that German and Chinese have the same ids and the same variables. 618 strings in 15 crates at the time of writing.
- **Rule (P-151)**: `Workspace::lang()` subscribes to the settings signal. Code that runs inside an untracked command effect wraps its calls in `workspace::untracked(…)`, where `lang()` peeks.
- **Not yet**: German and Chinese are machine-quality drafts awaiting a native review (Daniel for German). The JS bundles' few own phrases (CodeMirror's search panel) are still English.

## Themes
- **Choose** in Settings → Appearance → Theme: Dark (default), Light, Follow system, and every theme file found.
- **Tokens**: every colour is a `--mk-*` custom property, defined for Dark and Light in `packages/shell/src/theme.rs` (`TOKENS`, about 94) and nowhere else. dioxus-workbench's `--wb-*` variables are mapped onto them under `:root .wb-shell, :root .wb-workspace` (higher specificity than the workbench's own palette). `tools/check-colors.py` (ceiling 0, in `tools/colors-max.txt`) fails CI on any hex/`rgb()`/`hsl()` colour outside the theme module (vendored `xterm.css`, `milkdown.css` excepted).
- **How it applies**: the built-in sheet is rendered once as `style { dangerous_inner_html }` (P-152) with `:root[data-theme=dark|light]` rules and a `prefers-color-scheme` rule for *Follow system*; the frame sets `data-theme` on `<html>` by eval and dispatches a `moonkale-theme` event. `ws.shell.theme_light` tells Rust code (the native code editor) whether the current theme is light.
- **Editors**: CodeMirror builds its theme from the tokens (`packages/js/codemirror/src/theme.ts`, `themeFromTokens`), xterm reads the CSS variables and re-themes on the `data-theme` change, the graph renderer takes its clear and label colours through `GraphView::set_theme`. Milkdown, tables, the flow canvas and KaTeX use the tokens directly in CSS.
- **Theme files**: JSON, `{"name": "Paper", "base": "light", "tokens": {"canvas": "#fbf8f1", "accent": "#8a5a00"}}` — the base's palette with the named tokens overridden; unknown names are ignored, values that could close a CSS rule (`; { } < >`) are dropped. Sources: `<config>/moonkale/themes/*.json` (desktop and mobile read their own; the web client asks the server, `POST /api/themes`, via the `ThemeFiles` service) and `Extension::themes()`. Their CSS goes into a separate `#mk-theme-extra` style element.

## Fonts
The font stacks end in `"Noto Sans CJK SC", "PingFang SC", "Microsoft YaHei"` so Chinese renders where one of them is installed. A machine without any CJK font shows boxes (tofu) — on Arch install `noto-fonts-cjk` (see [[Development]]).

## Tests
- `packages/shell` unit tests: theme parsing, the `safe` filter, the CSS blocks, the workbench templates keeping their `{title}`/`{panel}` placeholders.
- E2E `appearance.mjs`: a German browser locale gives German labels; switching to Chinese and English live; Light tokens and computed colours; Follow system with `emulateMedia`; a `Paper` theme file in the config dir appears and applies.
