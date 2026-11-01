<script lang="ts">
  import { onDestroy, onMount, untrack } from 'svelte';
  import { Annotation, EditorSelection, EditorState, Transaction } from '@codemirror/state';
  import { EditorView, keymap, placeholder as placeholderExt } from '@codemirror/view';
  import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
  import { markdown } from '@codemirror/lang-markdown';
  import { HighlightStyle, syntaxHighlighting } from '@codemirror/language';
  import { acceptCompletion, autocompletion, pickedCompletion, startCompletion, type Completion, type CompletionContext } from '@codemirror/autocomplete';
  import { tags as t } from '@lezer/highlight';
  import { FORMAT_COMMANDS, PLACEHOLDER_COMMAND, findFormatCommands, formatInsertion, type FormatCommand } from '#lib/format-commands.ts';

  let {
    value,
    onchange,
    onsave,
    onescape,
    titles,
    onpastefile,
    autofocus = false,
    large = false,
    placeholders = false,
  }: {
    /** Stores a pasted file and returns the Markdown to insert for it. */
    onpastefile?: (file: File) => Promise<string | null>;
    value: string;
    onchange: (text: string) => void;
    onsave?: () => void;
    onescape?: () => void;
    /** Titles for `[[` completion. */
    titles: (prefix: string) => Promise<string[]>;
    autofocus?: boolean;
    /** Text size of a whole page rather than a side panel. */
    large?: boolean;
    /** Offers `{{name}}` too, for templates. */
    placeholders?: boolean;
  } = $props();

  /** Set on changes that come from outside, which must not count as typing. */
  const fromOutside = Annotation.define<boolean>();

  // Each stays as it was on creation: the editor is built once.
  const slashCommands = untrack(() => (placeholders ? [PLACEHOLDER_COMMAND, ...FORMAT_COMMANDS] : FORMAT_COMMANDS));
  const isLarge = untrack(() => large);

  let host: HTMLDivElement;
  let view: EditorView | undefined;

  const highlight = HighlightStyle.define([
    { tag: t.heading1, fontSize: '1.35em', fontWeight: '650' },
    { tag: t.heading2, fontSize: '1.18em', fontWeight: '650' },
    { tag: t.heading3, fontWeight: '600' },
    { tag: t.strong, fontWeight: '650' },
    { tag: t.emphasis, fontStyle: 'italic' },
    { tag: t.link, color: 'var(--color-accent)' },
    { tag: t.url, color: 'var(--color-faint)' },
    { tag: t.monospace, fontFamily: 'var(--font-mono)', color: '#f5c451' },
    { tag: t.quote, color: 'var(--color-muted)' },
    { tag: [t.processingInstruction, t.meta], color: 'var(--color-faint)' },
  ]);

  const theme = EditorView.theme({
    '&': { color: 'var(--color-text)', backgroundColor: 'transparent', fontSize: isLarge ? '15px' : '13.5px' },
    '.cm-content': { fontFamily: 'var(--font-sans)', lineHeight: isLarge ? '1.7' : '1.6', caretColor: 'var(--color-accent)', padding: '4px 0' },
    // Flush with the title above, and with the rendered page.
    '.cm-line': { padding: '0' },
    '&.cm-focused': { outline: 'none' },
    '.cm-cursor': { borderLeftColor: 'var(--color-accent)' },
    '.cm-selectionBackground, &.cm-focused .cm-selectionBackground': {
      backgroundColor: 'color-mix(in srgb, var(--color-accent) 30%, transparent) !important',
    },
    '.cm-placeholder': { color: 'var(--color-faint)' },
    '.cm-tooltip.cm-tooltip-autocomplete': {
      background: 'var(--color-panel)',
      border: '1px solid var(--color-line)',
      borderRadius: '10px',
      boxShadow: '0 12px 32px -8px rgb(0 0 0 / 0.5)',
      padding: '4px',
    },
    '.cm-tooltip.cm-tooltip-autocomplete > ul': { fontFamily: 'var(--font-sans)', fontSize: '13px', maxHeight: '264px', minWidth: '250px' },
    '.cm-tooltip.cm-tooltip-autocomplete > ul > li': { display: 'flex', alignItems: 'center', gap: '16px', borderRadius: '6px', padding: '5px 8px', lineHeight: '1.3' },
    '.cm-tooltip.cm-tooltip-autocomplete > ul > li[aria-selected]': {
      background: 'color-mix(in srgb, var(--color-text) 10%, transparent)',
      color: 'var(--color-text)',
    },
    '.cm-completionMatchedText': { textDecoration: 'none', color: 'var(--color-accent)' },
    '.cm-completionDetail': { marginLeft: 'auto', fontStyle: 'normal', fontFamily: 'var(--font-mono)', fontSize: '11px', color: 'var(--color-faint)' },
  });

  /** Replaces what was typed for the command with its Markdown. */
  function applyFormat(command: FormatCommand) {
    return (editor: EditorView, completion: Completion, from: number, to: number) => {
      const atLineStart = editor.state.doc.lineAt(from).from === from;
      const { text, selectFrom, selectTo } = formatInsertion(command, atLineStart);
      editor.dispatch({
        changes: { from, to, insert: text },
        selection: EditorSelection.single(from + selectFrom, from + selectTo),
        annotations: pickedCompletion.of(completion),
        userEvent: 'input.complete',
      });
      if (command.completes) startCompletion(editor);
    };
  }

  /** `/` after a space or at the start of a line opens the list of formats. */
  function slashSource(ctx: CompletionContext) {
    const m = ctx.matchBefore(/(?:^|\s)\/[\p{L}\p{N}-]*$/u);
    if (!m) return null;
    const typed = m.text.trimStart();
    const found = findFormatCommands(typed.slice(1), slashCommands);
    if (!found.length) return null;
    return {
      from: m.to - typed.length,
      filter: false,
      options: found.map((c) => ({ label: c.name, detail: c.hint, apply: applyFormat(c) })),
    };
  }

  /** Puts Markdown around the selection, or around a word to type over. */
  function wrap(mark: string, word: string) {
    return (editor: EditorView) => {
      editor.dispatch(
        editor.state.changeByRange((range) => {
          const inner = editor.state.sliceDoc(range.from, range.to) || word;
          const start = range.from + mark.length;
          return {
            changes: { from: range.from, to: range.to, insert: `${mark}${inner}${mark}` },
            range: EditorSelection.range(start, start + inner.length),
          };
        }),
      );
      return true;
    };
  }

  /** Inserts at the cursor, for buttons beside the editor. */
  export function insert(text: string) {
    if (!view) return;
    // A cursor still at the very start means nobody has put it anywhere yet.
    const untouched = view.state.selection.main.empty && view.state.selection.main.head === 0;
    if (untouched) view.dispatch({ selection: { anchor: view.state.doc.length } });
    view.dispatch(view.state.replaceSelection(text));
    view.focus();
  }

  /** Completes `[[partial` with existing titles and closes the link. */
  async function wikilinkSource(ctx: CompletionContext) {
    const m = ctx.matchBefore(/\[\[[^\]\n]*$/);
    if (!m) return null;
    const prefix = m.text.slice(2);
    const options = (await titles(prefix)).map((title) => ({ label: title, apply: `${title}]]` }));
    return { from: m.from + 2, options, validFor: /^[^\]\n]*$/ };
  }

  onMount(() => {
    view = new EditorView({
      parent: host,
      state: EditorState.create({
        doc: value,
        extensions: [
          history(),
          keymap.of([
            { key: 'Mod-s', run: () => (onsave?.(), true) },
            { key: 'Mod-b', run: wrap('**', 'fett') },
            { key: 'Mod-i', run: wrap('*', 'kursiv') },
            { key: 'Escape', run: () => (onescape ? (onescape(), true) : false) },
            // Before indenting, so Tab takes the marked format when the list is open.
            { key: 'Tab', run: acceptCompletion },
            indentWithTab,
            ...defaultKeymap,
            ...historyKeymap,
          ]),
          markdown(),
          syntaxHighlighting(highlight),
          EditorView.lineWrapping,
          autocompletion({ override: [slashSource, wikilinkSource], icons: false }),
          placeholderExt('Schreib einfach los. / zeigt alle Formate, [[ verlinkt andere Einträge.'),
          theme,
          EditorView.domEventHandlers({
            paste(event, editor) {
              const files = [...(event.clipboardData?.files ?? [])];
              if (!files.length || !onpastefile) return false;
              event.preventDefault();
              for (const file of files) {
                onpastefile(file).then((snippet) => {
                  if (snippet) editor.dispatch(editor.state.replaceSelection(snippet));
                });
              }
              return true;
            },
          }),
          EditorView.updateListener.of((u) => {
            if (u.docChanged && !u.transactions.some((tr) => tr.annotation(fromOutside))) onchange(u.state.doc.toString());
          }),
        ],
      }),
    });
    if (autofocus) view.focus();
  });

  onDestroy(() => view?.destroy());

  // External changes (e.g. an AI edited this entry) touch only what differs, so
  // the cursor stays where it is. They are neither typing nor an undo step.
  $effect(() => {
    const next = value;
    if (!view) return;
    const doc = view.state.doc.toString();
    if (next === doc) return;
    let start = 0;
    while (start < doc.length && start < next.length && doc[start] === next[start]) start++;
    let endDoc = doc.length;
    let endNext = next.length;
    while (endDoc > start && endNext > start && doc[endDoc - 1] === next[endNext - 1]) {
      endDoc--;
      endNext--;
    }
    view.dispatch({
      changes: { from: start, to: endDoc, insert: next.slice(start, endNext) },
      annotations: [fromOutside.of(true), Transaction.addToHistory.of(false)],
    });
  });
</script>

<div bind:this={host} class="min-h-[160px]"></div>
