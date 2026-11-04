import { lang } from '#lib/i18n.ts';

/**
 * Change summaries are written once, in German, by the engine and stay in the
 * history as they are (AI clients read them too). In English, the known forms are
 * translated for display; anything else is shown as written.
 */
const FORMS: [RegExp, (...parts: string[]) => string][] = [
  [/^Projekt „(.+)“ in „(.+)“ umbenannt$/, (a, b) => `Renamed project “${a}” to “${b}”`],
  [/^Projekt „(.+)“ erstellt$/, (a) => `Created project “${a}”`],
  [/^Projekt „(.+)“ bearbeitet$/, (a) => `Edited project “${a}”`],
  [/^Projekt „(.+)“ verschoben$/, (a) => `Moved project “${a}”`],
  [/^Projekt „(.+)“ gelöscht$/, (a) => `Deleted project “${a}”`],
  [/^Doku-Seite „(.+)“ erstellt$/, (a) => `Created documentation page “${a}”`],
  [/^Doku aus „(.+)“ aktualisiert$/, (a) => `Updated documentation from “${a}”`],
  [/^Doku aus „(.+)“ entfernt$/, (a) => `Removed documentation from “${a}”`],
  [/^Anhang „(.+)“ entfernt$/, (a) => `Removed attachment “${a}”`],
  [/^Verknüpfung „(.+)“ entfernt$/, (a) => `Removed file link “${a}”`],
  [/^„(.+)“ an „(.+)“ angehängt$/, (a, b) => `Attached “${a}” to “${b}”`],
  [/^„(.+)“ mit „(.+)“ verknüpft$/, (a, b) => `Linked “${a}” to “${b}”`],
  [/^(\d+) in „(.+)“ zusammengeführt$/, (n, b) => `Merged ${n} into “${b}”`],
  [/^„(.+)“ für KIs freigegeben$/, (a) => `Shared “${a}” with AI clients`],
  [/^„(.+)“ für KIs gesperrt$/, (a) => `Hid “${a}” from AI clients`],
  [/^„(.+)“ erstellt$/, (a) => `Created “${a}”`],
  [/^„(.+)“ bearbeitet$/, (a) => `Edited “${a}”`],
  [/^„(.+)“ gelöscht$/, (a) => `Deleted “${a}”`],
  [/^„(.+)“ wiederhergestellt$/, (a) => `Restored “${a}”`],
  [/^„(.+)“ importiert$/, (a) => `Imported “${a}”`],
  [/^(\d+) Einträge, (\d+) Verbindungen erfasst$/, (n, m) => `Captured ${n} entries and ${m} links`],
  [/^(\d+) Einträge zu Projekt hinzugefügt$/, (n) => `Added ${n} entries to a project`],
  [/^(\d+) Einträge aus Projekt entfernt$/, (n) => `Removed ${n} entries from a project`],
  [/^Verbindung erstellt$/, () => 'Created a link'],
  [/^Verbindung entfernt$/, () => 'Removed a link'],
  [/^Änderung rückgängig gemacht$/, () => 'Undid a change'],
];

/** A change summary in the app's language. */
export function summary(text: string): string {
  if (lang === 'de') return text;
  for (const [form, english] of FORMS) {
    const found = form.exec(text);
    if (found) return english(...found.slice(1));
  }
  return text;
}
