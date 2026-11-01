// What `/` offers in the editor, for anyone who does not know Markdown by heart.

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
  { name: 'Überschrift 1', aliases: ['h1', 'titel', 'heading'], hint: '#', before: '# ', block: true },
  { name: 'Überschrift 2', aliases: ['h2', 'heading'], hint: '##', before: '## ', block: true },
  { name: 'Überschrift 3', aliases: ['h3', 'heading'], hint: '###', before: '### ', block: true },
  { name: 'Liste', aliases: ['aufzählung', 'punkte', 'bullet', 'ul'], hint: '-', before: '- ', block: true },
  { name: 'Nummerierte Liste', aliases: ['zahlen', 'ol', 'numbered'], hint: '1.', before: '1. ', block: true },
  { name: 'Aufgabe', aliases: ['todo', 'checkbox', 'task', 'haken'], hint: '- [ ]', before: '- [ ] ', block: true },
  { name: 'Zitat', aliases: ['quote', 'hinweis'], hint: '>', before: '> ', block: true },
  { name: 'Code-Block', aliases: ['code', 'snippet', 'pre'], hint: '```', before: '```\n', after: '\n```', block: true },
  {
    name: 'Tabelle',
    aliases: ['table', 'spalten'],
    hint: '| |',
    before: '| ',
    placeholder: 'Spalte',
    after: ' | Spalte |\n| --- | --- |\n|  |  |',
    block: true,
  },
  { name: 'Trennlinie', aliases: ['linie', 'hr', 'divider'], hint: '---', before: '---\n', block: true },
  { name: 'Fett', aliases: ['bold', 'stark'], hint: '**', before: '**', placeholder: 'fett', after: '**' },
  { name: 'Kursiv', aliases: ['italic', 'schräg'], hint: '*', before: '*', placeholder: 'kursiv', after: '*' },
  { name: 'Code im Text', aliases: ['inline', 'mono'], hint: '`', before: '`', placeholder: 'code', after: '`' },
  { name: 'Link', aliases: ['url', 'web'], hint: '[ ]( )', before: '[', placeholder: 'Text', after: '](https://)' },
  { name: 'Verweis auf Eintrag', aliases: ['wikilink', 'verknüpfen', 'eintrag'], hint: '[[ ]]', before: '[[', completes: true },
];

/** What a template adds to the list. */
export const PLACEHOLDER_COMMAND: FormatCommand = {
  name: 'Platzhalter',
  aliases: ['variable', 'vorlage', 'template'],
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
