// src/lib/stores/preferences.ts

import { writable } from 'svelte/store';

export type FontScale = 'sm' | 'md' | 'lg';

export interface Preferences {
  fontScale: FontScale;
  fontScaleValue: number;  // valor real 0.8–1.5
}

export const FONT_SCALES: Record<FontScale, number> = {
  sm: 0.85,
  md: 1.0,
  lg: 1.2,
};

const STORAGE_KEY = 'agrodash-prefs';
const DEFAULT: Preferences = { fontScale: 'md', fontScaleValue: 1.0 };

function load(): Preferences {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return DEFAULT;
    return { ...DEFAULT, ...JSON.parse(raw) };
  } catch { return DEFAULT; }
}

function save(p: Preferences) {
  try { localStorage.setItem(STORAGE_KEY, JSON.stringify(p)); } catch {}
}

function createPreferences() {
  const { subscribe, set, update } = writable<Preferences>(DEFAULT);
  return {
    subscribe,
    init() { set(load()); },
    setFontScale(scale: FontScale) {
      update(p => {
        const next = { ...p, fontScale: scale, fontScaleValue: FONT_SCALES[scale] };
        save(next); return next;
      });
    },
    setFontScaleValue(value: number) {
      // Mapear al FontScale más cercano para compatibilidad
      const entry = Object.entries(FONT_SCALES).reduce((best, [k, v]) =>
        Math.abs(v - value) < Math.abs(FONT_SCALES[best as FontScale] - value) ? k : best
      , 'md' as string) as FontScale;
      update(p => {
        const next = { ...p, fontScale: entry, fontScaleValue: value };
        save(next); return next;
      });
    },
  };
}

export const preferences = createPreferences();
