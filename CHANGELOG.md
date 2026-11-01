# Was ist neu

Dieser Text erscheint in der App und auf GitHub. Ein Release ohne Eintrag für seine Version schlägt fehl.

## 0.7.0 · 2026-11-01

Du kannst Modellen jetzt Befehle für Brainmatter geben, und die App lässt sich stärker an dich anpassen.

### Neu
- **Befehle für KI-Modelle:** `setup-project` baut Struktur und Doku für ein Projekt auf, `update-docs` bringt die Doku auf den neuesten Stand, `capture-session` sichert das Wissen einer Sitzung, `recall` antwortet aus Brainmatter und `tidy-project` räumt ein Projekt auf. In Claude Code zum Beispiel als `/mcp__brainmatter__setup-project`.
- **Gemeinsames Gedächtnis:** Modelle werden jetzt angewiesen, Entscheidungen, Erkenntnisse und deine Korrekturen von sich aus zu speichern, damit das nächste Modell darauf aufbauen kann. Was zu keinem Projekt passt, kommt in das Projekt `AI Memory`.
- **Neues Verhalten beim Überfahren:** Ein Eintrag unter der Maus wird größer, und die Einträge daneben rücken zur Seite. Der große weiße Rand ist weg.
- **Einstellungen für den Graph:** Hintergründe und Namen der Projekte, Ausweichen und Abdunkeln beim Überfahren, Größe der Einträge, Stärke der Linien und Menge der Beschriftungen.
- **Mehr unter Allgemein:** Akzentfarbe, weniger Bewegung, KI-Aktivität ein- und ausblenden, Neuerungen nach Updates abschalten.

## 0.6.0 · 2026-11-01

Im Graph sind deine Projekte jetzt klar getrennt. Linien zwischen ihnen gibt es nur dort, wo Einträge wirklich verlinkt sind.

### Neu
- **Projekte als Gruppen:** Jedes Projekt bekommt im Graph einen eigenen Kreis mit seinem Namen, Unterprojekte liegen darin. Ein Klick auf den Namen öffnet das Projekt.
- **Verbindungen über Projektgrenzen:** Links zwischen zwei Projekten bleiben als Linien sichtbar und ziehen die Kreise nicht ineinander. Verlinkte Projekte liegen näher beieinander.
- **Ausblenden ohne Lücke:** Blendest du Projekte aus, rückt die Ansicht auf das, was übrig bleibt.

### Geändert
- Beim ersten Öffnen ordnet Brainmatter jede Ansicht einmal neu an. Von Hand verschobene Positionen gehen dabei einmalig verloren.
- Auch nach dem Verschieben eines Projekts wird die Ansicht neu angeordnet.
- Sehr große Graphen brauchen beim ersten Anordnen etwas Zeit: Bei 50.000 Einträgen rund eine Minute. Die App bleibt dabei bedienbar.

## 0.5.1 · 2026-11-01

Du siehst jetzt live, wenn eine KI etwas in Brainmatter schreibt.

### Neu
- **KI-Aktivität live:** Über dem Graph erscheint kurz, welche KI gerade was geändert hat, und die betroffenen Einträge pulsieren. Das gilt nur fürs Schreiben: Lesen hinterlässt keine Spur.

### Behoben
- Die Änderungsliste in der App zeigt wieder alle Versionen. In 0.5.0 fehlten dort ältere Einträge.

## 0.5.0 · 2026-11-01

Du behältst den Überblick, auch wenn der Graph wächst: Projekte lassen sich ausblenden, der Graph lässt sich filtern, und KIs brauchen spürbar weniger Tokens.

### Neu
- **Projekte ausblenden:** Fahre über ein Projekt und klicke das Auge, oder wähle `…` → „Nur dieses anzeigen“. Ausgeblendete Projekte verschwinden aus der Seitenleiste und aus dem Graph. Ein Eintrag, der auch in einem sichtbaren Projekt liegt, bleibt zu sehen. Die Suche findet weiter alles, und KIs sind davon nicht betroffen.
- **Filter im Graph:** Filtere nach Projekt, Art, Tag und danach, wer etwas geschrieben hat.
- **Lokaler Graph:** Zeigt nur, was bis zu drei Verbindungen um die Auswahl liegt.
- **Favoriten:** Der Stern im Eintrag legt ihn oben in die Seitenleiste.
- **Tags in der Seitenleiste:** Ein Klick filtert den Graph.
- **Tags für die Doku:** Jede Seite kann Tags tragen, auch aus Dateien gespiegelte. Die Doku lässt sich danach filtern.
- **⌘K findet mehr:** Auch Projekte und Aktionen wie „Was ist neu“.

### Für KIs
- **Weniger Tokens:** Tool-Beschreibungen und Anweisungen kosten pro Sitzung etwa ein Drittel weniger, rund 3.200 statt 4.800 Tokens. Ein Test achtet darauf, dass das so bleibt.
- `write_doc` nimmt Tags an, `list_docs` zeigt sie und filtert mit `tag`.
- `recent_changes` liefert ohne Angabe 10 statt 15 Änderungen.

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
