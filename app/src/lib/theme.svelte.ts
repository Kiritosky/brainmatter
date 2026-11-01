// Light or dark. The choice also lives in localStorage, so the first frame after
// starting already has the right colours, before the settings have loaded.
import './prefs.svelte.ts';

const KEY = 'bm.theme';

export type ThemeMode = 'system' | 'light' | 'dark';

class ThemeState {
  /** Bumped when colours change, for the graph, whose WebGL needs concrete ones. */
  tick = $state(0);

  constructor() {
    this.write((localStorage.getItem(KEY) as ThemeMode | null) ?? 'system');
    matchMedia('(prefers-color-scheme: light)').addEventListener('change', () => this.tick++);
  }

  private write(mode: ThemeMode) {
    if (mode === 'system') delete document.documentElement.dataset.theme;
    else document.documentElement.dataset.theme = mode;
  }

  apply(mode: ThemeMode) {
    localStorage.setItem(KEY, mode);
    this.write(mode);
    this.tick++;
  }
}

export const theme = new ThemeState();
