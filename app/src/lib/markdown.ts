// Markdown → sanitized HTML. Bodies may be written by AIs and must be treated
// as untrusted: everything goes through DOMPurify, and CSP blocks scripts.
import DOMPurify from 'dompurify';
import { Marked, type TokenizerAndRendererExtension } from 'marked';

const escape = (s: string) =>
  s.replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]!);

const wikilink: TokenizerAndRendererExtension = {
  name: 'wikilink',
  level: 'inline',
  start: (src) => src.indexOf('[['),
  tokenizer(src) {
    const m = /^\[\[([^\]\n|#]+)(?:#[^\]\n|]*)?(?:\|([^\]\n]+))?\]\]/.exec(src);
    if (!m) return undefined;
    return { type: 'wikilink', raw: m[0], target: m[1].trim(), label: (m[2] ?? m[1]).trim() };
  },
  renderer(token) {
    return `<span class="wikilink" data-target="${escape(token.target)}">${escape(token.label)}</span>`;
  },
};

const marked = new Marked({ gfm: true, breaks: false });
marked.use({ extensions: [wikilink] });

export function renderMarkdown(source: string, known?: (title: string) => boolean): string {
  const html = marked.parse(source, { async: false }) as string;
  const clean = DOMPurify.sanitize(html, { ADD_ATTR: ['data-target'], FORBID_TAGS: ['style', 'form'] });
  if (!known) return clean;
  // Mark links whose target does not exist yet.
  const doc = new DOMParser().parseFromString(`<div>${clean}</div>`, 'text/html');
  doc.querySelectorAll<HTMLElement>('.wikilink').forEach((el) => {
    if (!known(el.dataset.target ?? '')) el.classList.add('missing');
  });
  return doc.body.firstElementChild!.innerHTML;
}
