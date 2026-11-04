// Rules for skills that the interface and the mock share. The vault checks them
// again; these only let the interface answer before asking.
import { plural, t } from '#lib/i18n.ts';

export const MAX_SKILL_FILES = 100;
export const MAX_SKILL_BYTES = 5 * 1024 * 1024;

/** What a GitHub address points at: a whole repo, a folder in it, or one SKILL.md. */
export interface SkillAddress {
  /** `owner/repo` */
  repo: string;
  /** Branch, tag or commit; null for the default branch. */
  ref: string | null;
  /** Folder inside the repo, without slashes at either end; empty for all of it. */
  path: string;
}

const OWNER = /^[A-Za-z0-9](?:[A-Za-z0-9-]{0,38})$/;
const REPO = /^[A-Za-z0-9._-]{1,100}$/;

/**
 * Reads `https://github.com/owner/repo`, `…/tree/<ref>/<path>`,
 * `…/blob/<ref>/<path>/SKILL.md` and `owner/repo`. A ref with a slash in it
 * cannot be told from the path, so the first part after `tree` or `blob` is the ref.
 */
export function parseSkillAddress(text: string): SkillAddress | null {
  let rest = text.trim().replace(/[?#].*$/, '');
  const web = /^(?:https?:\/\/)?(?:www\.)?github\.com\//i;
  const full = web.test(rest);
  if (full) rest = rest.replace(web, '');
  else if (rest.includes(':') || rest.startsWith('/')) return null;
  const parts = rest.split('/').filter(Boolean);
  if (parts.length < 2) return null;
  const owner = parts[0];
  const repo = parts[1].replace(/\.git$/i, '');
  if (!OWNER.test(owner) || !REPO.test(repo) || repo === '.' || repo === '..') return null;
  const repoName = `${owner}/${repo}`;
  if (parts.length === 2) return { repo: repoName, ref: null, path: '' };
  // The short form is only ever a repo.
  if (!full) return null;
  const [kind, ref, ...path] = parts.slice(2);
  if (!ref || (kind !== 'tree' && kind !== 'blob')) return null;
  if (path.some((p) => p === '..' || p === '.')) return null;
  if (kind === 'blob') {
    if (path.at(-1)?.toLowerCase() !== 'skill.md') return null;
    path.pop();
  }
  return { repo: repoName, ref, path: path.join('/') };
}

/** Same as `slug` in crates/bm-core/src/templates.rs: the name an AI calls it by. */
export function skillSlug(title: string): string {
  let out = '';
  for (const c of title.toLowerCase()) {
    if (c === 'ä') out += 'ae';
    else if (c === 'ö') out += 'oe';
    else if (c === 'ü') out += 'ue';
    else if (c === 'ß') out += 'ss';
    else if (/[a-z0-9]/.test(c)) out += c;
    else if (out && !out.endsWith('-')) out += '-';
  }
  return out.replace(/-+$/, '');
}

/** Where a file usually goes in a skill folder, by what it is. */
export function suggestFilePath(name: string): string {
  const base = name.split(/[\\/]/).pop() ?? name;
  const ext = base.includes('.') ? base.split('.').pop()!.toLowerCase() : '';
  if (['py', 'sh', 'bash', 'zsh', 'js', 'mjs', 'ts', 'rb', 'pl', 'go', 'rs', 'swift'].includes(ext)) return `scripts/${base}`;
  if (['md', 'txt', 'pdf', 'rst', 'html', 'csv', 'json', 'yaml', 'yml', 'xml'].includes(ext)) return `references/${base}`;
  return `assets/${base}`;
}

/**
 * A path inside the skill folder as it is stored, or null if it cannot be one:
 * it must stay inside the folder and must not replace SKILL.md.
 */
export function cleanFilePath(path: string): string | null {
  const parts = path.trim().replaceAll('\\', '/').split('/').filter((p) => p && p !== '.');
  if (!parts.length || parts.some((p) => p === '..' || p.trim() !== p)) return null;
  const clean = parts.join('/');
  if (clean.toLowerCase() === 'skill.md') return null;
  return clean;
}

/** `3 Dateien`, or that there are none. */
export const filesLabel = (count: number): string =>
  count ? plural(count, ['Datei', 'Dateien'], ['file', 'files']) : t('keine Dateien', 'no files');

/** The first seven characters, as GitHub shows a commit. */
export const shortRevision = (revision: string): string => revision.slice(0, 7);

/** How much a skill takes up: its instructions and every file. */
export const skillBytes = (body: string, fileSizes: number[]): number =>
  new TextEncoder().encode(body).length + fileSizes.reduce((sum, size) => sum + size, 0);
