# Was ist neu

Dieser Text erscheint in der App und auf GitHub. Ein Release ohne Eintrag für seine Version schlägt fehl.

## 0.4.0 · 2026-11-01

Projekte lassen sich jetzt richtig verwalten, und die App sagt dir, was sich geändert hat.

### Neu
- **Projekte umbenennen:** Doppelklick auf den Namen in der Seitenleiste, oder über das Menü `…` neben dem Projekt.
- **Projekte verschieben:** Ziehe ein Projekt auf ein anderes, um es dort unterzuordnen. An dessen oberen oder unteren Rand gezogen landet es daneben, auf die Überschrift „Projekte“ gezogen auf der obersten Ebene. Unterprojekte wandern mit. Ohne Ziehen geht es über `…` → „Verschieben nach“.
- **Projekte löschen:** `…` → „Löschen“ zeigt vorher, was betroffen ist. Einträge bleiben unter „Alles“, wenn du nichts anderes wählst. Rückgängig machen kannst du es in der Aktivität.
- **Was ist neu:** Nach einem Update erscheint dieses Fenster einmal von selbst. Vor dem Installieren siehst du, was kommt, und alle bisherigen Änderungen findest du unter Einstellungen → Allgemein → „Änderungen“.

### Für KIs
- Neue Tools `update_project` und `delete_project`. Eine KI, die auf ein Projekt beschränkt ist, kann genau dieses nicht verschieben oder löschen.

### Gut zu wissen
- Doku-Seiten und Vorlagen, die nur zu einem gelöschten Projekt gehören, gehen immer mit in den Papierkorb. So wird aus einer Projekt-Vorlage nicht plötzlich eine für alle.

## 0.3.0 · 2026-11-01

Jedes Projekt bekommt sein Handbuch, und Einträge können auf Dateien auf deinem Mac zeigen.

### Neu
- **Projekt-Dokumentation:** Öffne ein Projekt und schalte oben rechts auf „Doku“. Dort schreibst du Seiten und Unterseiten, liest sie mit Inhaltsverzeichnis, blätterst vor und zurück und durchsuchst sie.
- **Doku aus Repos einbinden:** Unter „Aus Dateien“ bindest du einen Ordner wie `docs` oder eine `README.md` ein. Die Seiten bleiben schreibgeschützt und werden beim Öffnen der Doku aktualisiert. Ist ein Laufwerk gerade nicht da, bleiben sie erhalten.
- **Datei-Verknüpfungen:** Im Eintrag unter „Verknüpfte Dateien“ verweist du auf Dateien und Ordner, ohne sie zu kopieren. Der Link stimmt auch, nachdem du die Datei verschoben hast.

### Für KIs
- `list_docs`, `read_doc` und `write_doc`: Eine KI sieht das Inhaltsverzeichnis und liest nur den Abschnitt, den sie braucht. Das spart Tokens.
- `link_file` und `read_linked_file`: Einen Link, den eine KI selbst anlegt, darf sie erst lesen, nachdem du ihn im Eintrag freigegeben hast. Pro Link kannst du den Zugriff auch wieder sperren.

### Behoben
- „Zuletzt geändert“ war bei Einträgen aus derselben Millisekunde zufällig sortiert.
- Die Zahl neben einem Projekt zählte Doku-Seiten und Vorlagen mit. Jetzt zählt sie nur noch Einträge.

## 0.2.0 · 2026-10-31

Die erste veröffentlichte Version.

### Wissensgraph
- Einträge in Markdown, verbunden über `[[Wikilinks]]` oder benannte Beziehungen wie „hängt ab von“.
- Projekte in beliebiger Tiefe, einklappbar in der Seitenleiste. Ein Eintrag kann in mehreren Projekten sein.
- Einfärben nach Projekt, Art oder Thema. Der Zeitregler spielt ab, wie dein Graph gewachsen ist.
- Quick Capture: Mit ⌥⇧Space hältst du von überall etwas fest.

### Suche nach Sinn
- Wortsuche auf Deutsch und Englisch, ohne weitere Einrichtung.
- Optional ein lokales Modell (etwa 490 MB, Einstellungen → „Suche nach Sinn“): Es findet Einträge auch ohne gemeinsames Wort, erkennt Duplikate und schlägt Verbindungen vor.

### KI-Zugriff
- Claude Code, Claude Desktop, Cursor und Codex lassen sich unter Einstellungen → „KI-Verbindungen“ per Klick einrichten.
- Jede KI hat eigene Rechte und auf Wunsch nur Zugriff auf bestimmte Projekte. Was sie schreibt, bleibt als „ungeprüft“ markiert, bis du es gesehen hast, und jede Änderung lässt sich rückgängig machen.
- Vorlagen und Prompts: Geprüfte Vorlagen aus der Bibliothek stehen KIs auch als Prompt bereit.

### Sicherheit und Daten
- Alles liegt verschlüsselt auf deinem Mac, auch Suchindex, Anhänge und Backups. Entsperren geht mit Passwort oder Touch ID.
- Verschlüsselte Anhänge mit Vorschau, tägliche Backups mit Wiederherstellen und ein Papierkorb.
- Automatische Updates, signiert und abschaltbar.
