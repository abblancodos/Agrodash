// src/lib/stores/preferences.ts
//
// Preferencias de UI persistidas en localStorage.
// Actualmente: font scale (sm / md / lg).
// Fácil de extender con más preferencias sin tocar los componentes.

import { writable } from 'svelte/store';

export type FontScale = 'sm' | 'md' | 'lg';

export interface Preferences {
  fontScale: FontScale;
}

export const FONT_SCALES: Record<FontScale, number> = {
  sm: 0.85,
  md: 1.0,
  lg: 1.2,
};

const STORAGE_KEY = 'agrodash-prefs';

const DEFAULT: Preferences = { fontScale: 'md' };

function load(): Preferences {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return DEFAULT;
    return { ...DEFAULT, ...JSON.parse(raw) };
  } catch {
    return DEFAULT;
  }
}

function save(p: Preferences) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(p));
  } catch {}
}

function createPreferences() {
  const { subscribe, set, update } = writable<Preferences>(DEFAULT);

  return {
    subscribe,

    /** Llamar una vez al montar el layout para cargar desde localStorage */
    init() {
      set(load());
    },

    setFontScale(scale: FontScale) {
      update(p => {
        const next = { ...p, fontScale: scale };
        save(next);
        return next;
      });
    },
  };
}

export const preferences = createPreferences();