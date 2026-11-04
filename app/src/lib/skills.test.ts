import { describe, expect, test } from 'bun:test';
import { cleanFilePath, filesLabel, parseSkillAddress, shortRevision, skillBytes, skillSlug, suggestFilePath } from './skills';

describe('reading a GitHub address', () => {
  test('a repo, with or without the address around it', () => {
    const repo = { repo: 'example/agent-skills', ref: null, path: '' };
    expect(parseSkillAddress('https://github.com/example/agent-skills')).toEqual(repo);
    expect(parseSkillAddress('  github.com/example/agent-skills/ ')).toEqual(repo);
    expect(parseSkillAddress('https://www.github.com/example/agent-skills.git')).toEqual(repo);
    expect(parseSkillAddress('example/agent-skills')).toEqual(repo);
  });

  test('a folder in a branch', () => {
    expect(parseSkillAddress('https://github.com/example/agent-skills/tree/main/skills/pdf-forms')).toEqual({
      repo: 'example/agent-skills',
      ref: 'main',
      path: 'skills/pdf-forms',
    });
    expect(parseSkillAddress('https://github.com/example/agent-skills/tree/v2')).toEqual({ repo: 'example/agent-skills', ref: 'v2', path: '' });
  });

  test('a SKILL.md stands for its folder', () => {
    expect(parseSkillAddress('https://github.com/example/agent-skills/blob/main/skills/commit-messages/SKILL.md?plain=1#L4')).toEqual({
      repo: 'example/agent-skills',
      ref: 'main',
      path: 'skills/commit-messages',
    });
    expect(parseSkillAddress('https://github.com/example/one-skill/blob/main/SKILL.md')).toEqual({ repo: 'example/one-skill', ref: 'main', path: '' });
  });

  test('anything else is not one', () => {
    for (const text of [
      '',
      'example',
      'https://gitlab.com/example/agent-skills',
      'https://github.com/example',
      'https://github.com/example/agent-skills/blob/main/README.md',
      'https://github.com/example/agent-skills/issues/3',
      'https://github.com/example/agent-skills/tree/main/../secrets',
      'example/agent-skills/tree/main/skills',
      'git@github.com:example/agent-skills.git',
      '/Users/alex/skills',
      'exa mple/agent-skills',
    ]) {
      expect(parseSkillAddress(text)).toBeNull();
    }
  });
});

describe('the name an AI calls a skill by', () => {
  test('like a template name', () => {
    expect(skillSlug('PDF-Formulare ausfüllen')).toBe('pdf-formulare-ausfuellen');
    expect(skillSlug('  Größe & Maße!  ')).toBe('groesse-masse');
    expect(skillSlug('commit-messages')).toBe('commit-messages');
  });

  test('nothing usable gives nothing', () => {
    expect(skillSlug('!!! …')).toBe('');
  });
});

describe('files of a skill', () => {
  test('go where skills usually keep them', () => {
    expect(suggestFilePath('fill_form.py')).toBe('scripts/fill_form.py');
    expect(suggestFilePath('Reference.MD')).toBe('references/Reference.MD');
    expect(suggestFilePath('logo.png')).toBe('assets/logo.png');
    expect(suggestFilePath('Makefile')).toBe('assets/Makefile');
  });

  test('a path is tidied up', () => {
    expect(cleanFilePath(' ./scripts//fill_form.py ')).toBe('scripts/fill_form.py');
    expect(cleanFilePath('scripts\\check.sh')).toBe('scripts/check.sh');
    expect(cleanFilePath('/references/my notes.md')).toBe('references/my notes.md');
  });

  test('a path must stay inside the folder and leave SKILL.md alone', () => {
    expect(cleanFilePath('')).toBeNull();
    expect(cleanFilePath('../outside.py')).toBeNull();
    expect(cleanFilePath('scripts/../../x')).toBeNull();
    expect(cleanFilePath('skill.md')).toBeNull();
    expect(cleanFilePath('docs/SKILL.md')).toBe('docs/SKILL.md');
    expect(cleanFilePath('scripts/ x.py')).toBeNull();
  });

  test('size counts the instructions in bytes and every file', () => {
    expect(skillBytes('äb', [10, 5])).toBe(18);
    expect(skillBytes('', [])).toBe(0);
  });

  test('their number reads as words, none included', () => {
    expect(filesLabel(0)).toBe('keine Dateien');
    expect(filesLabel(1)).toBe('1 Datei');
    expect(filesLabel(3)).toBe('3 Dateien');
  });

  test('a revision is shown short', () => {
    expect(shortRevision('9f2c4e1b7a03d5c8e6f1a2b3c4d5e6f7a8b9c0d1')).toBe('9f2c4e1');
  });
});
