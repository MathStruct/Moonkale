
## Commands (the palette and keybindings)

cmd-workspace-openFolder = Datei: Ordner öffnen …
cmd-workspace-closeFolder = Datei: Ordner schließen
cmd-remote-open = Datei: Entfernten Ordner öffnen … (SSH)
cmd-remote-close = Datei: Entfernte Verbindung trennen
cmd-server-connect = Datei: Mit Server verbinden …
cmd-server-disconnect = Datei: Server trennen
cmd-file-new = Datei: Neue Datei …
cmd-file-save = Datei: Speichern
cmd-file-saveAll = Datei: Alle speichern
cmd-editor-closeAll = Datei: Alle Editoren schließen
cmd-view-toggleSide = Ansicht: Seitenleiste ein/aus
cmd-view-toggleBottom = Ansicht: Unteres Panel ein/aus
cmd-editor-find = Bearbeiten: Suchen
cmd-editor-replace = Bearbeiten: Ersetzen
cmd-editor-rename = Bearbeiten: Symbol umbenennen
cmd-editor-codeActions = Bearbeiten: Code-Aktionen
cmd-editor-definition = Bearbeiten: Zur Definition
cmd-editor-references = Bearbeiten: Verweise suchen
cmd-editor-toggleComment = Bearbeiten: Kommentar ein/aus
cmd-editor-foldAll = Ansicht: Alles einklappen
cmd-editor-unfoldAll = Ansicht: Alles ausklappen
cmd-editor-close = Datei: Editor schließen
cmd-view-settings = Datei: Einstellungen …
cmd-edit-undo = Bearbeiten: Rückgängig
cmd-edit-redo = Bearbeiten: Wiederholen
cmd-view-palette = Ansicht: Befehlspalette …
cmd-view-quickOpen = Gehe zu Datei …
cmd-search-workspace = Suche: Im Arbeitsbereich suchen …
cmd-view-newWindow = Ansicht: Neues Fenster
cmd-view-newTerminal = Ansicht: Neues Terminal
cmd-view-resetLayout = Ansicht: Layout zurücksetzen
cmd-view-panel-explorer = Ansicht: Quellen anzeigen
cmd-view-panel-search = Ansicht: Suche anzeigen
cmd-view-panel-graph = Ansicht: Graph anzeigen
cmd-view-panel-terminal = Ansicht: Terminal anzeigen
cmd-view-panel-agent = Ansicht: Agent anzeigen
cmd-help-about = Hilfe: Über Moonkale
cmd-view-show-panel = Ansicht: { $panel } anzeigen

## Menus

menu-new-file = Neue Datei …
menu-open-folder = Ordner öffnen …
menu-close-folder = Ordner schließen
menu-open-remote = Entfernten Ordner öffnen …
menu-disconnect-remote = Entfernte Verbindung trennen
menu-connect-server = Mit Server verbinden …
menu-disconnect-server = Server trennen
menu-new-flow = Neuer Flow …
menu-settings = Einstellungen …
menu-save = Speichern
menu-save-all = Alle speichern
menu-close-all = Alle Editoren schließen
menu-close-editor = Editor schließen
menu-exit = Beenden
menu-palette = Befehlspalette …
menu-go-to-file = Gehe zu Datei …
menu-toggle-side = Seitenleiste ein/aus
menu-toggle-bottom = Unteres Panel ein/aus
menu-toggle-wrap = Zeilenumbruch ein/aus
menu-fold-all = Alles einklappen
menu-unfold-all = Alles ausklappen
menu-new-window = Neues Fenster
menu-new-terminal = Neues Terminal
menu-reset-layout = Layout zurücksetzen
menu-devtools = Entwicklerwerkzeuge ein/aus
menu-undo = Rückgängig
menu-redo = Wiederholen
menu-find = Suchen
menu-replace = Ersetzen
menu-find-workspace = Im Arbeitsbereich suchen …
menu-rename = Symbol umbenennen
menu-code-actions = Code-Aktionen
menu-definition = Zur Definition
menu-references = Verweise suchen
menu-toggle-comment = Kommentar ein/aus
menu-about = Über Moonkale
menu-shortcuts = Tastenkürzel
menu-docs = Dokumentation
menu-file = Datei
menu-edit = Bearbeiten
menu-view = Ansicht
menu-help = Hilfe

## Explorer

explorer-title = Quellen
explorer-delete-confirm = { $name } löschen? (bleibt in .moonkale/trash)
explorer-delete = Löschen
cancel = Abbrechen
explorer-path-placeholder = Ordnerpfad (leer = Standardordner)
explorer-opening = Wird geöffnet …
explorer-open = Öffnen
explorer-empty = Öffne einen Ordner, um seine Dateien zu durchsuchen.
explorer-read-only = schreibgeschützt
explorer-not-watched = Änderungen werden nicht verfolgt – aktualisieren
explorer-refresh-source = { $name } aktualisieren
explorer-db-failed = Datenbank konnte nicht geöffnet werden: { $error }
explorer-binary = { $name } ist eine Binärdatei
explorer-presence = { $name } hat dies geöffnet
explorer-terminal-here = Neues Terminal hier
explorer-new-folder = Neuer Ordner …
explorer-rename = Umbenennen …
explorer-delete-dots = Löschen …
explorer-open-terminal = Im Terminal öffnen
explorer-refresh = Aktualisieren
explorer-file-name = Dateiname
explorer-folder-name = Ordnername
explorer-new-name = neuer Name

## Search, palette, terminal chooser

search-title = Suche
search-no-folder = Öffne zuerst einen Ordner.
search-replaced =
    { $n ->
        [one] { $n } Vorkommen
       *[other] { $n } Vorkommen
    } von { $needle } ersetzt
search-placeholder = Dateien durchsuchen … (Enter)
search-go = Los
search-replace-with = Ersetzen durch …
search-preview = Vorschau
search-preview-none = Die gefundenen Dateien enthalten die Suchanfrage nicht wörtlich.
search-preview-head = { $n } Vorkommen von „{ $query }“ durch „{ $replacement }“ ersetzen in:
search-open-unsaved =  – geöffnet, bleibt ungespeichert
search-replace-all = Alle ersetzen
search-hint = Stichwort- und semantische Suche im geöffneten Ordner.
search-no-matches = Keine Treffer.
palette-open-failed = Öffnen fehlgeschlagen: { $error }
palette-commands = Befehl eingeben …
palette-files = Gehe zu Datei (mit :Zeile) …
chooser-title = Terminal öffnen mit …
chooser-hint = Zwei Terminal-Panels sind aktiv. Einstellungen → Terminal → Implementierung legt die Wahl dauerhaft fest.
chooser-xterm =  – das JavaScript-Terminal (Standard)
chooser-native =  – das von Dioxus gerenderte Terminal (ohne JavaScript)
chooser-remember = Meine Wahl merken

## Settings

settings-title = Einstellungen
provider-mock = Attrappe (offline)
provider-claude-code = Claude Code (Abo, kein API-Schlüssel)
provider-openai = OpenAI-kompatibel (OpenAI, Mistral, …)
provider-ollama = Ollama (lokal)
settings-form = Formular
settings-changes-go-to = Änderungen gehen an: 
settings-user-title = Dieser Rechner (alle Ordner)
settings-user = Benutzer
settings-workspace-title = Dieser Ordner (.moonkale/settings.json)
settings-workspace = Arbeitsbereich
settings-not-persisted = Benutzereinstellungen werden auf dieser Plattform nicht gespeichert; Arbeitsbereich-Einstellungen schon.
settings-ignored-title = Die .moonkale/settings.json eines Ordners ist Daten, keine Befugnis: Sie darf kein Sprachmodell und keinen Agenten festlegen, keine Schreibzugriffe automatisch erlauben, keine Embeddings einschalten, keine Berechtigungen erteilen und keine Shell oder SSH-Hosts nennen.
settings-ignored = Die Einstellungen dieses Ordners wollten { $fields } setzen – ignoriert: Darüber entscheiden nur deine Benutzereinstellungen.
settings-agents = Agenten
settings-agents-hint = Gespeicherte Agenten: je ein Name und ein Sprachmodell. Einer läuft standardmäßig; jede Sitzung im Agent-Panel kann einen anderen wählen. Schlüssel stehen nie in den Einstellungen – sie kommen aus MOONKALE_SECRET_<NAME>, ANTHROPIC_API_KEY / OPENAI_API_KEY oder der Secrets-Datei (Desktop) bzw. der des Servers (Web).
settings-add-agent = Agent hinzufügen
settings-secret-name = Name des Secrets (z. B. openai)
settings-api-key = API-Schlüssel
settings-secret-stored = Secret { $name } gespeichert
settings-secret-not-stored = nicht gespeichert: { $error }
settings-store-secret = Secret speichern
settings-embedding-model = Embedding-Modell
settings-embedding-none = keines (nur Stichwortsuche)
settings-embedding-title = Vom Anbieter des Standard-Agenten bereitgestellt
settings-use-embeddings = Embeddings verwenden, wenn ein Embedding-Modell eingestellt ist (gilt für danach geöffnete Ordner)
settings-which-extension = Welche Erweiterung
settings-code-editor = Code-Editor
settings-editor-codemirror = CodeMirror (JavaScript; Language-Server, Wiki-Links, Umbruch)
settings-editor-native = Rust (dioxus-code-editor; tree-sitter für jede Kernsprache, noch kein LSP)
settings-terminal = Terminal
settings-terminal-ask = jedes Mal fragen, wenn beide aktiv sind
settings-terminal-xterm = xterm.js (JavaScript)
settings-terminal-native = Rust (vt100-Raster, Dioxus-Zeilen)
settings-which-hint = Es zählen nur eingeschaltete Erweiterungen; eine ausgeschaltete Wahl fällt auf die andere zurück. Erweiterungen schaltest du im Erweiterungen-Panel (Puzzle-Symbol) ein und aus; dort sind auch ihre eigenen Einstellungen.
settings-you = Du
settings-name = Name
settings-name-placeholder = wird im Verlauf und bei der Anwesenheit angezeigt
settings-appearance = Darstellung
settings-language = Sprache
settings-language-system = System ({ $lang })
settings-theme = Design
settings-theme-dark = Dunkel
settings-theme-light = Hell
settings-theme-system = Wie das System
settings-keybindings = Tastenkürzel
settings-keybindings-hint = Strg in einem Kürzel bedeutet unter macOS Cmd (und nur Cmd – Strg bleibt frei für Cursortasten im Emacs-Stil). Leer = nicht belegt; ein Bereich speichert nur die dort geänderten Kürzel.
settings-custom = angepasst
settings-unbound = nicht belegt
settings-not-a-keybinding = Kein Tastenkürzel: { $value } (z. B. Ctrl+Shift+P, F12, Alt+ArrowUp)
settings-remembered = Gemerkt
settings-remembered-line = Zuletzt geöffnete Ordner: { $recent } · gespeicherte Verbindungen: { $connections } · Layout gespeichert: { $layout } · offene Dokumente: { $open }
settings-json-user = Benutzereinstellungen
settings-this-machine = dieser Rechner
settings-json-workspace = Arbeitsbereich-Datei
settings-no-folder = (kein Ordner geöffnet)
settings-json-env = Umgebungsvariablen
settings-rename-agent = Diesen Agenten umbenennen
settings-runs-by-default = läuft standardmäßig
settings-forget-agent = Diesen Agenten vergessen
settings-remove = Entfernen
settings-provider = Anbieter
settings-model = Modell
settings-model-claude = Standard des Abos (oder z. B. claude-sonnet-5)
settings-model-default = Standard des Anbieters
settings-command = Befehl
settings-command-placeholder = claude (im PATH)
settings-permissions = Berechtigungen
settings-mode-plan = plan – nur lesen und vorschlagen
settings-mode-default = default – die eigenen Regeln von Claude Code (.claude/settings.json)
settings-mode-accept = acceptEdits – darf Dateien im Ordner bearbeiten
settings-mode-bypass = bypassPermissions – alles (Vorsicht)
settings-allowed-tools = Erlaubte Werkzeuge
settings-allowed-tools-placeholder = z. B. Read,Grep,Bash(git:*)
settings-endpoint = Endpunkt
settings-secret = Name des Secrets
settings-secret-placeholder = standardmäßig der Name des Anbieters
claude-login-status = Claude Code: Im Browser anmelden, dann den Code in den Terminal-Tab einfügen; danach „Erneut prüfen“ drücken
claude-checking = claude-CLI wird geprüft …
claude-no-status = Status auf dieser Plattform nicht verfügbar
claude-login-title = Führt `claude auth login` in einem Terminal-Tab aus
claude-login-again = Erneut anmelden
claude-login = Anmelden
claude-check-again = Erneut prüfen
claude-hint = Führt die claude-CLI ohne Oberfläche im geöffneten Ordner mit deiner Abo-Anmeldung aus; kein Schlüssel, Moonkale speichert nichts. Ihre Werkzeugaufrufe erscheinen im Verlauf als ▸-Zeilen. Im Web läuft die CLI – und ist angemeldet – auf dem Server.

## Extensions panel

extensions-title = Erweiterungen
extensions-changes-go-to = Änderungen gehen an 
extensions-user-settings = Benutzereinstellungen
extensions-this-folder = diesen Ordner
extensions-catalogue-before = Was jede Erweiterung ist, zu welcher Stufe sie gehört und wo sie läuft: der 
extensions-catalogue = Erweiterungskatalog
extensions-catalogue-after = .
extensions-hint = Optionale Funktionen werden nur geladen, wenn sie eingeschaltet sind. Berechtigungen legen fest, was eine Erweiterung darf; Haken entfernen, um sie einzuschränken. Berechtigungen gehören immer dir (Benutzereinstellungen): Ein Ordner kann sie nicht erteilen.
extensions-core = Kern
extensions-opt-in = zuschaltbar
extensions-wasm = Installiert (wasm)
extensions-wasm-hint = Module von Dritten aus ~/.config/moonkale/extensions und <Ordner>/.moonkale/extensions. Aus, bis sie eingeschaltet werden; keine Berechtigung, bis sie angehakt ist.
extensions-wasm-desc = { $description } · Befehle: { $commands }

## Remote and server dialogs

connect = Verbinden
remote-name-needed = Gib der Verbindung einen Namen, um sie zu speichern
remote-title = Entfernten Ordner öffnen
remote-hint-ssh = Öffnet einen Ordner auf einem anderen Rechner über das ssh deines Systems. Die Anmeldung ist die von ssh – Schlüssel, Agent, Passwörter und Host-Key-Prüfung funktionieren genau wie im Terminal; was ssh fragt, erscheint im Terminal-Panel, und Moonkale sieht nie ein Geheimnis.
remote-hint-host = Host: was du nach ssh tippen würdest – ein Name oder benutzer@host, ein Alias aus ~/.ssh/config (unten angeboten), mit beliebigen ssh-Optionen davor (-p 2222, -i ~/.ssh/key, -J jumphost, -o …); VAR=wert-Wörter am Anfang werden in der Umgebung von ssh gesetzt.
remote-hint-server = Beim ersten Mal pro Host und Version wird Moonkales eigener Server (etwa 150 MB) nach ~/.local/share/moonkale/server/ auf den Host kopiert und dort nur für diese Sitzung gestartet: Der Ordner, sein Index, git, Language-Server und Terminals laufen dann auf diesem Rechner; der Editor und deine API-Schlüssel bleiben hier. Schließt du den Ordner, enden Sitzung und Server.
remote-saved = Gespeicherte Verbindung
remote-pick = – unten einen Host eingeben oder einen wählen –
remote-host = Host (wie nach ssh getippt)
remote-folder = Ordner auf diesem Rechner
remote-save-name = Name, unter dem die Verbindung gespeichert wird
remote-save-title = Host und Ordner unter diesem Namen merken (Benutzereinstellungen)
remote-forget = Vergessen
server-title = Mit Server verbinden
server-hint = Einen laufenden Moonkale-Server (moonkale-server oder dx serve mit MOONKALE_TOKEN) aus dieser App nutzen: Ordner, Index, git, Language-Server, Terminals und Agenten-Sitzungen laufen dort; der Editor und deine API-Schlüssel bleiben hier. Über das Netz sollte der Server HTTPS oder einen Tunnel verwenden – siehe „Remote and Server Modes“.
server-url = Server-URL
server-token = Zugriffstoken (MOONKALE_TOKEN)
server-token-placeholder = leer bei einem Entwicklungsserver ohne Token

## Frame, shell, status bar

status-no-new-window = Ein neues Fenster ist auf dieser Plattform nicht verfügbar
status-no-terminal = Keine Terminal-Erweiterung ist eingeschaltet (Erweiterungen-Panel)
status-could-not-create = { $name } konnte nicht angelegt werden: { $error }
status-could-not-move = Dokument konnte nicht hierher verschoben werden: { $error }
drop-target = Ablegen (oder klicken), um { $name } in dieses Fenster zu verschieben
drop-banner = { $name } wurde aus einem anderen Fenster gezogen.
drop-move-here = Hierher verschieben
dismiss = Schließen
status-open-folder-failed = Ordner öffnen fehlgeschlagen: { $error }
status-nothing-in-tile = Im Bereich { $tile } gibt es nichts anzuzeigen
status-saved =
    { $n ->
        [one] { $n } Dokument
       *[other] { $n } Dokumente
    } gespeichert
status-kept-open =
    { $n ->
        [one] { $n } ungespeichertes Dokument bleibt offen
       *[other] { $n } ungespeicherte Dokumente bleiben offen
    }
status-no-folder = Kein Ordner ist geöffnet
status-about = Moonkale { $version } – graphbasierter Code- und Wissenseditor · mathstruct.github.io/Moonkale
editor = Editor
rail-title = { $label } – erneut klicken zum Ausblenden
people = Personen
people-nobody = Sonst ist niemand hier
people-here = Hier: { $names }
people-nobody-folder = Sonst sieht sich niemand diesen Ordner an
status-no-folder-open = Kein Ordner geöffnet
status-server-client = Diese App ist Client eines Moonkale-Servers
status-remote = Entfernter Ordner über SSH – { $phase }
status-lsp = Language-Server
status-others = { $names } – auch in diesem Ordner
status-window-title = Dieses Fenster: { $id }. Andere Fenster dieser Sitzung werden gezählt, sobald sie antworten.
status-windows =
    { $n ->
        [one] { $n } Fenster
       *[other] { $n } Fenster
    }
more = Mehr
status-ready = Bereit

## Workbench chrome (dioxus-workbench)

wb-tab-group = Panelgruppe
wb-tab-group-labelled = Panelgruppe: { "{" }panel{ "}" }
wb-tab-hint = Ziehen zum Andocken. Alt+Umschalt+Pfeile teilt; Alt+Umschalt+Bild-Tasten verschiebt.
wb-close-tab = { "{" }title{ "}" } schließen
wb-empty-group = Leere Gruppe
wb-empty-title = Leere Panelgruppe
wb-empty-hint = Ziehe einen Panel-Tab hierher oder schließe diese Gruppe.
wb-empty-keys = Alt+Umschalt+Pfeile teilt · Alt+Umschalt+Bild-Tasten verschiebt · Pfeile wechseln Tabs
wb-split-right = Panelgruppe nach rechts teilen
wb-split-right-hint = Rechts teilen (Alt+Umschalt+Rechts)
wb-split-down = Panelgruppe nach unten teilen
wb-split-down-hint = Unten teilen (Alt+Umschalt+Runter)
wb-close-empty = Leere Panelgruppe schließen
wb-close-empty-hint = Leere Gruppe schließen
wb-splitter = Größe der Panelgruppen ändern
wb-splitter-hint = Ziehen ändert die Größe. Pfeiltasten ändern sie; mit Umschalt weiter; Doppelklick setzt zurück.
