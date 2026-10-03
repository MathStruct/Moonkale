
## Commands (the palette and keybindings)

cmd-workspace-openFolder = File: Open Folder…
cmd-workspace-closeFolder = File: Close Folder
cmd-remote-open = File: Open Remote Folder… (SSH)
cmd-remote-close = File: Disconnect Remote
cmd-server-connect = File: Connect to Server…
cmd-server-disconnect = File: Disconnect Server
cmd-file-new = File: New File…
cmd-file-save = File: Save
cmd-file-saveAll = File: Save All
cmd-editor-closeAll = File: Close All Editors
cmd-view-toggleSide = View: Toggle Side Bar
cmd-view-toggleBottom = View: Toggle Bottom Panel
cmd-editor-find = Edit: Find
cmd-editor-replace = Edit: Replace
cmd-editor-rename = Edit: Rename Symbol
cmd-editor-codeActions = Edit: Code Actions
cmd-editor-definition = Edit: Go to Definition
cmd-editor-references = Edit: Find References
cmd-editor-toggleComment = Edit: Toggle Comment
cmd-editor-foldAll = View: Fold All
cmd-editor-unfoldAll = View: Unfold All
cmd-editor-close = File: Close Editor
cmd-view-settings = File: Settings…
cmd-edit-undo = Edit: Undo
cmd-edit-redo = Edit: Redo
cmd-view-palette = View: Command Palette…
cmd-view-quickOpen = Go to File…
cmd-search-workspace = Search: Find in Workspace…
cmd-view-newWindow = View: New Window
cmd-view-newTerminal = View: New Terminal
cmd-view-resetLayout = View: Reset Layout
cmd-view-panel-explorer = View: Show Sources
cmd-view-panel-search = View: Show Search
cmd-view-panel-graph = View: Show Graph
cmd-view-panel-terminal = View: Show Terminal
cmd-view-panel-agent = View: Show Agent
cmd-help-about = Help: About Moonkale
cmd-view-show-panel = View: Show { $panel }

## Menus

menu-new-file = New File…
menu-open-folder = Open Folder…
menu-close-folder = Close Folder
menu-open-remote = Open Remote Folder…
menu-disconnect-remote = Disconnect Remote
menu-connect-server = Connect to Server…
menu-disconnect-server = Disconnect Server
menu-new-flow = New Flow…
menu-settings = Settings…
menu-save = Save
menu-save-all = Save All
menu-close-all = Close All Editors
menu-close-editor = Close Editor
menu-exit = Exit
menu-palette = Command Palette…
menu-go-to-file = Go to File…
menu-toggle-side = Toggle Side Bar
menu-toggle-bottom = Toggle Bottom Panel
menu-toggle-wrap = Toggle Word Wrap
menu-fold-all = Fold All
menu-unfold-all = Unfold All
menu-new-window = New Window
menu-new-terminal = New Terminal
menu-reset-layout = Reset Layout
menu-devtools = Toggle Developer Tools
menu-undo = Undo
menu-redo = Redo
menu-find = Find
menu-replace = Replace
menu-find-workspace = Find in Workspace…
menu-rename = Rename Symbol
menu-code-actions = Code Actions
menu-definition = Go to Definition
menu-references = Find References
menu-toggle-comment = Toggle Comment
menu-about = About Moonkale
menu-shortcuts = Keyboard Shortcuts
menu-docs = Documentation
menu-file = File
menu-edit = Edit
menu-view = View
menu-help = Help

## Explorer

explorer-title = Sources
explorer-delete-confirm = Delete { $name }? (kept in .moonkale/trash)
explorer-delete = Delete
cancel = Cancel
explorer-path-placeholder = folder path (blank = default root)
explorer-opening = Opening…
explorer-open = Open
explorer-empty = Open a folder to browse its files.
explorer-read-only = read-only
explorer-not-watched = Not watched for changes — refresh
explorer-refresh-source = Refresh { $name }
explorer-db-failed = Could not open database: { $error }
explorer-binary = { $name } is a binary file
explorer-presence = { $name } has this open
explorer-terminal-here = New terminal here
explorer-new-folder = New Folder…
explorer-rename = Rename…
explorer-delete-dots = Delete…
explorer-open-terminal = Open in Terminal
explorer-refresh = Refresh
explorer-file-name = file name
explorer-folder-name = folder name
explorer-new-name = new name

## Search, palette, terminal chooser

search-title = Search
search-no-folder = Open a folder first.
search-replaced =
    Replaced { $n ->
        [one] { $n } occurrence
       *[other] { $n } occurrences
    } of { $needle }
search-placeholder = Search files… (Enter)
search-go = Go
search-replace-with = Replace with…
search-preview = Preview
search-preview-none = No literal occurrences of the query in the found files.
search-preview-head =
    Replace { $n ->
        [one] { $n } occurrence
       *[other] { $n } occurrences
    } of “{ $query }” with “{ $replacement }” in:
search-open-unsaved =  — open, stays unsaved
search-replace-all = Replace all
search-hint = Keyword + semantic search over the open folder.
search-no-matches = No matches.
palette-open-failed = Open failed: { $error }
palette-commands = Type a command…
palette-files = Go to file (append :line)…
chooser-title = Open the terminal with…
chooser-hint = Two terminal panels are enabled. Settings → Terminal → Implementation makes this permanent.
chooser-xterm =  — the JavaScript terminal (default)
chooser-native =  — the Dioxus-rendered terminal (no JavaScript)
chooser-remember = Remember my choice

## Settings

settings-title = Settings
provider-mock = mock (offline)
provider-claude-code = Claude Code (subscription, no API key)
provider-openai = OpenAI-compatible (OpenAI, Mistral, …)
provider-ollama = Ollama (local)
settings-form = Form
settings-changes-go-to = changes go to: 
settings-user-title = This machine (all folders)
settings-user = user
settings-workspace-title = This folder (.moonkale/settings.json)
settings-workspace = workspace
settings-not-persisted = User settings are not persisted on this platform; workspace settings still are.
settings-ignored-title = A folder's .moonkale/settings.json is data, not authority: it may not set a language model or agent, auto-approve writes, turn on embeddings, grant permissions, name a shell or SSH hosts.
settings-ignored = This folder's settings tried to set { $fields } — ignored: only your user settings decide those.
settings-agents = Agents
settings-agents-hint = Saved agents: a name and a language model each. One runs by default; every session in the Agent panel can pick another. Keys are never stored in settings — they come from MOONKALE_SECRET_<NAME>, ANTHROPIC_API_KEY / OPENAI_API_KEY, or the secrets file (desktop) / the server's (web).
settings-add-agent = Add agent
settings-secret-name = secret name (e.g. openai)
settings-api-key = API key
settings-secret-stored = stored secret { $name }
settings-secret-not-stored = not stored: { $error }
settings-store-secret = Store secret
settings-embedding-model = Embedding model
settings-embedding-none = none (keyword search only)
settings-embedding-title = Served by the Default agent's provider
settings-use-embeddings = Use embeddings when an embedding model is configured (applies to folders opened afterwards)
settings-which-extension = Which extension
settings-code-editor = Code editor
settings-editor-codemirror = CodeMirror (JavaScript; language servers, wiki-links, wrap)
settings-editor-native = Rust (dioxus-code-editor; tree-sitter for every core language, no LSP yet)
settings-terminal = Terminal
settings-terminal-ask = ask each time both are enabled
settings-terminal-xterm = xterm.js (JavaScript)
settings-terminal-native = Rust (vt100 grid, Dioxus rows)
settings-which-hint = Only extensions that are switched on count; a choice that is off falls back to the other. Switch extensions on and off in the Extensions panel (puzzle icon), where each extension's own settings are too.
settings-you = You
settings-name = Name
settings-name-placeholder = shown in history and presence
settings-appearance = Appearance
settings-language = Language
settings-language-system = System ({ $lang })
settings-theme = Theme
settings-theme-dark = Dark
settings-theme-light = Light
settings-theme-system = Follow system
settings-keybindings = Keybindings
settings-keybindings-hint = Ctrl in a binding means Cmd on macOS (and only Cmd — Ctrl stays free for Emacs-style cursor keys). Empty = unbound; a scope only stores the bindings changed there.
settings-custom = custom
settings-unbound = unbound
settings-not-a-keybinding = Not a keybinding: { $value } (try Ctrl+Shift+P, F12, Alt+ArrowUp)
settings-remembered = Remembered
settings-remembered-line = Recent folders: { $recent } · saved connections: { $connections } · layout saved: { $layout } · open documents: { $open }
settings-json-user = User settings
settings-this-machine = this machine
settings-json-workspace = Workspace file
settings-no-folder = (no folder open)
settings-json-env = Environment overrides
settings-rename-agent = Rename this agent
settings-runs-by-default = runs by default
settings-forget-agent = Forget this agent
settings-remove = Remove
settings-provider = Provider
settings-model = Model
settings-model-claude = the subscription's default (or e.g. claude-sonnet-5)
settings-model-default = provider default
settings-command = Command
settings-command-placeholder = claude (on PATH)
settings-permissions = Permissions
settings-mode-plan = plan — read and propose only
settings-mode-default = default — Claude Code's own rules (.claude/settings.json)
settings-mode-accept = acceptEdits — may edit files in the folder
settings-mode-bypass = bypassPermissions — everything (careful)
settings-allowed-tools = Allowed tools
settings-allowed-tools-placeholder = e.g. Read,Grep,Bash(git:*)
settings-endpoint = Endpoint
settings-secret = Secret name
settings-secret-placeholder = defaults to the provider name
claude-login-status = Claude Code: sign in in the browser, then paste the code into the terminal tab; press Check again afterwards
claude-checking = checking the claude CLI…
claude-no-status = status not available on this platform
claude-login-title = Runs `claude auth login` in a terminal tab
claude-login-again = Log in again
claude-login = Log in
claude-check-again = Check again
claude-hint = Runs the claude CLI headless in the open folder with your subscription login; no key, nothing stored by Moonkale. Its tool calls appear in the transcript as ▸ lines. On the web the CLI runs — and is logged in — on the server.

## Extensions panel

extensions-title = Extensions
extensions-changes-go-to = Changes go to 
extensions-user-settings = user settings
extensions-this-folder = this folder
extensions-catalogue-before = What each extension is, which tier it belongs to and where it runs: the 
extensions-catalogue = Extension Catalogue
extensions-catalogue-after = .
extensions-hint = Optional features load only when switched on. Permissions are what an extension may do; untick to restrict it. Permissions are always yours (user settings): a folder cannot grant them.
extensions-core = core
extensions-opt-in = opt-in
extensions-wasm = Installed (wasm)
extensions-wasm-hint = Third-party modules from ~/.config/moonkale/extensions and <folder>/.moonkale/extensions. Off until enabled; no permission is granted until ticked.
extensions-wasm-desc = { $description } · commands: { $commands }

## Remote and server dialogs

connect = Connect
remote-name-needed = Give the connection a name to save it
remote-title = Open Remote Folder
remote-hint-ssh = Opens a folder on another machine through your system ssh. Authentication is ssh's own — keys, agent, passwords and host-key checks work exactly as in a terminal; whatever ssh asks appears in the Terminal panel, and Moonkale never sees a secret.
remote-hint-host = Host: what you would type after ssh — a name or user@host, an alias from ~/.ssh/config (offered below), with any ssh options in front (-p 2222, -i ~/.ssh/key, -J jumphost, -o …); VAR=value words at the start are set in ssh's environment.
remote-hint-server = The first time per host and version, Moonkale's own server (about 150 MB) is copied to the host into ~/.local/share/moonkale/server/ and started there for this session only: the folder, its index, git, language servers and terminals then run on that machine; the editor and your API keys stay here. Closing the folder ends the session and the server.
remote-saved = Saved connection
remote-pick = — type a host below, or pick one —
remote-host = Host (as typed after ssh)
remote-folder = Folder on that machine
remote-save-name = name to save this connection as
remote-save-title = Remember host and folder under this name (user settings)
remote-forget = Forget
server-title = Connect to Server
server-hint = Use a running Moonkale server (moonkale-server or dx serve with MOONKALE_TOKEN) from this app: its folder, index, git, language servers, terminals and agent sessions run there; the editor and your API keys stay here. Over the network the server should use HTTPS or a tunnel — see Remote and Server Modes.
server-url = Server URL
server-token = Access token (MOONKALE_TOKEN)
server-token-placeholder = empty for a dev server without a token

## Frame, shell, status bar

status-no-new-window = New window is not available on this platform
status-no-terminal = No terminal extension is enabled (Extensions panel)
status-could-not-create = Could not create { $name }: { $error }
status-could-not-move = Could not move document here: { $error }
drop-target = Drop (or click) to move { $name } into this window
drop-banner = { $name } was dragged from another window.
drop-move-here = Move it here
dismiss = Dismiss
status-open-folder-failed = Open folder failed: { $error }
status-nothing-in-tile = Nothing to show in the { $tile } area
status-saved =
    Saved { $n ->
        [one] { $n } document
       *[other] { $n } documents
    }
status-kept-open =
    { $n ->
        [one] { $n } unsaved document left open
       *[other] { $n } unsaved documents left open
    }
status-no-folder = No folder is open
status-about = Moonkale { $version } — graph-native code and knowledge editor · mathstruct.github.io/Moonkale
editor = Editor
rail-title = { $label } — click again to hide
people = People
people-nobody = Nobody else is here
people-here = Here: { $names }
people-nobody-folder = Nobody else is looking at this folder
status-no-folder-open = No folder open
status-server-client = This app is a client of a Moonkale server
status-remote = Remote folder over SSH — { $phase }
status-lsp = Language server
status-others = { $names } — in this folder too
status-window-title = This window: { $id }. Other windows of this session are counted once they answer.
status-windows =
    { $n ->
        [one] { $n } window
       *[other] { $n } windows
    }
more = More
status-ready = Ready

## Workbench chrome (dioxus-workbench)

wb-tab-group = Panel group
wb-tab-group-labelled = Panel group: { "{" }panel{ "}" }
wb-tab-hint = Drag to dock. Alt+Shift+Arrows split; Alt+Shift+Page keys move.
wb-close-tab = Close { "{" }title{ "}" }
wb-empty-group = Empty group
wb-empty-title = Empty panel group
wb-empty-hint = Drag a panel tab here, or close this group.
wb-empty-keys = Alt+Shift+Arrows split · Alt+Shift+Page keys move · Arrows switch tabs
wb-split-right = Split panel group right
wb-split-right-hint = Split right (Alt+Shift+Right)
wb-split-down = Split panel group down
wb-split-down-hint = Split down (Alt+Shift+Down)
wb-close-empty = Close empty panel group
wb-close-empty-hint = Close empty group
wb-splitter = Resize panel groups
wb-splitter-hint = Drag to resize. Arrow keys resize; Shift moves farther; double-click resets.
