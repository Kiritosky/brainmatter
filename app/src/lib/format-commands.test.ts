import { describe, expect, test } from 'bun:test';
import { FORMAT_COMMANDS, PLACEHOLDER_COMMAND, findFormatCommands, formatInsertion } from './format-commands';

const named = (name: string) => FORMAT_COMMANDS.find((c) => c.name === name)!;

describe('finding a format', () => {
  test('no query lists everything', () => {
    expect(findFormatCommands('')).toHaveLength(FORMAT_COMMANDS.length);
  });

  test('by German name and by the word from other tools', () => {
    expect(findFormatCommands('tab')[0].name).toBe('Tabelle');
    expect(findFormatCommands('h2')[0].name).toBe('Überschrift 2');
    expect(findFormatCommands('todo')[0].name).toBe('Aufgabe');
  });

  test('what starts with the query comes before what merely contains it', () => {
    const names = findFormatCommands('liste').map((c) => c.name);
    expect(names).toEqual(['Liste', 'Nummerierte Liste']);
  });

  test('nothing is offered for a word that is no format', () => {
    expect(findFormatCommands('usr')).toEqual([]);
  });
});

describe('inserting a format', () => {
  test('a block typed after text starts on its own line', () => {
    expect(formatInsertion(named('Überschrift 2'), false).text).toBe('\n\n## ');
    expect(formatInsertion(named('Überschrift 2'), true).text).toBe('## ');
  });

  test('inline formats stay in the sentence, with their word selected', () => {
    const { text, selectFrom, selectTo } = formatInsertion(named('Fett'), false);
    expect(text).toBe('**fett**');
    expect(text.slice(selectFrom, selectTo)).toBe('fett');
  });

  test('the cursor lands inside a code block', () => {
    const { text, selectFrom, selectTo } = formatInsertion(named('Code-Block'), true);
    expect(text).toBe('```\n\n```');
    expect(selectFrom).toBe(4);
    expect(selectTo).toBe(4);
  });

  test('a table is complete Markdown', () => {
    const { text } = formatInsertion(named('Tabelle'), true);
    expect(text.split('\n')).toEqual(['| Spalte | Spalte |', '| --- | --- |', '|  |  |']);
  });

  test('a placeholder has the form templates use', () => {
    expect(formatInsertion(PLACEHOLDER_COMMAND, false).text).toBe('{{name}}');
  });
});
