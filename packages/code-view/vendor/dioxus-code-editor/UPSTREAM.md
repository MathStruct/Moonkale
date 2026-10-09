# Upstream and local changes

This crate is a small source fork of [`dioxus-code-editor`](https://github.com/ealmloff/dioxus-code), version 0.1.2, distributed under the MIT license (as declared by the upstream crate metadata).

Moonkale's local change binds the contents of the syntax and plain-text editor textareas through the live `value` property. Upstream rendered the text as textarea child content, which did not synchronize the browser's `.value` after programmatic undo or reload updates. The fork otherwise keeps the upstream component and stylesheet intact.

When updating from upstream, reapply and verify the `value` binding in both textarea render paths. Keep this fork temporary if upstream adds an equivalent controlled-value fix.
