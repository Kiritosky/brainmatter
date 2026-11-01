// Personal display choices. They belong to this Mac rather than to the vault, and
// live in localStorage so the first frame, even on the lock screen, already has them.
const KEY = 'bm.prefs';

export type Accent = 'blue' | 'teal' | 'green' | 'amber' | 'rose';
export type Scale = 'small' | 'medium' | 'large';
export type LabelAmount = 'few' | 'normal' | 'many';

export interface Prefs {
  groupBackgrounds: boolean;
  groupNames: boolean;
  hoverPush: boolean;
  hoverDim: boolean;
  nodeSize: Scale;
  edgeWeight: Scale;
  labels: LabelAmount;
  accent: Accent;
  reduceMotion: boolean;
  aiActivity: boolean;
  whatsNew: boolean;
}

export const DEFAULT_PREFS: Readonly<Prefs> = {
  groupBackgrounds: true,
  groupNames: true,
  hoverPush: true,
  hoverDim: true,
  nodeSize: 'medium',
  edgeWeight: 'medium',
  labels: 'normal',
  accent: 'blue',
  reduceMotion: false,
  aiActivity: true,
  whatsNew: true,
};

function load(): Prefs {
  try {
    return { ...DEFAULT_PREFS, ...JSON.parse(localStorage.getItem(KEY) ?? '{}') };
  } catch {
    return { ...DEFAULT_PREFS };
  }
}

class PrefsState {
  values = $state<Prefs>(load());

  constructor() {
    this.applyToPage();
  }

  private applyToPage() {
    const root = document.documentElement;
    if (this.values.accent === 'blue') delete root.dataset.accent;
    else root.dataset.accent = this.values.accent;
    if (this.values.reduceMotion) root.dataset.motion = 'reduced';
    else delete root.dataset.motion;
  }

  private save() {
    localStorage.setItem(KEY, JSON.stringify(this.values));
    this.applyToPage();
  }

  set<K extends keyof Prefs>(key: K, value: Prefs[K]) {
    this.values[key] = value;
    this.save();
  }

  reset() {
    this.values = { ...DEFAULT_PREFS };
    this.save();
  }
}

export const prefs = new PrefsState();

/** The user can ask for less movement here or in the system settings. */
export const motionAllowed = () => !prefs.values.reduceMotion && !matchMedia('(prefers-reduced-motion: reduce)').matches;
