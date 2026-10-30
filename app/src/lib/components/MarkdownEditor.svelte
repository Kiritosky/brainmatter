<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { EditorState } from '@codemirror/state';
  import { EditorView, keymap, placeholder as placeholderExt } from '@codemirror/view';
  import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
  import { markdown } from '@codemirror/lang-markdown';
  import { HighlightStyle, syntaxHighlighting } from '@codemirror/language';
  import { autocompletion, type CompletionContext } from '@codemirror/autocomplete';
  import { tags as t } from '@lezer/highlight';

  let {
    value,
    onchange,
    onsave,
    titles,
    autofocus = false,
  }: {
    value: string;
    onchange: (text: string) => void;
    onsave?: () => void;
    /** Titles for `[[` completion. */
    titles: (prefix: string) => Promise<string[]>;
    autofocus?: boolean;
  } = $props();

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
    '&': { color: 'var(--color-text)', backgroundColor: 'transparent', fontSize: '13.5px' },
    '.cm-content': { fontFamily: 'var(--font-sans)', lineHeight: '1.6', caretColor: 'var(--color-accent)', padding: '4px 0' },
    '&.cm-focused': { outline: 'none' },
    '.cm-cursor': { borderLeftColor: 'var(--color-accent)' },
    '.cm-selectionBackground, &.cm-focused .cm-selectionBackground': {
      backgroundColor: 'color-mix(in srgb, var(--color-accent) 30%, transparent) !important',
    },
    '.cm-placeholder': { color: 'var(--color-faint)' },
    '.cm-tooltip': { background: 'var(--color-elevated)', border: '1px solid var(--color-line)', borderRadius: '8px' },
    '.cm-tooltip-autocomplete ul li[aria-selected]': { background: 'var(--color-accent)', color: 'white' },
  });

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
            indentWithTab,
            ...defaultKeymap,
            ...historyKeymap,
          ]),
          markdown(),
          syntaxHighlighting(highlight),
          EditorView.lineWrapping,
          autocompletion({ override: [wikilinkSource], icons: false }),
          placeholderExt('Schreibe in Markdown … [[ verlinkt andere Einträge'),
          theme,
          EditorView.updateListener.of((u) => {
            if (u.docChanged) onchange(u.state.doc.toString());
          }),
        ],
      }),
    });
    if (autofocus) view.focus();
  });

  onDestroy(() => view?.destroy());

  // External changes (e.g. an AI edited this entry) replace the document
  // unless they equal what is already there.
  $effect(() => {
    const next = value;
    if (view && next !== view.state.doc.toString()) {
      view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: next } });
    }
  });
</script>

<div bind:this={host} class="min-h-[160px]"></div>
