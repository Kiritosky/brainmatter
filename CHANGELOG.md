# Was ist neu

Dieser Text erscheint in der App und auf GitHub. Ein Release ohne Eintrag für seine Version schlägt fehl.

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

