// What the app can do, for the tour and the help page in the settings.

export const TOUR_SEEN_KEY = 'bm.tourSeen';

export interface TourStep {
  /** Value of the `data-tour` attribute to point at. Without it, the card is centred. */
  target?: string;
  title: string;
  text: string;
  /** Shortcut worth remembering from this step. */
  keys?: string;
}

export const TOUR: TourStep[] = [
  {
    title: 'Willkommen bei Brainmatter',
    text: 'Dein Wissen als Graph, verschlüsselt und nur auf diesem Mac. Hier sind in zehn Schritten die Dinge, die du wissen solltest.',
  },
  { target: 'search', title: 'Suchen und alles andere', text: 'Findet Einträge, Projekte und Befehle. Wenn du etwas nicht findest, fang hier an.', keys: '⌘K' },
  { target: 'new', title: 'Einen Eintrag anlegen', text: 'Ein Gedanke, ein Eintrag. Schreibst du [[Titel]] in den Text, ist er mit diesem Eintrag verbunden.', keys: '⌘N' },
  { target: 'projects', title: 'Projekte', text: 'Ordnen deine Einträge, auch ineinander verschachtelt. Ziehen verschiebt sie, das Auge blendet sie im Graph aus.' },
  { target: 'graph', title: 'Der Graph', text: 'Klick öffnet einen Eintrag. Mit gedrückter ⌥-Taste von einem Eintrag zum anderen ziehen verbindet sie. Doppelklick auf einen Kreis öffnet das Projekt, Esc geht wieder hoch.' },
  { target: 'tools', title: 'Ansicht anpassen', text: 'Einfärben, filtern und durch die Zeit gehen. Unter Filter speicherst du auch eine Ansicht, die du öfter brauchst.' },
  { target: 'library', title: 'Vorlagen', text: 'Texte, die du immer wieder brauchst, mit Platzhaltern. Auch KIs können sie als Befehl nutzen.' },
  { title: 'Doku für jedes Projekt', text: 'Öffne ein Projekt und wechsle oben rechts zu Doku. Beim Schreiben zeigt / alle Formate, du musst kein Markdown können.' },
  { title: 'Von überall festhalten', text: 'Ein kleines Fenster, egal in welcher App du gerade bist. Die erste Zeile wird zum Titel.', keys: '⌥⇧Space' },
  { target: 'settings', title: 'KI verbinden', text: 'Unter KI-Verbindungen richtest du Claude, Cursor und andere ein. Sie lesen und schreiben dann in Brainmatter, nur mit deiner Erlaubnis.' },
  { target: 'review', title: 'Was KIs geschrieben haben', text: 'Landet hier zum Prüfen. Alles lässt sich rückgängig machen, und in der Aktivität siehst du, wer was getan hat.' },
  { target: 'lock', title: 'Sperren', text: 'Sperrt sofort und passiert auch von selbst. Backups und den Export als Markdown findest du in den Einstellungen.', keys: '⌘⇧L' },
];

export const APP_FEATURES: { title: string; items: { label: string; keys?: string }[] }[] = [
  {
    title: 'Überall',
    items: [
      { label: 'Suchen und Befehle', keys: '⌘K' },
      { label: 'Neuer Eintrag', keys: '⌘N' },
      { label: 'Etwas festhalten, aus jeder App', keys: '⌥⇧Space' },
      { label: 'Einstellungen', keys: '⌘,' },
      { label: 'Sperren', keys: '⌘⇧L' },
    ],
  },
  {
    title: 'Im Graph',
    items: [
      { label: 'Eintrag öffnen', keys: 'Klick' },
      { label: 'Zwei Einträge verbinden', keys: '⌥ + Ziehen' },
      { label: 'Projekt öffnen', keys: 'Doppelklick' },
      { label: 'Eine Ebene hoch', keys: 'Esc' },
      { label: 'Ausgewählten Eintrag verbinden', keys: '⌘L' },
    ],
  },
  {
    title: 'Beim Schreiben',
    items: [
      { label: 'Alle Formate', keys: '/' },
      { label: 'Anderen Eintrag verlinken', keys: '[[' },
      { label: 'Fett und kursiv', keys: '⌘B ⌘I' },
      { label: 'Sofort speichern', keys: '⌘S' },
    ],
  },
];

/** Every MCP tool the server offers. A test in crates/bm-mcp keeps this complete. */
export const MCP_TOOLS: { title: string; tools: { name: string; about: string }[] }[] = [
  {
    title: 'Lesen',
    tools: [
      { name: 'search', about: 'Einträge nach Wörtern und Bedeutung finden' },
      { name: 'get', about: 'Einen Eintrag vollständig lesen' },
      { name: 'explore', about: 'Was um einen Eintrag herum liegt' },
      { name: 'find_path', about: 'Kürzester Weg zwischen zwei Einträgen' },
      { name: 'similar', about: 'Ähnliche Einträge und mögliche Duplikate' },
      { name: 'recent_changes', about: 'Was sich zuletzt geändert hat und durch wen' },
      { name: 'unlinked', about: 'Einträge ohne Verbindung' },
      { name: 'unreviewed', about: 'Von KIs Geschriebenes, das du noch nicht gesehen hast' },
    ],
  },
  {
    title: 'Schreiben',
    tools: [
      { name: 'capture', about: 'Einträge und Verbindungen in einem Schritt anlegen' },
      { name: 'update', about: 'Einen Eintrag ändern oder ergänzen' },
      { name: 'link', about: 'Zwei Einträge verbinden' },
      { name: 'unlink', about: 'Eine Verbindung lösen' },
      { name: 'merge', about: 'Duplikate zusammenführen' },
      { name: 'attach', about: 'Längeren Text als Datei anhängen' },
      { name: 'read_attachment', about: 'Einen Anhang lesen' },
    ],
  },
  {
    title: 'Projekte',
    tools: [
      { name: 'list_projects', about: 'Alle Projekte als Baum' },
      { name: 'project_overview', about: 'Zusammenfassung eines Projekts' },
      { name: 'create_project', about: 'Projekt oder Unterprojekt anlegen' },
      { name: 'update_project', about: 'Umbenennen, beschreiben, verschieben' },
      { name: 'delete_project', about: 'Projekt löschen, die Einträge bleiben' },
      { name: 'add_to_project', about: 'Einträge einem Projekt zuordnen' },
      { name: 'remove_from_project', about: 'Einträge aus einem Projekt nehmen' },
    ],
  },
  {
    title: 'Doku, Vorlagen und Dateien',
    tools: [
      { name: 'list_docs', about: 'Doku-Seiten eines Projekts' },
      { name: 'read_doc', about: 'Eine Seite oder einen Abschnitt lesen' },
      { name: 'write_doc', about: 'Eine Seite anlegen oder ersetzen' },
      { name: 'list_templates', about: 'Deine Vorlagen' },
      { name: 'get_template', about: 'Eine Vorlage ausgefüllt holen' },
      { name: 'save_template', about: 'Eine Vorlage anlegen oder ersetzen' },
      { name: 'link_file', about: 'Eine Datei auf dem Mac verknüpfen, du gibst sie frei' },
      { name: 'read_linked_file', about: 'Eine freigegebene Datei lesen' },
    ],
  },
  {
    title: 'Rückgängig',
    tools: [
      { name: 'trash', about: 'Papierkorb ansehen' },
      { name: 'restore', about: 'Aus dem Papierkorb holen' },
      { name: 'delete', about: 'In den Papierkorb legen' },
      { name: 'revert', about: 'Eine Änderung rückgängig machen' },
    ],
  },
];

/** The commands every connected model gets. See crates/bm-mcp/src/prompts.rs. */
export const MCP_COMMANDS: { name: string; about: string }[] = [
  { name: 'setup-project', about: 'Baut für ein Projekt Struktur und Doku auf' },
  { name: 'update-docs', about: 'Bringt die Doku eines Projekts auf den aktuellen Stand' },
  { name: 'capture-session', about: 'Hält fest, was in dieser Sitzung wichtig war' },
  { name: 'recall', about: 'Holt zusammen, was zu einem Thema bekannt ist' },
  { name: 'tidy-project', about: 'Räumt ein Projekt auf: Duplikate, Lücken, Veraltetes' },
];
