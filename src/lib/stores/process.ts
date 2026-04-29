// src/lib/stores/process.ts

import { writable, derived } from 'svelte/store';

const API = (import.meta as any).env?.VITE_API_BASE ?? '';

// ── Types ─────────────────────────────────────────────────────────────────────

export interface SensorEntry  { id: string; label: string; }
export interface NodePosition { x: number; y: number; }

export interface NodeConfig {
  id:   string;
  type: string;
  [key: string]: any;
}

export interface EdgeConfig {
  id:        string;
  source:    string;
  target:    string;
  animated?: boolean;
}

export interface PipelineConfig {
  id:                    string;
  label:                 string;
  loop_interval_seconds: number;
  connections:           any;
  nodes:                 NodeConfig[];
  edges:                 EdgeConfig[];
  node_positions:        Record<string, NodePosition>;
}

export interface ProcessConfig {
  version:             number;
  shared_connections?: any;
  pipelines:           PipelineConfig[];
}

export interface NodeState {
  node_id:   string;
  node_type: string;
  data:      any;
  is_ready:  boolean;
}

export interface PipelineAgentState {
  pipeline_id:     string;
  label:           string;
  cycle:           number;
  is_ready:        boolean;
  override_active: boolean;
  node_states:     Record<string, NodeState>;
  last_signals:    Record<string, any>;
}

export interface Process {
  id:           string;
  name:         string;
  description:  string | null;
  type:         string;
  status:       'running' | 'stopped' | 'error' | 'unknown';
  config:       ProcessConfig | null;
  last_seen_at: string | null;
  owner_id:     string;
  created_at:   string;
  user_role:    string | null;
}

export interface ProcessLog {
  id:         string;
  process_id: string;
  ts:         string;
  level:      'debug' | 'info' | 'warn' | 'error';
  source:     string;
  message:    string;
  data:       any;
}

export interface ProcessReading {
  id:          string;
  pipeline_id: string;
  ts:          string;
  raw:         number[] | null;
  filtered:    number[] | null;
  p_diag:      number[] | null;
  decision:    number | null;
  actuator:    string | null;
}

// ── Store state ───────────────────────────────────────────────────────────────

interface ProcessStoreState {
  process:        Process | null;
  pipelineStates: Record<string, PipelineAgentState>;
  logs:           ProcessLog[];
  loading:        boolean;
  error:          string | null;
}

const INITIAL: ProcessStoreState = {
  process:        null,
  pipelineStates: {},
  logs:           [],
  loading:        false,
  error:          null,
};

function createProcessStore() {
  const { subscribe, update, set } = writable<ProcessStoreState>(INITIAL);
  let sseSource: EventSource | null = null;

  return {
    subscribe,

    async load(id: string) {
      update(s => ({ ...s, loading: true, error: null }));
      try {
        const [proc, logs] = await Promise.all([
          fetch(`${API}/api/v1/processes/${id}`, { credentials: 'include' }).then(r => r.json()),
          fetch(`${API}/api/v1/processes/${id}/logs?limit=100`, { credentials: 'include' }).then(r => r.json()),
        ]);

        // Cargar estado de cada pipeline
        const pipelineStates: Record<string, PipelineAgentState> = {};
        const pipelines: PipelineConfig[] = proc.config?.pipelines ?? [];

        await Promise.all(pipelines.map(async (pl: PipelineConfig) => {
          try {
            const state = await fetch(
              `${API}/api/v1/processes/${id}/agent-state/${pl.id}`,
              { credentials: 'include' }
            ).then(r => r.json());
            if (state && typeof state === 'object' && !state.error) {
              pipelineStates[pl.id] = { ...state, label: pl.label };
            }
          } catch {}
        }));

        update(s => ({
          ...s,
          process:        proc,
          pipelineStates,
          logs:           logs.logs ?? [],
          loading:        false,
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

      const connect = () => {
        sseSource = new EventSource(url, { withCredentials: true });

        sseSource.addEventListener('state', (e: MessageEvent) => {
          try {
            if (!e.data || e.data === 'ping') return;
            const payload = JSON.parse(e.data);
            update(s => {
              const proc = s.process ? {
                ...s.process,
                status:       payload.status ?? s.process.status,
                last_seen_at: payload.last_seen_at ?? s.process.last_seen_at,
              } : s.process;

              const newLogs: ProcessLog[] = Array.isArray(payload.new_logs)
                ? payload.new_logs : [];
              const logs = newLogs.length
                ? [...newLogs, ...s.logs].slice(0, 500)
                : s.logs;

              return { ...s, process: proc, logs };
            });
          } catch {}
        });

        sseSource.onerror = () => {
          if (sseSource) {
            sseSource.close();
            sseSource = null;
            setTimeout(connect, 5000);
          }
        };
      };

      connect();
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

    // ── Readings ──────────────────────────────────────────────────────────
    async fetchReadings(
      id: string,
      pipelineId: string,
      sinceHours: number,
      limit = 500
    ): Promise<ProcessReading[]> {
      const since = new Date(Date.now() - sinceHours * 3_600_000).toISOString();
      const res = await fetch(
        `${API}/api/v1/processes/${id}/readings?pipeline_id=${pipelineId}&since=${since}&limit=${limit}`,
        { credentials: 'include' }
      );
      const data = await res.json();
      return (data.readings ?? []).reverse(); // oldest first
    },

    // ── Config update ─────────────────────────────────────────────────────
    async saveConfig(id: string, config: ProcessConfig): Promise<void> {
      const res = await fetch(`${API}/api/v1/processes/${id}`, {
        method: 'PATCH', credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ config }),
      });
      if (!res.ok) {
        const d = await res.json();
        throw new Error(d.error ?? `HTTP ${res.status}`);
      }
      await this.load(id);
    },

    // ── Pipeline state refresh ────────────────────────────────────────────
    async refreshPipelineState(processId: string, pipelineId: string) {
      try {
        const state = await fetch(
          `${API}/api/v1/processes/${processId}/agent-state/${pipelineId}`,
          { credentials: 'include' }
        ).then(r => r.json());

        if (state && !state.error) {
          update(s => ({
            ...s,
            pipelineStates: {
              ...s.pipelineStates,
              [pipelineId]: state,
            }
          }));
        }
      } catch {}
    },
  };
}

export const processStore = createProcessStore();

// ── Derived ───────────────────────────────────────────────────────────────────

export const canOperate = derived(processStore, $s =>
  ['operator', 'admin'].includes($s.process?.user_role ?? '')
);

export const canAdmin = derived(processStore, $s =>
  $s.process?.user_role === 'admin'
);
