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
editor-to-rust = Show this file in the Rust editor (dioxus-code-editor)
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
