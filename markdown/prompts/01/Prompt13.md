Especially w.r.t editor and graph behaviour I might ask a question:
Can we have a case selector?

Example: the ZED editor wanted to render very big text file fluently, so maybe we do:
- CodeMirror with rich display in normal cases
- Basic text editor with GPU support in stupid big cases.


Same for graph:
- CPU variant (like right now) in normal cases. 
- GPU variant (or sparsified variant) for over the top big cases. (1 million+ )

Please document in vault.