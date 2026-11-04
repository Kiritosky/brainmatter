import { expect, test } from 'bun:test';
import { readFileSync } from 'node:fs';

// The app shows one changelog or the other, and a release needs both.
const versions = (file: string) =>
  [...readFileSync(new URL(`../../../${file}`, import.meta.url), 'utf8').matchAll(/^## (\d+\.\d+\.\d+)/gm)].map((m) => m[1]);

test('the English changelog has every version the German one has', () => {
  expect(versions('CHANGELOG.en.md')).toEqual(versions('CHANGELOG.md'));
});
