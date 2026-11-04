/**
 * Brainmatter speaks German or English, as macOS does. The app tells the web view
 * which before any script runs (`src-tauri/src/i18n.rs`); in a browser, with the
 * mock, `?lang=de` or `?lang=en` decides, else the browser's language.
 *
 * Texts come in pairs, `t('Speichern', 'Save')`: two languages are few enough that
 * each text is easiest to read next to its translation.
 */

export type Lang = 'de' | 'en';

const injected = globalThis as { __BM_LANG?: string; __BM_LOCALE?: string };
const asked = typeof location === 'undefined' ? null : new URLSearchParams(location.search).get('lang');
// Outside a page, as in unit tests, German, as the app was written.
const browser = typeof location === 'undefined' ? 'de-DE' : (navigator.language ?? 'de-DE');

export const lang: Lang = (injected.__BM_LANG ?? asked ?? browser).startsWith('de') ? 'de' : 'en';

/** For dates and numbers. */
export const locale: string =
  injected.__BM_LOCALE ?? (lang === 'de' ? 'de-DE' : browser.startsWith('en') ? browser : 'en-US');

/** The text in the app's language. */
export const t = (de: string, en: string): string => (lang === 'de' ? de : en);

/** `1 Eintrag`, `3 Einträge`, with the number written for the locale. */
export function plural(n: number, de: [one: string, many: string], en: [one: string, many: string]): string {
  const [one, many] = lang === 'de' ? de : en;
  return `${n.toLocaleString(locale)} ${n === 1 ? one : many}`;
}

/** A number written for the locale. */
export const num = (n: number): string => n.toLocaleString(locale);

/** “Text” in the quotes of the language. */
export const quote = (text: string): string => (lang === 'de' ? `„${text}“` : `“${text}”`);
