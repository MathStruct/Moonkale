lsp-starting = { $language }: Language-Server startet …
lsp-ready = { $server }: bereit
lsp-exited = { $language }: Language-Server beendet
lsp-init-failed = { $language }: Initialisierung fehlgeschlagen: { $error }
editor-rename-needs-lsp = Umbenennen braucht einen Language-Server für diese Datei
editor-definition-outside = Die Definition liegt außerhalb des Ordners: { $uri }
editor-no-definition = Keine Definition gefunden
editor-applied =
    Änderungen auf { $n ->
        [one] { $n } Datei
       *[other] { $n } Dateien
    } angewendet
editor-edit-failed = Bearbeitung fehlgeschlagen: { $error }
editor-nothing-to-rename = Hier gibt es nichts umzubenennen
editor-plain-text = Klartext
editor-unsaved = Ungespeicherte Änderungen
editor-wrap-title = Lange Zeilen umbrechen
editor-wrap = Umbruch
editor-save = Speichern
editor-reload-title = Änderungen verwerfen und aus der Quelle neu laden
editor-reload = Neu laden
editor-to-rust = Diese Datei im Rust-Editor anzeigen
editor-rename-prompt = „{ $word }“ umbenennen in:
editor-rename = Umbenennen
editor-cancel = Abbrechen
editor-no-actions = Hier gibt es keine Code-Aktionen.
editor-actions = Code-Aktionen:
editor-references =
    { $n ->
        [one] { $n } Verweis
       *[other] { $n } Verweise
    }
editor-conflict = Die Datei wurde außerhalb des Editors geändert. Neu laden zeigt den neuen Inhalt (deine Änderungen gehen verloren), erneut speichern überschreibt ihn.
editor-save-failed = Speichern fehlgeschlagen: { $error }
editor-failed = Der Editor konnte nicht starten.
editor-failed-hint =  Das ist ein Fehler – bitte mit der Meldung unten melden (die Datei selbst ist in Ordnung; „Neu laden“ versucht es erneut).
editor-loading = Editor wird geladen …

editor-to-codemirror = Diese Datei in CodeMirror anzeigen
editor-document-closed = Dieses Dokument ist nicht mehr geöffnet.
editor-rust-meta = { $language } · v{ $version } · Rust-Editor
editor-rust-label = Code-Editor
editor-find = Suchen
editor-match-case = Groß-/Kleinschreibung
editor-whole-word = Ganzes Wort
editor-find-previous = Zurück
editor-find-next = Weiter
editor-replace = Ersetzen
editor-replace-all = Alle ersetzen
editor-toggle-comment = Kommentar umschalten
editor-fold = Einklappen
editor-unfold = Ausklappen
editor-fold-all = Alles einklappen
editor-unfold-all = Alles ausklappen

editor-search-regex = Regulärer Ausdruck
editor-search-count = { $current } von { $total }
editor-search-captures-help = $1 oder $name für Gruppen verwenden; $$ fügt ein Dollarzeichen ein.
editor-search-invalid = Ungültiger regulärer Ausdruck: { $error }

editor-diagnostics = Diagnosen
editor-hover = Hover-Information anzeigen
editor-completion = Vervollständigen
editor-definition = Zur Definition
editor-definition-needs-lsp = Zur Definition benötigt einen Sprachserver für diese Datei
editor-definition-failed = Zur Definition fehlgeschlagen: { $error }
editor-definition-timeout = Zeitüberschreitung beim Laden der Definition

editor-rename-shortcut = Symbol umbenennen
editor-rename-timeout = Zeitüberschreitung bei der Umbenennung

editor-actions-shortcut = Code-Aktionen
editor-references-shortcut = Referenzen suchen
editor-tools-needs-lsp = Diese Aktion benötigt einen Sprachserver für die Datei
editor-tools-timeout = Zeitüberschreitung bei der Sprachserver-Anfrage
editor-tools-loading = Wird geladen…
editor-reference-outside = Diese Referenz liegt außerhalb des Ordners

editor-preview = Vorschau
editor-preview-title = Markdown-Formatierung anzeigen; aktiviert den Zeilenumbruch während der Vorschau.
editor-out-of-sync = Editor und Dokument stimmen nicht überein und ließen sich nicht wieder abgleichen; die Bearbeitung wurde hier angehalten, um die Datei zu schützen. Schließen und öffnen Sie sie erneut; die gespeicherte Datei ist unverändert.

lsp-restart-unavailable = Zum Neustart wird ein Sprachserver für ein aktives Ordnerdokument benötigt

editor-rename-stale = Dokument geändert; Umbenennen erneut öffnen

editor-link-failed = Link konnte nicht geöffnet werden: { $error }
editor-link-heading-missing = Überschrift nicht gefunden: { $heading }

editor-image-loading = Bild wird geladen…
editor-image-missing = Bild nicht gefunden
editor-image-too-big = Bild überschreitet die Grenze von 24 MiB
editor-image-pixels = Bildabmessungen überschreiten die Pixelgrenze
editor-image-failed = Bild kann nicht angezeigt werden
editor-math-failed = Diese Formel kann nicht dargestellt werden. Zum Bearbeiten des Quelltexts klicken.
editor-math-config = Ungültige oder zu große .moonkale/katex.json ignoriert; Standardmakros werden verwendet.

editor-wiki-preview = Vorschau des Wiki-Links
editor-wiki-preview-loading = Notiz wird geladen…
editor-wiki-preview-missing = Notiz nicht gefunden
editor-wiki-preview-unavailable = Vorschau nicht verfügbar
editor-wiki-preview-too-large = Notiz überschreitet das Vorschaulimit von 256 KiB

editor-properties = Eigenschaften
editor-properties-none = keine
editor-properties-yaml = Metadaten (YAML)
editor-properties-apply = Übernehmen
editor-properties-cancel = Abbrechen
editor-properties-stale = Dieser Entwurf ist veraltet. Abbrechen und Eigenschaften erneut öffnen, um das aktuelle Dokument zu bearbeiten.
editor-properties-invalid = YAML-Eigenschaften ohne Metadaten-Begrenzungszeilen eingeben.

editor-doc-preview = Dokuvorschau
editor-doc-preview-title = Dokumentation als Markdown darstellen; die Vorschau zeigt beim Bearbeiten ihren Quelltext.

editor-a11y-help = Mit den Pfeiltasten den Quelltext lesen und den Cursor bewegen. { $primary }+M wechselt zwischen Fokuswechsel und Einrücken mit Tab. { $primary }+Enter öffnet den Link oder das Bild am Cursor. Escape schließt Werkzeuge.
editor-a11y-position = Zeile { $line }, Spalte { $column }.
editor-a11y-selection = Auswahl von Zeile { $line }, Spalte { $column } bis Zeile { $end_line }, Spalte { $end_column }.
editor-a11y-tab-focus = Tab wechselt den Fokus.
editor-a11y-tab-indent = Tab rückt ein.

editor-group-history-blocked = Diese Umbenennung kann nicht rückgängig gemacht oder wiederholt werden. Zuerst neuere Änderungen in { $file } rückgängig machen; möglicherweise ist der gemeinsame Verlauf nicht mehr verfügbar.


native-reload = Rust-Editor: Neu laden
native-hover = Rust-Editor: Hover-Informationen anzeigen
native-complete = Rust-Editor: Vervollständigen
native-preview = Rust-Editor: Markdown-Vorschau
native-doc-preview = Rust-Editor: Dokumentationsvorschau
native-codemirror = Rust-Editor: Zu CodeMirror wechseln
native-indent-style = Einrückung
native-indent-width = Breite
native-default = Sprachstandard
native-spaces = Leerzeichen
native-tabs = Tabulatoren

# CodeMirror search panel
cm-search-find = Suchen
cm-search-replace = Ersetzen
cm-search-next = weiter
cm-search-previous = zurück
cm-search-all = alle
cm-search-case = Groß-/Kleinschreibung
cm-search-regexp = Regulärer Ausdruck
cm-search-word = Ganzes Wort
cm-search-replace-one = ersetzen
cm-search-replace-all = alle ersetzen
cm-search-close = schließen
cm-search-goto = Gehe zu Zeile
cm-search-go = Los
cm-search-current = Aktueller Treffer
cm-search-replaced = $ Treffer ersetzt
cm-search-replaced-line = Treffer in Zeile $ ersetzt
cm-search-on-line = in Zeile
