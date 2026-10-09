lsp-starting = { $language }: starting language server…
lsp-ready = { $server }: ready
lsp-exited = { $language }: language server exited
lsp-init-failed = { $language }: initialize failed: { $error }
editor-rename-needs-lsp = Rename needs a language server for this file
editor-definition-outside = Definition is outside the folder: { $uri }
editor-no-definition = No definition found
editor-applied =
    Applied edits to { $n ->
        [one] { $n } file
       *[other] { $n } files
    }
editor-edit-failed = Edit failed: { $error }
editor-nothing-to-rename = Nothing to rename here
editor-plain-text = plain text
editor-unsaved = Unsaved changes
editor-wrap-title = Wrap long lines (Alt+Z)
editor-wrap = Wrap
editor-save = Save
editor-reload-title = Discard edits and reload from the source
editor-reload = Reload
editor-to-rust = Show this file in the Rust editor
editor-rename-prompt = Rename “{ $word }” to:
editor-rename = Rename
editor-cancel = Cancel
editor-no-actions = No code actions here.
editor-actions = Code actions:
editor-references =
    { $n ->
        [one] { $n } reference
       *[other] { $n } references
    }
editor-conflict = The file changed outside the editor. Reload to see the new content (your edits will be lost) or save again to overwrite.
editor-save-failed = Save failed: { $error }
editor-failed = The editor could not start.
editor-failed-hint =  This is a bug — please report it with the message below (the file itself is fine; Reload retries).
editor-loading = Loading editor…

editor-to-codemirror = Show this file in CodeMirror
editor-document-closed = This document is no longer open.
editor-rust-meta = { $language } · v{ $version } · Rust editor
editor-rust-label = Code editor
editor-find = Find
editor-match-case = Match case
editor-whole-word = Whole word
editor-find-previous = Previous
editor-find-next = Next
editor-replace = Replace
editor-replace-all = Replace all
editor-toggle-comment = Toggle comment
editor-fold = Fold
editor-unfold = Unfold
editor-fold-all = Fold all
editor-unfold-all = Unfold all
editor-indent-style = Indentation
editor-indent-width = Width
editor-language-default = Language default
editor-indent-spaces = Spaces
editor-indent-tabs = Tabs

editor-search-regex = Regular expression
editor-search-count = { $current } of { $total }
editor-search-captures-help = Use $1 or $name for captures; $$ inserts a dollar sign.
editor-search-invalid = Invalid regular expression: { $error }

editor-diagnostics = Diagnostics
editor-hover = Show hover information
editor-completion = Complete (Ctrl+Space)
editor-definition = Go to definition (F12)
editor-definition-needs-lsp = Go to definition needs a language server for this file
editor-definition-failed = Go to definition failed: { $error }
editor-definition-timeout = Go to definition timed out

editor-rename-shortcut = Rename symbol (F2)
editor-rename-timeout = Rename request timed out

editor-actions-shortcut = Code actions (Mod-.)
editor-references-shortcut = Find references (Shift-F12)
editor-tools-needs-lsp = This action needs a language server for the file
editor-tools-timeout = Language server request timed out
editor-tools-loading = Loading…
editor-reference-outside = This reference is outside the folder

editor-preview = Preview
editor-preview-title = Preview Markdown formatting; wraps source while enabled.
