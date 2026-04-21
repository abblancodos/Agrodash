// src/lib/stores/downloading.ts
//
// Store global que indica si hay una descarga CSV en curso.
// Cuando downloading = true:
//   - SensorChart pausa sus efectos y no lanza fetchReadings
//   - La UI muestra un overlay de pantalla completa
//   - El fondo queda difuminado e ininteractuable

import { writable } from 'svelte/store';

export interface DownloadState {
  active:   boolean;
  label:    string;   // "Caja S — descargando 3/5 sensores..."
  progress: number;   // 0–100
}

function createDownloadingStore() {
  const { subscribe, set, update } = writable<DownloadState>({
    active:   false,
    label:    '',
    progress: 0,
  });

  return {
    subscribe,

    start(label: string) {
      set({ active: true, label, progress: 0 });
    },

    setProgress(progress: number, label?: string) {
      update(s => ({ ...s, progress, ...(label ? { label } : {}) }));
    },

    finish() {
      // Breve delay para que el usuario vea el 100% antes de cerrar
      update(s => ({ ...s, progress: 100 }));
      setTimeout(() => set({ active: false, label: '', progress: 0 }), 400);
    },

    cancel() {
      set({ active: false, label: '', progress: 0 });
    },
  };
}

export const downloading = createDownloadingStore();
