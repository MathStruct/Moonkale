Additionally to CodeMirror extension build an extension around:
https://crates.io/crates/dioxus-code-editor

And enable switching code editors like we had with switching termminals.

One question for both extensions:
- Is the code editor aware over which word the cursor currently hovers?
Please document then implement.

Currently the Rust terminal does load, however everything below a certain line gets cut.