// What the app can do, for the tour and the help page in the settings.
import { t } from '#lib/i18n.ts';

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
    title: t('Willkommen bei Brainmatter', 'Welcome to Brainmatter'),
    text: t(
      'Dein Wissen als Graph, verschlüsselt und nur auf diesem Mac. Hier sind in zehn Schritten die Dinge, die du wissen solltest.',
      'Your knowledge as a graph, encrypted and only on this Mac. Here are the things worth knowing, in ten steps.',
    ),
  },
  {
    target: 'search',
    title: t('Suchen und alles andere', 'Search, and everything else'),
    text: t(
      'Findet Einträge, Projekte und Befehle. Wenn du etwas nicht findest, fang hier an.',
      'Finds entries, projects and commands. If you cannot find something, start here.',
    ),
    keys: '⌘K',
  },
  {
    target: 'new',
    title: t('Einen Eintrag anlegen', 'Creating an entry'),
    text: t(
      'Ein Gedanke, ein Eintrag. Schreibst du [[Titel]] in den Text, ist er mit diesem Eintrag verbunden.',
      'One thought, one entry. Write [[Title]] in the text, and it is linked to that entry.',
    ),
    keys: '⌘N',
  },
  {
    target: 'projects',
    title: t('Projekte', 'Projects'),
    text: t(
      'Ordnen deine Einträge, auch ineinander verschachtelt. Ziehen verschiebt sie, das Auge blendet sie im Graph aus.',
      'They sort your entries, one inside another if you like. Drag to move them; the eye hides them in the graph.',
    ),
  },
  {
    target: 'graph',
    title: t('Der Graph', 'The graph'),
    text: t(
      'Klick öffnet einen Eintrag. Mit gedrückter ⌥-Taste von einem Eintrag zum anderen ziehen verbindet sie. Doppelklick auf einen Kreis öffnet das Projekt, Esc geht wieder hoch.',
      'A click opens an entry. Drag from one entry to another while holding ⌥ to link them. Double-click a circle to open its project; Esc goes back up.',
    ),
  },
  {
    target: 'tools',
    title: t('Ansicht anpassen', 'Shaping the view'),
    text: t(
      'Einfärben, filtern und durch die Zeit gehen. Unter Filter speicherst du auch eine Ansicht, die du öfter brauchst.',
      'Colour, filter and travel through time. Under Filter you can also save a view you need often.',
    ),
  },
  {
    target: 'library',
    title: t('Vorlagen', 'Templates'),
    text: t(
      'Texte, die du immer wieder brauchst, mit Platzhaltern. Auch KIs können sie als Befehl nutzen.',
      'Texts you need again and again, with placeholders. AI clients can use them as commands too.',
    ),
  },
  {
    title: t('Doku für jedes Projekt', 'Documentation for every project'),
    text: t(
      'Öffne ein Projekt und wechsle oben rechts zu Doku. Beim Schreiben zeigt / alle Formate, du musst kein Markdown können.',
      'Open a project and switch to Docs at the top right. While writing, / shows every format; no Markdown needed.',
    ),
  },
  {
    title: t('Von überall festhalten', 'Capture from anywhere'),
    text: t(
      'Ein kleines Fenster, egal in welcher App du gerade bist. Die erste Zeile wird zum Titel.',
      'A small window, whatever app you are in. The first line becomes the title.',
    ),
    keys: '⌥⇧Space',
  },
  {
    target: 'settings',
    title: t('KI verbinden', 'Connecting AI'),
    text: t(
      'Unter KI-Verbindungen richtest du Claude, Cursor und andere ein. Sie lesen und schreiben dann in Brainmatter, nur mit deiner Erlaubnis.',
      'Under AI connections you set up Claude, Cursor and others. They then read and write in Brainmatter, only with your permission.',
    ),
  },
  {
    target: 'review',
    title: t('Was KIs geschrieben haben', 'What AI clients wrote'),
    text: t(
      'Landet hier zum Prüfen. Alles lässt sich rückgängig machen, und in der Aktivität siehst du, wer was getan hat.',
      'Lands here for you to review. Everything can be undone, and the activity shows who did what.',
    ),
  },
  {
    target: 'lock',
    title: t('Sperren', 'Locking'),
    text: t(
      'Sperrt sofort und passiert auch von selbst. Backups und den Export als Markdown findest du in den Einstellungen.',
      'Locks at once, and also by itself. Backups and the Markdown export are in the settings.',
    ),
    keys: '⌘⇧L',
  },
];

export const APP_FEATURES: { title: string; items: { label: string; keys?: string }[] }[] = [
  {
    title: t('Überall', 'Everywhere'),
    items: [
      { label: t('Suchen und Befehle', 'Search and commands'), keys: '⌘K' },
      { label: t('Neuer Eintrag', 'New entry'), keys: '⌘N' },
      { label: t('Etwas festhalten, aus jeder App', 'Capture something, from any app'), keys: '⌥⇧Space' },
      { label: t('Einstellungen', 'Settings'), keys: '⌘,' },
      { label: t('Sperren', 'Lock'), keys: '⌘⇧L' },
    ],
  },
  {
    title: t('Im Graph', 'In the graph'),
    items: [
      { label: t('Eintrag öffnen', 'Open an entry'), keys: t('Klick', 'Click') },
      { label: t('Zwei Einträge verbinden', 'Link two entries'), keys: t('⌥ + Ziehen', '⌥ + drag') },
      { label: t('Projekt öffnen', 'Open a project'), keys: t('Doppelklick', 'Double-click') },
      { label: t('Eine Ebene hoch', 'Up one level'), keys: 'Esc' },
      { label: t('Ausgewählten Eintrag verbinden', 'Link the selected entry'), keys: '⌘L' },
    ],
  },
  {
    title: t('Beim Schreiben', 'While writing'),
    items: [
      { label: t('Alle Formate', 'All formats'), keys: '/' },
      { label: t('Anderen Eintrag verlinken', 'Link another entry'), keys: '[[' },
      { label: t('Fett und kursiv', 'Bold and italic'), keys: '⌘B ⌘I' },
      { label: t('Sofort speichern', 'Save now'), keys: '⌘S' },
    ],
  },
];

/** Every MCP tool the server offers. A test in crates/bm-mcp keeps this complete. */
export const MCP_TOOLS: { title: string; tools: { name: string; about: string }[] }[] = [
  {
    title: t('Lesen', 'Reading'),
    tools: [
      { name: 'search', about: t('Einträge nach Wörtern und Bedeutung finden', 'Find entries by words and meaning') },
      { name: 'get', about: t('Einen Eintrag vollständig lesen', 'Read an entry in full') },
      { name: 'explore', about: t('Was um einen Eintrag herum liegt', 'What lies around an entry') },
      { name: 'find_path', about: t('Kürzester Weg zwischen zwei Einträgen', 'Shortest path between two entries') },
      { name: 'similar', about: t('Ähnliche Einträge und mögliche Duplikate', 'Similar entries and likely duplicates') },
      { name: 'recent_changes', about: t('Was sich zuletzt geändert hat und durch wen', 'What changed lately, and who changed it') },
      { name: 'unlinked', about: t('Einträge ohne Verbindung', 'Entries without links') },
      { name: 'unreviewed', about: t('Von KIs Geschriebenes, das du noch nicht gesehen hast', 'What AI clients wrote that you have not seen yet') },
    ],
  },
  {
    title: t('Schreiben', 'Writing'),
    tools: [
      { name: 'capture', about: t('Einträge und Verbindungen in einem Schritt anlegen', 'Create entries and links in one step') },
      { name: 'update', about: t('Einen Eintrag ändern oder ergänzen', 'Change or extend an entry') },
      { name: 'link', about: t('Zwei Einträge verbinden', 'Link two entries') },
      { name: 'unlink', about: t('Eine Verbindung lösen', 'Remove a link') },
      { name: 'merge', about: t('Duplikate zusammenführen', 'Merge duplicates') },
      { name: 'attach', about: t('Längeren Text als Datei anhängen', 'Attach longer text as a file') },
      { name: 'read_attachment', about: t('Einen Anhang lesen', 'Read an attachment') },
    ],
  },
  {
    title: t('Projekte', 'Projects'),
    tools: [
      { name: 'list_projects', about: t('Alle Projekte als Baum', 'All projects as a tree') },
      { name: 'project_overview', about: t('Zusammenfassung eines Projekts', 'Summary of a project') },
      { name: 'create_project', about: t('Projekt oder Unterprojekt anlegen', 'Create a project or subproject') },
      { name: 'update_project', about: t('Umbenennen, beschreiben, verschieben', 'Rename, describe, move') },
      { name: 'delete_project', about: t('Projekt löschen, die Einträge bleiben', 'Delete a project; its entries stay') },
      { name: 'add_to_project', about: t('Einträge einem Projekt zuordnen', 'Add entries to a project') },
      { name: 'remove_from_project', about: t('Einträge aus einem Projekt nehmen', 'Take entries out of a project') },
    ],
  },
  {
    title: t('Doku, Vorlagen und Dateien', 'Docs, templates and files'),
    tools: [
      { name: 'list_docs', about: t('Doku-Seiten eines Projekts', 'Documentation pages of a project') },
      { name: 'read_doc', about: t('Eine Seite oder einen Abschnitt lesen', 'Read a page or a section') },
      { name: 'write_doc', about: t('Eine Seite anlegen oder ersetzen', 'Create or replace a page') },
      { name: 'list_templates', about: t('Deine Vorlagen', 'Your templates') },
      { name: 'get_template', about: t('Eine Vorlage ausgefüllt holen', 'Get a template, filled in') },
      { name: 'save_template', about: t('Eine Vorlage anlegen oder ersetzen', 'Create or replace a template') },
      { name: 'link_file', about: t('Eine Datei auf dem Mac verknüpfen, du gibst sie frei', 'Link a file on the Mac; you allow it') },
      { name: 'read_linked_file', about: t('Eine freigegebene Datei lesen', 'Read an allowed file') },
    ],
  },
  {
    title: t('Rückgängig', 'Undo'),
    tools: [
      { name: 'trash', about: t('Papierkorb ansehen', 'Look into the trash') },
      { name: 'restore', about: t('Aus dem Papierkorb holen', 'Take out of the trash') },
      { name: 'delete', about: t('In den Papierkorb legen', 'Move to the trash') },
      { name: 'revert', about: t('Eine Änderung rückgängig machen', 'Undo a change') },
    ],
  },
];

/** The commands every connected model gets. See crates/bm-mcp/src/prompts.rs. */
export const MCP_COMMANDS: { name: string; about: string }[] = [
  { name: 'setup-project', about: t('Baut für ein Projekt Struktur und Doku auf', 'Builds structure and docs for a project') },
  { name: 'update-docs', about: t('Bringt die Doku eines Projekts auf den aktuellen Stand', 'Brings a project’s docs up to date') },
  { name: 'capture-session', about: t('Hält fest, was in dieser Sitzung wichtig war', 'Captures what mattered in this session') },
  { name: 'recall', about: t('Holt zusammen, was zu einem Thema bekannt ist', 'Gathers what is known about a topic') },
  { name: 'tidy-project', about: t('Räumt ein Projekt auf: Duplikate, Lücken, Veraltetes', 'Tidies a project: duplicates, gaps, stale entries') },
];
