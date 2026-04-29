// src/lib/stores/process.ts
import { writable, derived, get } from 'svelte/store';

export interface Process {
  id:           string;
  name:         string;
  description:  string | null;
  type:         string;
  status:       'running' | 'stopped' | 'error' | 'unknown';
  control_url:  string;
  config:       any;
  last_state:   ProcessState | null;
  last_seen_at: string | null;
  owner_id:     string;
  created_at:   string;
  user_role:    string | null;
}

export interface ProcessState {
  kalman: {
    convergido:   boolean;
    x_hat:        number[];
    P:            number[];
    Q_base:       number[];
    R:            number[];
    P_convergencia: number;
    n_updates:    number;
  };
  estado_lineas:  Record<string, string>;   // "1" → "on" | "off" | "desconocido"
  rangos_linea:   Record<string, [number, number]>;
  overrides:      Record<string, boolean | null>;
  campbell_crudo: Record<string, number | null>;
  calibrado:      Record<string, number | null>;
}

export interface ProcessLog {
  id:         string;
  process_id: string;
  ts:         string;
  level:      'debug' | 'info' | 'warn' | 'error';
  source:     'control' | 'user' | 'system' | 'worker';
  message:    string;
  data:       any;
  user_id:    string | null;
}

export interface ValveEvent {
  id:                string;
  process_id:        string;
  ts:                string;
  linea:             string;
  estado:            boolean;
  modo:              'auto' | 'override' | 'system';
  triggered_by:      string | null;
  kalman_convergido: boolean | null;
  kalman_x_hat:      number[] | null;
}

export interface ProcessCollaborator {
  user_id:  string;
  role:     'viewer' | 'operator' | 'admin';
  email:    string;
  added_at: string;
}

// ── Store state ───────────────────────────────────────────────────────────────

interface ProcessStoreState {
  process:       Process | null;
  logs:          ProcessLog[];
  valveEvents:   ValveEvent[];
  collaborators: ProcessCollaborator[];
  loading:       boolean;
  error:         string | null;
  liveLog:       string[];   // líneas del Control.log tail
}

const INITIAL: ProcessStoreState = {
  process:       null,
  logs:          [],
  valveEvents:   [],
  collaborators: [],
  loading:       false,
  error:         null,
  liveLog:       [],
};

const API = (typeof import.meta !== 'undefined' && (import.meta as any).env?.VITE_API_BASE) ?? '';

function createProcessStore() {
  const { subscribe, update, set } = writable<ProcessStoreState>(INITIAL);
  let sseSource: EventSource | null = null;

  return {
    subscribe,

    async load(id: string) {
      update(s => ({ ...s, loading: true, error: null }));
      try {
        const [proc, logs, valveEvents, collabs] = await Promise.all([
          fetch(`${API}/api/v1/processes/${id}`, { credentials: 'include' }).then(r => r.json()),
          fetch(`${API}/api/v1/processes/${id}/logs?limit=100`, { credentials: 'include' }).then(r => r.json()),
          fetch(`${API}/api/v1/processes/${id}/valve-events?limit=50`, { credentials: 'include' }).then(r => r.json()),
          fetch(`${API}/api/v1/processes/${id}/collaborators`, { credentials: 'include' }).then(r => r.json()),
        ]);
        update(s => ({
          ...s,
          process:       proc,
          logs:          logs.logs ?? [],
          valveEvents:   valveEvents.events ?? [],
          collaborators: collabs.collaborators ?? [],
          loading:       false,
        }));
      } catch (e: any) {
        update(s => ({ ...s, loading: false, error: e.message }));
      }
    },

    reset() {
      this.stopSSE();
      set(INITIAL);
    },

    // ── SSE ────────────────────────────────────────────────────────────────
    startSSE(id: string, intervalSecs = 5) {
      this.stopSSE();
      const url = `${API}/api/v1/processes/${id}/stream?interval_secs=${intervalSecs}`;
      sseSource = new EventSource(url, { withCredentials: true });

      sseSource.addEventListener('state', (e: MessageEvent) => {
        try {
          const payload = JSON.parse(e.data);
          update(s => {
            const proc = s.process ? {
              ...s.process,
              status:      payload.status ?? s.process.status,
              last_state:  payload.last_state ?? s.process.last_state,
              last_seen_at: payload.last_seen_at ?? s.process.last_seen_at,
            } : s.process;

            // Agregar logs nuevos al frente
            const newLogs: ProcessLog[] = payload.new_logs ?? [];
            const logs = newLogs.length
              ? [...newLogs, ...s.logs].slice(0, 500)
              : s.logs;

            return { ...s, process: proc, logs };
          });
        } catch {}
      });

      sseSource.onerror = () => {
        update(s => s.process ? {
          ...s, process: { ...s.process, status: 'unknown' }
        } : s);
      };
    },

    stopSSE() {
      sseSource?.close();
      sseSource = null;
    },

    // ── Comandos ──────────────────────────────────────────────────────────
    async command(id: string, cmd: Record<string, any>): Promise<any> {
      const res = await fetch(`${API}/api/v1/processes/${id}/command`, {
        method: 'POST', credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(cmd),
      });
      const data = await res.json();
      if (!res.ok) throw new Error(data.error ?? `HTTP ${res.status}`);
      return data;
    },

    // ── Control.log tail ──────────────────────────────────────────────────
    async fetchLogTail(id: string) {
      const res = await fetch(`${API}/api/v1/processes/${id}/logs/tail`, {
        credentials: 'include',
      });
      const data = await res.json();
      if (data.ok) update(s => ({ ...s, liveLog: data.lines ?? [] }));
    },

    // ── Colaboradores ─────────────────────────────────────────────────────
    async addCollaborator(id: string, user_id: string, role: string) {
      await fetch(`${API}/api/v1/processes/${id}/collaborators`, {
        method: 'POST', credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ user_id, role }),
      });
      const res = await fetch(`${API}/api/v1/processes/${id}/collaborators`, { credentials: 'include' });
      const data = await res.json();
      update(s => ({ ...s, collaborators: data.collaborators ?? [] }));
    },

    async removeCollaborator(id: string, user_id: string) {
      await fetch(`${API}/api/v1/processes/${id}/collaborators/${user_id}`, {
        method: 'DELETE', credentials: 'include',
      });
      update(s => ({
        ...s,
        collaborators: s.collaborators.filter(c => c.user_id !== user_id),
      }));
    },

    // ── Setters locales ───────────────────────────────────────────────────
    updateState(state: ProcessState) {
      update(s => s.process ? {
        ...s, process: { ...s.process, last_state: state }
      } : s);
    },

    prependLog(log: ProcessLog) {
      update(s => ({ ...s, logs: [log, ...s.logs].slice(0, 500) }));
    },
  };
}

export const processStore = createProcessStore();

// ── Derived helpers ───────────────────────────────────────────────────────────

export const canOperate = derived(processStore, $s =>
  ['operator', 'admin'].includes($s.process?.user_role ?? '')
);

export const canAdmin = derived(processStore, $s =>
  $s.process?.user_role === 'admin'
);

export const processState = derived(processStore, $s =>
  $s.process?.last_state ?? null
);
