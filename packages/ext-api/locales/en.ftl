
## Workspace status messages

restored = Restored as an unsaved edit — save to keep it
remote-unavailable = Remote folders are not available on this platform
remote-need-host = Remote: a host and a path are needed
remote-error = Remote: { $error }
remote-connecting = Remote: connecting to { $host }…
remote-prompt = ssh { $host }: { $line } — answer in the terminal
remote-uploading = Remote: { $host } has no Moonkale server yet — uploading it (once per version)…
remote-starting = Remote: starting the server on { $host }…
remote-connected = Remote: connected to { $host }, opening { $path }…
remote-open-failed = Remote: { $host } is connected but { $path } did not open: { $error }
remote-disconnected = Remote: disconnected from { $host }
server-unavailable = Connecting to a server is not available on this platform
server-need-url = Server: a URL is needed
server-error = Server: { $error }
server-connected = Connected to { $url }; opening its folder…
server-open-failed = Server { $url }: connected, but its folder did not open: { $error }
server-disconnected = Disconnected from { $url }
close-dirty =
    { $name }: save or reload { $n ->
        [one] { $n } unsaved document
       *[other] { $n } unsaved documents
    } before closing it
closed = Closed { $name }
no-folder-dialog = No folder dialog on this platform — type a path in Sources
opened = Opened { $name }
refreshed = Refreshed { $name }
extensions-not-scanned = Extensions not scanned: { $error }
settings-not-loaded = Settings not loaded: { $error }
reopen-failed = Could not reopen the last folder: { $error }
settings-not-saved = Settings not saved: { $error }
workspace-settings-ignored = Workspace settings ignored: { $error }
workspace-settings-not-saved = Workspace settings not saved: { $error }
not-found-in-folders = { $path }: not found in the open folders
saved = Saved { $name }
save-failed = Save failed: { $error }
created = Created { $name }
renamed = Renamed { $from } → { $to }
deleted = Deleted { $name } (kept in .moonkale/trash)
reloaded = Reloaded from disk
program-unavailable = Running a program in a terminal is not available on this platform
program-failed = Could not start { $program }: { $error }
wiki-unresolved = [[{ $target }]] does not resolve to a page
wiki-renamed =
    Renamed { $from } → { $to }: links updated in { $n ->
        [one] { $n } file
       *[other] { $n } files
    }
moved-to-window = Moved to window { $window }
dragging = Dragging { $name } — drop it on another Moonkale window to move it there

move-unsaved = Unsaved document kept here — save before moving it to another window.
