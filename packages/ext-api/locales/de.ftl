
## Workspace status messages

restored = Als ungespeicherte Änderung wiederhergestellt – speichern, um sie zu behalten
remote-unavailable = Entfernte Ordner sind auf dieser Plattform nicht verfügbar
remote-need-host = Entfernt: Host und Pfad werden benötigt
remote-error = Entfernt: { $error }
remote-connecting = Entfernt: Verbindung zu { $host } …
remote-prompt = ssh { $host }: { $line } – im Terminal antworten
remote-uploading = Entfernt: { $host } hat noch keinen Moonkale-Server – er wird hochgeladen (einmal pro Version) …
remote-starting = Entfernt: Server auf { $host } wird gestartet …
remote-connected = Entfernt: mit { $host } verbunden, { $path } wird geöffnet …
remote-open-failed = Entfernt: { $host } ist verbunden, aber { $path } ließ sich nicht öffnen: { $error }
remote-disconnected = Entfernt: Verbindung zu { $host } getrennt
server-unavailable = Die Verbindung zu einem Server ist auf dieser Plattform nicht verfügbar
server-need-url = Server: Eine URL wird benötigt
server-error = Server: { $error }
server-connected = Mit { $url } verbunden; sein Ordner wird geöffnet …
server-open-failed = Server { $url }: verbunden, aber sein Ordner ließ sich nicht öffnen: { $error }
server-disconnected = Verbindung zu { $url } getrennt
close-dirty =
    { $name }: { $n ->
        [one] { $n } ungespeichertes Dokument
       *[other] { $n } ungespeicherte Dokumente
    } vor dem Schließen speichern oder neu laden
closed = { $name } geschlossen
no-folder-dialog = Kein Ordnerdialog auf dieser Plattform – Pfad unter Quellen eingeben
opened = { $name } geöffnet
refreshed = { $name } aktualisiert
extensions-not-scanned = Erweiterungen nicht eingelesen: { $error }
settings-not-loaded = Einstellungen nicht geladen: { $error }
reopen-failed = Der letzte Ordner konnte nicht wieder geöffnet werden: { $error }
settings-not-saved = Einstellungen nicht gespeichert: { $error }
workspace-settings-ignored = Arbeitsbereich-Einstellungen ignoriert: { $error }
workspace-settings-not-saved = Arbeitsbereich-Einstellungen nicht gespeichert: { $error }
not-found-in-folders = { $path }: in den geöffneten Ordnern nicht gefunden
saved = { $name } gespeichert
save-failed = Speichern fehlgeschlagen: { $error }
created = { $name } angelegt
renamed = { $from } → { $to } umbenannt
deleted = { $name } gelöscht (bleibt in .moonkale/trash)
reloaded = Von der Festplatte neu geladen
program-unavailable = Programme im Terminal auszuführen ist auf dieser Plattform nicht verfügbar
program-failed = { $program } konnte nicht gestartet werden: { $error }
wiki-unresolved = [[{ $target }]] verweist auf keine Seite
wiki-renamed =
    { $from } → { $to } umbenannt: Links in { $n ->
        [one] { $n } Datei
       *[other] { $n } Dateien
    } angepasst
moved-to-window = In Fenster { $window } verschoben
dragging = { $name } wird gezogen – auf ein anderes Moonkale-Fenster ziehen, um es dorthin zu verschieben

move-unsaved = Ungespeichertes Dokument bleibt hier — vor dem Verschieben in ein anderes Fenster speichern.
