extension-name = Flow editor
extension-description = Drag-and-drop blocks wired by typed ports (*.flow.json). Block libraries come from other extensions.
flow-generated = Generated { $file } — run: { $run }
flow-unsaved = Unsaved changes
flow-counts = { $blocks } blocks · { $wires } wires
flow-issues =
     · { $n ->
        [one] { $n } issue
       *[other] { $n } issues
    }
flow-generate-title = Write the { $language } file next to this flow
flow-generate = Generate { $language }
flow-layout = Layout
flow-fit = Fit
flow-save = Save
flow-no-libraries = No block libraries are enabled. Turn one on in Settings → Extensions (e.g. Lux.jl).
flow-broken = This file is not a valid flow ({ $error }). Nothing is written to it; fix it in a text editor, then press Reload.
flow-reload = Reload
flow-reload-title = Discard edits and reload from the file
