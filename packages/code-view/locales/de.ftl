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
editor-to-rust = Diese Datei im Rust-Editor anzeigen (dioxus-code-editor)
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
