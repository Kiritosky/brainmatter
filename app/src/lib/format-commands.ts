// What `/` offers in the editor, for anyone who does not know Markdown by heart.
// Aliases hold both languages, since people type the words they know.

import { t } from '#lib/i18n.ts';

export interface FormatCommand {
  name: string;
  /** Other words people would type for it. */
  aliases: string[];
  /** The Markdown it stands for, shown beside the name. */
  hint: string;
  /** Inserted before the cursor. */
  before: string;
  /** Inserted after the cursor. */
  after?: string;
  /** Starts on its own line, even when typed at the end of a sentence. */
  block?: boolean;
  /** Selects the text between `before` and `after`, to be typed over. */
  placeholder?: string;
  /** Opens the list of entries right away. */
  completes?: boolean;
}

export const FORMAT_COMMANDS: FormatCommand[] = [
  { name: t('Überschrift 1', 'Heading 1'), aliases: ['h1', 'titel', 'heading', 'title'], hint: '#', before: '# ', block: true },
  { name: t('Überschrift 2', 'Heading 2'), aliases: ['h2', 'heading'], hint: '##', before: '## ', block: true },
  { name: t('Überschrift 3', 'Heading 3'), aliases: ['h3', 'heading'], hint: '###', before: '### ', block: true },
  { name: t('Liste', 'List'), aliases: ['aufzählung', 'punkte', 'bullet', 'ul', 'list'], hint: '-', before: '- ', block: true },
  { name: t('Nummerierte Liste', 'Numbered list'), aliases: ['zahlen', 'ol', 'numbered', 'nummeriert'], hint: '1.', before: '1. ', block: true },
  { name: t('Aufgabe', 'Task'), aliases: ['todo', 'checkbox', 'task', 'haken', 'aufgabe'], hint: '- [ ]', before: '- [ ] ', block: true },
  { name: t('Zitat', 'Quote'), aliases: ['quote', 'hinweis', 'zitat'], hint: '>', before: '> ', block: true },
  { name: t('Code-Block', 'Code block'), aliases: ['code', 'snippet', 'pre'], hint: '```', before: '```\n', after: '\n```', block: true },
  {
    name: t('Tabelle', 'Table'),
    aliases: ['table', 'spalten', 'tabelle', 'columns'],
    hint: '| |',
    before: '| ',
    placeholder: t('Spalte', 'Column'),
    after: ` | ${t('Spalte', 'Column')} |\n| --- | --- |\n|  |  |`,
    block: true,
  },
  { name: t('Trennlinie', 'Divider'), aliases: ['linie', 'hr', 'divider', 'line'], hint: '---', before: '---\n', block: true },
  { name: t('Fett', 'Bold'), aliases: ['bold', 'stark', 'fett'], hint: '**', before: '**', placeholder: t('fett', 'bold'), after: '**' },
  { name: t('Kursiv', 'Italic'), aliases: ['italic', 'schräg', 'kursiv'], hint: '*', before: '*', placeholder: t('kursiv', 'italic'), after: '*' },
  { name: t('Code im Text', 'Inline code'), aliases: ['inline', 'mono'], hint: '`', before: '`', placeholder: 'code', after: '`' },
  { name: 'Link', aliases: ['url', 'web'], hint: '[ ]( )', before: '[', placeholder: t('Text', 'text'), after: '](https://)' },
  { name: t('Verweis auf Eintrag', 'Link to entry'), aliases: ['wikilink', 'verknüpfen', 'eintrag', 'entry'], hint: '[[ ]]', before: '[[', completes: true },
];

/** What a template adds to the list. */
export const PLACEHOLDER_COMMAND: FormatCommand = {
  name: t('Platzhalter', 'Placeholder'),
  aliases: ['variable', 'vorlage', 'template', 'platzhalter'],
  hint: '{{ }}',
  before: '{{',
  placeholder: 'name',
  after: '}}',
};

export function findFormatCommands(query: string, commands: FormatCommand[] = FORMAT_COMMANDS): FormatCommand[] {
  const q = query.trim().toLowerCase();
  if (!q) return commands;
  const starts = (c: FormatCommand) => c.name.toLowerCase().startsWith(q) || c.aliases.some((a) => a.startsWith(q));
  const contains = (c: FormatCommand) => c.name.toLowerCase().includes(q);
  return [...commands.filter(starts), ...commands.filter((c) => !starts(c) && contains(c))];
}

/** The text a command leaves in the document, and where the selection goes in it. */
export function formatInsertion(command: FormatCommand, atLineStart: boolean) {
  const lead = command.block && !atLineStart ? '\n\n' : '';
  const start = lead.length + command.before.length;
  const middle = command.placeholder ?? '';
  return {
    text: `${lead}${command.before}${middle}${command.after ?? ''}`,
    selectFrom: start,
    selectTo: start + middle.length,
  };
}
