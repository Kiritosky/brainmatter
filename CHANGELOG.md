# Was ist neu

Dieser Text erscheint in der App und auf GitHub. Ein Release ohne Eintrag für seine Version schlägt fehl. Die englische Fassung steht in [CHANGELOG.en.md](CHANGELOG.en.md).

## 1.5.0

Brainmatter merkt jetzt, welche Einträge vielleicht nicht mehr stimmen, welche keiner mehr liest und welche noch fehlen. Tokens können ablaufen, und den Kontext eines Projekts kopierst du für Chat-Apps ohne MCP.

### Neu
- **Projekt-Ordner:** Im Projekt-Menü gibst du einem Projekt seinen Ordner auf dem Mac, etwa das Repo. Dann prüft Brainmatter die Dateipfade, die in seinen Einträgen und denen seiner Unterprojekte stehen. Es schaut nur nach, ob es eine Datei noch gibt und wann sie sich geändert hat, und liest sie nie.
- **Vielleicht veraltet:** Fehlt eine genannte Datei oder hat sie sich seit dem Eintrag geändert, sagt der Eintrag es dir und steht im Prüf-Panel unter „Veraltet“. Eine KI, die ihn liest, erfährt, welche Dateien fehlen. „Stimmt noch“ nimmt ihn heraus, bis sich der Eintrag oder eine seiner Dateien wieder ändert.
- **Lesestatistik:** Brainmatter zählt bei dir auf dem Mac, wie oft ein Eintrag gelesen wird, von dir und von deinen KIs. Unten im Eintrag steht, wie oft, und beim Darüberfahren, wann zuletzt und von wem. Unter „Selten“ stehen Einträge, die 90 Tage lang niemand gelesen oder geändert hat. Gezählt wird ab diesem Update, die Liste füllt sich also nach 90 Tagen.
- **Fehlende Einträge:** `[[Links]]` auf Einträge, die es noch nicht gibt, sammelt das Prüf-Panel unter „Fehlend“. Ein Klick legt den Eintrag an, in den Projekten des Eintrags, der auf ihn verweist.
- **Tokens mit Ablauf:** Ein Token für einen eigenen Client gilt auf Wunsch nur einen Tag, 7 oder 30 Tage, etwa für einen Test. Danach weist Brainmatter ihn ab, und in der Liste steht „abgelaufen“.
- **Kontext kopieren:** Im Projekt-Menü legt „Kontext kopieren“ das Wichtigste eines Projekts als Markdown in die Zwischenablage: Beschreibung, Unterprojekte, Doku-Seiten, die wichtigsten Einträge und offene Aufgaben, so lang, dass es in einen Chat passt. So hat auch eine Chat-App ohne MCP dein Projekt vor sich.

## 1.4.1 · 2026-11-13

Das Passwort-Fenster für KIs schließt sich jetzt nach dem Entsperren.

### Behoben
- Nach dem Entsperren blieb das Passwort-Fenster für KIs offen. Jetzt verschwindet es, sobald Brainmatter entsperrt ist, egal wo. Es lässt sich nun auch verschieben und passt seine Höhe dem Inhalt an.

## 1.4.0 · 2026-11-13

Braucht eine KI Brainmatter, bleibt das Hauptfenster zu. Das Passwort gibst du in einem kleinen Fenster ein.

### Neu
- **Passwort für KIs im kleinen Fenster:** Findet eine KI Brainmatter gesperrt, fragt Brainmatter in einem kleinen Fenster nach deinem Master-Passwort und nennt die KI, die zugreifen will. Das passiert, wenn Touch ID nicht eingerichtet ist oder du bei Touch ID „Passwort verwenden“ wählst. Danach arbeitet die KI gleich weiter. „Nicht jetzt“ sagt ihr sofort ab, und sie fragt eine Weile nicht wieder.
- **In der Mitte oder in der Menüleiste:** Unter Einstellungen → Sperren wählst du, wo das Fenster erscheint: in der Bildschirmmitte, oder als Fenster der Menüleiste unter ihrem Symbol.
- **Von einer KI gestartet, nur in der Menüleiste:** Ist Brainmatter beendet und eine KI startet es, bleibt es in der Menüleiste, ohne Fenster und ohne Symbol im Dock.

### Behoben
- Touch ID konnte sich nach dem Sperren melden, obwohl das Hauptfenster geschlossen war. Jetzt fragt es erst, wenn du das Fenster öffnest.
- Eine KI-Verbindung mit einer anderen Brainmatter-Adresse, etwa für einen Entwicklungs-Build, holte das Hauptfenster nach vorn, wenn dort niemand antwortete.

## 1.3.0 · 2026-11-05

Deine Befehle und Skills stehen jetzt auch im Skill-Menü deiner KI-Apps, und die Gedächtnis-Regel bleibt von selbst aktuell.

### Neu
- **Im Skill-Menü:** Für Claude Code, Codex und Cursor legt Brainmatter auf Wunsch seine Befehle, deine geprüften Vorlagen und deine Skills in den Skill-Ordner der App, so weit die Verbindung sie sehen darf: `~/.claude/skills`, `~/.agents/skills` oder `~/.cursor/skills`. So stehen sie auch in Apps zur Wahl, die Skills zeigen, MCP-Befehle aber nicht. Brainmatter hält den Ordner bei jeder Änderung auf Stand und rührt nur an, was es selbst dort abgelegt hat. Neue Verbindungen haben es eingeschaltet, bestehende bekommen einen Schalter unter Einstellungen → KI-Verbindungen.
- **Direkt als Dateien oder über Brainmatter:** Bei jedem Skill legst du fest, wie er in den Skill-Ordner kommt: ganz, mit allen Dateien, oder als kurzer Verweis, der ihn erst beim Laden aus dem Tresor holt. Skills von GitHub gehen als Dateien, eigene über Brainmatter. Im Ordner liegt beides unverschlüsselt, beim Verweis nur Name und Beschreibung.
- **Befehle über `get_template`:** `get_template` liefert auch die eingebauten Befehle ausgefüllt, so wie die Befehlsliste sie zeigt.
- **Die Gedächtnis-Regel bleibt aktuell:** Beim Start und beim Entsperren bekommt eine Regel, die Brainmatter früher geschrieben hat, den aktuellen Wortlaut, an ihrer Stelle und in ihrer Fassung zum Lesen oder Schreiben.

## 1.2.0 · 2026-11-04

Deine KIs greifen jetzt von selbst zu Brainmatter, und ein Skill-Tresor hält die Anleitungen, mit denen sie arbeiten, an einem Ort.

### Neu
- **Brainmatter als Gedächtnis:** Beim Verbinden schreibt Brainmatter auf Wunsch eine kurze Regel in die Anweisungen der KI: `~/.claude/CLAUDE.md` für Claude Code, `~/.codex/AGENTS.md` für Codex, `~/.cursor/rules` für Cursor. Dann schlägt sie vor jeder Aufgabe nach und sichert Entscheidungen, Erkenntnisse und deine Korrekturen von selbst. Für Claude Desktop und jede andere KI mit MCP gibt es die Regel zum Kopieren. Bestehende Verbindungen bekommen dafür einen Schalter unter Einstellungen → KI-Verbindungen.
- **Skill-Tresor:** Skills sind Anleitungen, die eine KI von selbst lädt, wenn eine Aufgabe passt, etwa wie du Commit-Nachrichten haben willst oder wie man ein PDF-Formular ausfüllt. Lege sie pro Projekt oder für alles an, schreibe und bearbeite sie in der Bibliothek, hänge Dateien wie Skripte an, und jede verbundene KI kann sie nutzen, egal welche. Skills, die du geprüft hast, erscheinen auch als Befehle.
- **Skills von GitHub:** Füge die Adresse eines öffentlichen Repos oder Ordners ein, wähle die Skills aus, und sie sind im Tresor. Später bringt ein Klick einen Skill auf den Stand seiner Quelle. Repos im verbreiteten Agent-Skills-Format funktionieren so, wie sie sind, und ein Export schreibt Skills wieder in diesem Format.
- **Installieren mit Homebrew:** `brew install --cask kiritosky/tap/brainmatter`. Die App aktualisiert sich weiterhin selbst.
- **Klarere Hinweise für jede KI:** Der MCP-Server sagt jeder verbundenen KI genauer, wann sie nachschlagen und wann sie speichern soll.

### Verbessert
- Klareres Englisch in der ganzen Oberfläche.

### Behoben
- Die Suche nach Sinn hängt nicht mehr von einer ZIP-Bibliothek mit bekannten Sicherheitslücken ab.
- Fehler beim Laden des Modells für die Suche nach Sinn kamen immer auf Deutsch.
- Ein leerer Eintrag oder eine leere Seite forderte auch auf Englisch auf Deutsch zum Schreiben auf.
- Die Fehlermeldungen im Browser-Demo folgen jetzt der Sprache der Oberfläche.

## 1.1.0 · 2026-11-04

Brainmatter spricht jetzt auch Englisch, holt deine Notizen aus anderen Apps und sichert sich auf Wunsch an einem zweiten Ort.

### Neu
- **Englisch:** Die App folgt der Sprache von macOS. Steht Deutsch in den Systemeinstellungen an erster Stelle, bleibt alles deutsch, sonst ist sie englisch.
- **Markdown importieren:** Ein Ordner aus Obsidian, Logseq oder einem Brainmatter-Export wird zu Einträgen. Ordner werden Projekte, Front Matter wird zu Tags und Art, Links zwischen den Dateien werden Verbindungen. Vorher siehst du, was kommt, und der ganze Import lässt sich mit einem Klick zurücknehmen. Zu finden unter Einstellungen → Backups.
- **Backups an einem zweiten Ort:** Jedes Backup landet auf Wunsch auch auf einem externen Laufwerk oder in einem anderen Ordner. Ist das Laufwerk gerade nicht da, holt Brainmatter die Kopie nach, sobald es wieder angeschlossen ist.
- **Auf einem neuen Mac weitermachen:** Beim Einrichten „Aus einem Backup übernehmen“ wählen, den Ordner zeigen und mit dem Master-Passwort von damals öffnen.
- **Doku folgt ihren Dateien live:** Eingebundene Doku-Ordner aktualisieren sich, sobald du dort speicherst, nicht erst beim Öffnen der Doku.
- **Brainmatter in der Menüleiste:** Ein Klick auf das Symbol zeigt, ob der Tresor offen ist, was die KIs zuletzt geändert haben, und eine Suche. Entsperren geht direkt dort, per Touch ID oder Passwort. Ein Rechtsklick öffnet das Menü wie bisher.
- **Nur noch in der Menüleiste, wenn das Fenster zu ist:** Schließt du das Fenster, verschwindet Brainmatter aus dem Dock und aus ⌘Tab und läuft oben weiter.

### Verbessert
- **Große Tresore:** Entsperren und Suchen sind bei 50.000 Einträgen schneller, und eine Änderung lädt nur noch, was sie betrifft, statt des ganzen Graphen.
- Große Graphen zeigen ihre Einträge zuerst und die Linien einen Moment später.

### Behoben
- Links und Verweise unterschieden bei Umlauten zwischen Groß- und Kleinschreibung, etwa bei „Ärger“ und „ärger“.

## 1.0.0 · 2026-11-02

Schreiben, Vorlagen und der Einstieg sind neu gemacht, der Graph bleibt mit 50.000 Einträgen flüssig, und die App wirkt ruhiger.

### Neu
- **Doku direkt auf der Seite schreiben:** Tippe `/` für Überschriften, Listen, Tabellen und alle anderen Formate, auch wenn du die Markdown-Zeichen nicht kennst. ⌘B und ⌘I formatieren die Auswahl.
- **Einführung:** Beim ersten Start zeigt dir ein kurzer Rundgang die wichtigsten Stellen. In ⌘K lässt er sich jederzeit neu starten.
- **Hilfe in den Einstellungen:** Was die App kann, und welche MCP-Werkzeuge und Befehle eine KI nutzen kann.
- **Vorlagen in drei Schritten:** Eine neue Vorlage entsteht jetzt in einem geführten Ablauf, und aus einer Vorlage wird mit einem Klick ein Eintrag.
- **Suchen beim Zuweisen:** Überall, wo du ein Projekt wählst, kannst du es jetzt suchen und direkt neu anlegen, auch in Quick Capture mit ⌘P.
- **Export:** Alle Einträge und Seiten als Markdown-Dateien, in Ordnern nach Projekt.
- **Gespeicherte Ansichten:** Filter und Einfärbung unter einem Namen sichern und mit einem Klick wieder aufrufen.
- **Schneller im Graph:** Mit ⌥ von einem Eintrag zum anderen ziehen verbindet sie, ein Doppelklick öffnet das Projekt darunter, Esc geht eine Ebene zurück.
- **Für KI-Modelle:** Projekte und Doku-Seiten sind als MCP-Ressourcen lesbar, und ein verbundenes Modell erfährt es, wenn sich seine Vorlagen ändern.

### Verbessert
- **Großer Graph:** Bei 50.000 Einträgen und rund 150.000 Verbindungen laufen Zoomen, Verschieben und Überfahren jetzt flüssig. Vorher stand die App bei jeder dieser Gesten teils mehrere Sekunden.
- **Schnellerer Start:** Der Graph ist nach dem Entsperren bei dieser Größe deutlich schneller zu sehen und braucht danach weniger Nacharbeit.
- **Suche nach Sinn:** Findet deutlich weniger falsche Treffer, wenn es zu deiner Anfrage nichts Passendes gibt.
- **Schnellere Suche:** Besonders kurze Suchwörter, die zu vielen Einträgen passen, sind in großen Tresoren bis zu siebenmal schneller.
- **Ruhigere Optik:** Viele Kleinigkeiten überarbeitet, von Schatten und Icons bis zur Auswahlfarbe.

### Behoben
- Beim Schreiben in der Doku konnte der Cursor springen und gerade Getipptes verloren gehen.
- Eine volle Seitenleiste schob in kleinen Fenstern den Fußbereich und die Graph-Werkzeuge aus dem Bild.
- Einträge rückten bei jedem Öffnen einer Ansicht ein Stück. Ihre Plätze bleiben jetzt erhalten.
- Quick Capture zeigte nach dem Entsperren keine Projekte.
- Ist das Tastenkürzel für Quick Capture von einer anderen App belegt, sagen die Einstellungen das jetzt.

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
