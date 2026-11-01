// What changed in each version. CHANGELOG.md is built into the app, so this also
// works offline and always matches the installed version.
import source from '../../../CHANGELOG.md?raw';

export interface Release {
  version: string;
  date: string | null;
  /** Markdown. */
  notes: string;
}

/** Parses `## 1.2.3 · date` sections, newest first as they are in the file. */
export function parseChangelog(text: string): Release[] {
  const out: Release[] = [];
  let current: Release | null = null;
  for (const line of text.split('\n')) {
    const heading = /^## (\d+\.\d+\.\d+)(?:\s*·\s*(.+))?\s*$/.exec(line);
    if (heading) {
      current = { version: heading[1], date: heading[2] ?? null, notes: '' };
      out.push(current);
    } else if (current) {
      current.notes += `${line}\n`;
    }
  }
  return out.map((r) => ({ ...r, notes: r.notes.trim() }));
}

export function isNewer(a: string, b: string): boolean {
  const parts = (v: string) => v.split('.').map(Number);
  const [x, y] = [parts(a), parts(b)];
  for (let i = 0; i < 3; i++) {
    if ((x[i] ?? 0) !== (y[i] ?? 0)) return (x[i] ?? 0) > (y[i] ?? 0);
  }
  return false;
}

export const releases = parseChangelog(source);

/** Versions newer than `seen`, up to and including `current`. */
export function releasesSince(seen: string | null, current: string): Release[] {
  return releases.filter(
    (r) => !isNewer(r.version, current) && (seen ? isNewer(r.version, seen) : r.version === current),
  );
}
