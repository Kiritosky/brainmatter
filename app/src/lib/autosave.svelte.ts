import { api, errorMessage, type Id, type Node } from './api';

const SAVE_DELAY_MS = 900;

/**
 * The text of one entry while it is being written.
 *
 * What the server says comes back after a delay, while the user keeps typing. An
 * answer must therefore never replace what is on screen unless it really is
 * newer: only nothing unsaved and nothing on its way lets it through, and `rev`
 * tells late echoes from new text.
 */
export class EntryDraft {
  title = $state('');
  body = $state('');
  dirty = $state(false);
  inFlight = $state(0);

  private saved = { title: '', body: '' };
  private newestRev = 0;
  private timer: ReturnType<typeof setTimeout> | undefined;

  constructor(
    private readonly id: Id,
    private readonly onerror: (message: string) => void,
  ) {}

  /** The entry as the server has it now. */
  accept(node: Node) {
    if (node.rev < this.newestRev) return;
    this.newestRev = node.rev;
    // The answer to the save on its way decides, not an echo of an earlier one.
    if (this.inFlight > 0) return;
    this.saved = { title: node.title, body: node.body };
    if (!this.dirty) {
      this.title = node.title;
      this.body = node.body;
    }
  }

  /** Call after changing `title` or `body`. */
  changed() {
    this.dirty = true;
    clearTimeout(this.timer);
    this.timer = setTimeout(() => this.save(), SAVE_DELAY_MS);
  }

  /** The draft goes away with its entry, so what is unsaved must not be written later. */
  discard() {
    clearTimeout(this.timer);
    this.dirty = false;
  }

  async save() {
    clearTimeout(this.timer);
    if (!this.dirty) return;
    this.dirty = false;
    const patch: Parameters<typeof api.updateNode>[1] = {};
    if (this.title.trim() && this.title !== this.saved.title) patch.title = this.title;
    if (this.body !== this.saved.body) patch.body = { mode: 'replace', text: this.body };
    if (!patch.title && !patch.body) return;
    this.inFlight++;
    try {
      const node = await api.updateNode(this.id, patch);
      this.newestRev = Math.max(this.newestRev, node.rev);
      if (this.inFlight === 1) this.saved = { title: node.title, body: node.body };
    } catch (e) {
      // `saved` still holds the old text, so the next change sends this again.
      this.dirty = true;
      this.onerror(errorMessage(e));
    } finally {
      this.inFlight--;
    }
  }
}
