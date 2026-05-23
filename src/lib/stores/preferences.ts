// src/lib/stores/preferences.ts

import { writable } from 'svelte/store';

export type FontScale  = 'sm' | 'md' | 'lg';
export type CardSort   = 'anomalia' | 'reciente' | 'nombre-az' | 'nombre-za';
export type SensorSort = 'score' | 'reciente' | 'alfa';

// ── Per-box config (persisted keyed by box.id) ────────────────────────────
export interface BoxPrefs {
  activePreset:    string;           // '1h' | '6h' | '24h' | '7d' | '30d' | 'custom'
  fromMs:          number;           // Date.getTime()
  toMs:            number;
  sensorSort:      SensorSort;
  hideOlderThanH:  number | null;
  hideLowVariance: boolean;
}

// ── Global dashboard config ───────────────────────────────────────────────
export interface DashPrefs {
  cardSort:       CardSort;
  cardTypeFilter: string;            // '' = todos
  mode:           string;            // 'compacto' | 'cards' | 'graficas' | 'analisis'
  live:           boolean;
  activePreset:   string;            // preset global de tiempo
  fromMs:         number;
  toMs:           number;
}

export interface Preferences {
  fontScale:            FontScale;
  fontScaleValue:       number;
  expFontScale:         FontScale;
  expFontScaleValue:    number;
  dash:                 DashPrefs;
  boxes:                Record<string, BoxPrefs>;
}

export const FONT_SCALES: Record<FontScale, number> = {
  sm: 0.9,
  md: 1.0,
  lg: 1.2,
};

const now = Date.now();
const DEFAULT_DASH: DashPrefs = {
  cardSort:       'anomalia',
  cardTypeFilter: '',
  mode:           'cards',
  live:           false,
  activePreset:   '24h',
  fromMs:         now - 24 * 3_600_000,
  toMs:           now,
};

const DEFAULT_BOX: BoxPrefs = {
  activePreset:    '24h',
  fromMs:          now - 24 * 3_600_000,
  toMs:            now,
  sensorSort:      'score',
  hideOlderThanH:  null,
  hideLowVariance: false,
};

const DEFAULT: Preferences = {
  fontScale:         'md',
  fontScaleValue:    1.0,
  expFontScale:      'lg',
  expFontScaleValue: 1.2,
  dash:              DEFAULT_DASH,
  boxes:             {},
};

const STORAGE_KEY = 'agrodash-prefs';

function load(): Preferences {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return DEFAULT;
    const parsed = JSON.parse(raw);
    return {
      ...DEFAULT,
      ...parsed,
      dash:  { ...DEFAULT_DASH,  ...(parsed.dash  ?? {}) },
      boxes: parsed.boxes ?? {},
    };
  } catch { return DEFAULT; }
}

function save(p: Preferences) {
  try { localStorage.setItem(STORAGE_KEY, JSON.stringify(p)); } catch {}
}

function defaultBox(): BoxPrefs {
  const n = Date.now();
  return {
    ...DEFAULT_BOX,
    fromMs: n - 24 * 3_600_000,
    toMs:   n,
  };
}

function createPreferences() {
  const { subscribe, set, update } = writable<Preferences>(DEFAULT);

  return {
    subscribe,

    init() { set(load()); },

    // ── Font scale ──────────────────────────────────────────────────────────
    setFontScale(scale: FontScale) {
      update(p => {
        const next = { ...p, fontScale: scale, fontScaleValue: FONT_SCALES[scale] };
        save(next); return next;
      });
    },
    setFontScaleValue(value: number) {
      const entry = Object.entries(FONT_SCALES).reduce((best, [k, v]) =>
        Math.abs(v - value) < Math.abs(FONT_SCALES[best as FontScale] - value) ? k : best
      , 'md' as string) as FontScale;
      update(p => {
        const next = { ...p, fontScale: entry, fontScaleValue: value };
        save(next); return next;
      });
    },
    setExpFontScale(scale: FontScale) {
      update(p => {
        const next = { ...p, expFontScale: scale, expFontScaleValue: FONT_SCALES[scale] };
        save(next); return next;
      });
    },
    setExpFontScaleValue(value: number) {
      const entry = Object.entries(FONT_SCALES).reduce((best, [k, v]) =>
        Math.abs(v - value) < Math.abs(FONT_SCALES[best as FontScale] - value) ? k : best
      , 'lg' as string) as FontScale;
      update(p => {
        const next = { ...p, expFontScale: entry, expFontScaleValue: value };
        save(next); return next;
      });
    },

    // ── Dashboard global ────────────────────────────────────────────────────
    setDash(patch: Partial<DashPrefs>) {
      update(p => {
        const next = { ...p, dash: { ...p.dash, ...patch } };
        save(next); return next;
      });
    },

    // ── Per-box ─────────────────────────────────────────────────────────────
    getBox(boxId: string): BoxPrefs {
      // Leer directamente del localStorage en vez de depender del store
      // porque este método se llama en $derived antes de que el store reactive
      try {
        const raw = localStorage.getItem(STORAGE_KEY);
        if (raw) {
          const p = JSON.parse(raw);
          const b = p.boxes?.[boxId];
          if (b) return { ...DEFAULT_BOX, ...b };
        }
      } catch {}
      return defaultBox();
    },

    setBox(boxId: string, patch: Partial<BoxPrefs>) {
      update(p => {
        const prev = p.boxes[boxId] ?? defaultBox();
        const next = {
          ...p,
          boxes: {
            ...p.boxes,
            [boxId]: { ...prev, ...patch },
          },
        };
        save(next); return next;
      });
    },
  };
}

export const preferences = createPreferences();