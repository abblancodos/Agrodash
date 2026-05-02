// src/lib/stores/process.ts — Svelte 5 runes

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

export type ProcessStatus = 'running' | 'stopping' | 'stopped' | 'error' | 'unknown';

export interface Process {
  id:           string;
  name:         string;
  description?: string;
  status:       ProcessStatus;
  config:       ProcessConfig;
  last_seen_at: string | null;
  updated_at:   string;
  user_role?:   string;
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

export interface NodeState {
  node_id:   string;
  node_type: string;
  data:      Record<string, any>;
  is_ready:  boolean;
}

export interface ProcessLog {
  id?:      string;
  ts:       string;
  level:    string;
  source:   string;
  message:  string;
  data?:    any;
}

export interface ProcessReading {
  id:           string;
  pipeline_id:  string;
  ts:           string;
  raw:          number[] | null;
  filtered:     number[] | null;
  p_diag:       number[] | null;
  decision:     number | null;
  actuator:     string | null;
  scope_values: Record<string, number[] | string> | null;
}

export type TestStatus = 'ok' | 'warn' | 'error' | 'info';
export interface TestCheck {
  name:   string;
  status: TestStatus;
  detail: string;
  nodes?: any;
}

export interface SelfTestResult {
  overall: TestStatus;
  checks:  TestCheck[];
}

// ── Store (Svelte 5 runes) ────────────────────────────────────────────────────
// Usamos un objeto con propiedades $state — funciona como módulo singleton.
// Los componentes importan `processStore` y leen sus propiedades directamente.

function createStore() {
  let process        = $state<Process | null>(null);
  let pipelineStates = $state<Record<string, PipelineAgentState>>({});
  let logs           = $state<ProcessLog[]>([]);
  let loading        = $state(false);
  let error          = $state<string | null>(null);
  let testResult     = $state<SelfTestResult | null>(null);
  let testLoading    = $state(false);
  let lastCycle      = $state(0);

  return {
    // Exponer estado como getters reactivos
    get process()        { return process; },
    get pipelineStates() { return pipelineStates; },
    get logs()           { return logs; },
    get loading()        { return loading; },
    get error()          { return error; },
    get testResult()     { return testResult; },
    get testLoading()    { return testLoading; },
    get lastCycle()      { return lastCycle; },

    // Derived
    get canOperate() {
      return ['operator', 'admin'].includes(process?.user_role ?? '');
    },
    get canAdmin() {
      return process?.user_role === 'admin';
    },

    // ── Carga inicial ──────────────────────────────────────────────────────
    async load(id: string) {
      loading = true; error = null;
      try {
        const [proc, logsData] = await Promise.all([
          fetch(`${API}/api/v1/processes/${id}`, { credentials: 'include' }).then(r => r.json()),
          fetch(`${API}/api/v1/processes/${id}/logs?limit=100`, { credentials: 'include' }).then(r => r.json()),
        ]);

        const newStates: Record<string, PipelineAgentState> = {};
        await Promise.all((proc.config?.pipelines ?? []).map(async (pl: PipelineConfig) => {
          try {
            const state = await fetch(
              `${API}/api/v1/processes/${id}/agent-state/${pl.id}`,
              { credentials: 'include' }
            ).then(r => r.json());
            if (state && typeof state === 'object' && !state.error) {
              newStates[pl.id] = { ...state, label: pl.label };
            }
          } catch {}
        }));

        process        = proc;
        pipelineStates = newStates;
        logs           = logsData.logs ?? [];
        loading        = false;
      } catch (e: any) {
        loading = false;
        error   = e.message;
      }
    },

    reset() {
      process        = null;
      pipelineStates = {};
      logs           = [];
      loading        = false;
      error          = null;
      testResult     = null;
      testLoading    = false;
      lastCycle      = 0;
    },

    // ── SSE (legacy, mantenido para compatibilidad) ────────────────────────
    stopSSE() {},
    startSSE(_id: string, _secs = 10) {},

    // ── Proceso ────────────────────────────────────────────────────────────
    async start(id: string) {
      const res = await fetch(`${API}/api/v1/processes/${id}/start`, {
        method: 'POST', credentials: 'include',
      });
      if (!res.ok) {
        const d = await res.json().catch(() => ({}));
        throw new Error(d.error ?? `HTTP ${res.status}`);
      }
      if (process) process = { ...process, status: 'running' };
    },

    async stop(id: string) {
      const res = await fetch(`${API}/api/v1/processes/${id}/stop`, {
        method: 'POST', credentials: 'include',
      });
      if (!res.ok && res.status !== 202) {
        const d = await res.json().catch(() => ({}));
        throw new Error(d.error ?? `HTTP ${res.status}`);
      }
      if (process) process = { ...process, status: 'stopping' };
    },

    patchStatus(status: string, lastSeenAt: string | null) {
      if (process) process = { ...process, status: status as ProcessStatus, last_seen_at: lastSeenAt };
    },

    // ── Logs ───────────────────────────────────────────────────────────────
    setLogs(newLogs: ProcessLog[]) {
      logs = newLogs;
    },

    // ── Self-test ──────────────────────────────────────────────────────────
    async selfTest(id: string, healthOnly = true): Promise<SelfTestResult> {
      testLoading = true; testResult = null;
      try {
        const res = await fetch(`${API}/api/v1/processes/${id}/test?health_only=${healthOnly}`, {
          method: 'POST', credentials: 'include',
        });
        const data: SelfTestResult = await res.json();
        if (!res.ok) throw new Error((data as any).error ?? `HTTP ${res.status}`);
        testResult  = data;
        testLoading = false;
        return data;
      } catch (e: any) {
        testLoading = false;
        throw e;
      }
    },

    clearTestResult() { testResult = null; },

    // ── Comandos ───────────────────────────────────────────────────────────
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

    // ── Readings ───────────────────────────────────────────────────────────
    async fetchReadings(
      id: string, pipelineId: string, sinceHours: number, limit = 500
    ): Promise<ProcessReading[]> {
      const since = new Date(Date.now() - sinceHours * 3_600_000).toISOString();
      const res = await fetch(
        `${API}/api/v1/processes/${id}/readings?pipeline_id=${pipelineId}&since=${since}&limit=${limit}`,
        { credentials: 'include' }
      );
      const data = await res.json();
      return (data.readings ?? []).reverse();
    },

    // ── Config ────────────────────────────────────────────────────────────
    async saveConfig(id: string, config: ProcessConfig) {
      const res = await fetch(`${API}/api/v1/processes/${id}`, {
        method: 'PATCH', credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ config }),
      });
      if (!res.ok) {
        const d = await res.json();
        throw new Error(d.error ?? `HTTP ${res.status}`);
      }
      if (process) process = { ...process, config };
    },

    // ── Pipeline state ─────────────────────────────────────────────────────
    async refreshPipelineState(processId: string, pipelineId: string) {
      try {
        const res = await fetch(
          `${API}/api/v1/processes/${processId}/agent-state/${pipelineId}`,
          { credentials: 'include' }
        );
        if (!res.ok) return;
        const state = await res.json();
        if (state && !state.error) {
          const prevCycle = pipelineStates[pipelineId]?.cycle ?? 0;
          const newCycle  = state.cycle ?? 0;
          pipelineStates = { ...pipelineStates, [pipelineId]: state };
          if (newCycle > prevCycle) lastCycle += 1;
        }
      } catch {}
    },
  };
}

export const processStore = createStore();

// ── Compatibilidad Svelte 4 → 5 ───────────────────────────────────────────────
// Los componentes que usen $processStore necesitan un store compatible.
// Exportamos un proxy que implementa .subscribe() para compatibilidad.
// Los componentes nuevos usen processStore directamente sin $.

// canOperate y canAdmin se leen directamente: processStore.canOperate
// No exportamos derived stores — todos leen del objeto directamente.