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
editor-wrap-title = Lange Zeilen umbrechen (Alt+Z)
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
editor-indent-style = Einrückung
editor-indent-width = Breite
editor-language-default = Sprachstandard
editor-indent-spaces = Leerzeichen
editor-indent-tabs = Tabulatoren

editor-search-regex = Regulärer Ausdruck
editor-search-count = { $current } von { $total }
editor-search-captures-help = $1 oder $name für Gruppen verwenden; $$ fügt ein Dollarzeichen ein.
editor-search-invalid = Ungültiger regulärer Ausdruck: { $error }

editor-diagnostics = Diagnosen
editor-hover = Hover-Information anzeigen
editor-completion = Vervollständigen (Strg+Leertaste)
editor-definition = Zur Definition (F12)
editor-definition-needs-lsp = Zur Definition benötigt einen Sprachserver für diese Datei
editor-definition-failed = Zur Definition fehlgeschlagen: { $error }
editor-definition-timeout = Zeitüberschreitung beim Laden der Definition

editor-rename-shortcut = Symbol umbenennen (F2)
editor-rename-timeout = Zeitüberschreitung bei der Umbenennung

editor-actions-shortcut = Code-Aktionen (Mod-.)
editor-references-shortcut = Referenzen suchen (Umschalt-F12)
editor-tools-needs-lsp = Diese Aktion benötigt einen Sprachserver für die Datei
editor-tools-timeout = Zeitüberschreitung bei der Sprachserver-Anfrage
editor-tools-loading = Wird geladen…
editor-reference-outside = Diese Referenz liegt außerhalb des Ordners
