// src/lib/stores/process.svelte.ts — Svelte 5 runes

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


// ── WebSocket ─────────────────────────────────────────────────────────────────

export type WsStatus = 'disconnected' | 'connecting' | 'connected' | 'error';

export interface MqttAckEvent {
  pipeline_id: string;
  actuator_id: string;
  msg: string;
}

function createStore() {
  let process        = $state<Process | null>(null);
  let pipelineStates = $state<Record<string, PipelineAgentState>>({});
  let logs           = $state<ProcessLog[]>([]);
  let loading        = $state(false);
  let error          = $state<string | null>(null);
  let testResult     = $state<SelfTestResult | null>(null);
  let testLoading    = $state(false);
  let lastCycle      = $state(0);

  // ── WebSocket ────────────────────────────────────────────────────────────
  let wsStatus       = $state<WsStatus>('disconnected');
  let wsSocket: WebSocket | null = null;
  let wsReconnectTimer: ReturnType<typeof setTimeout> | null = null;
  let wsProcessId: string | null = null;
  // Callbacks registrados por actuadores para recibir ACKs MQTT
  let mqttAckHandlers = new Map<string, (msg: string) => void>();

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
    get wsStatus()       { return wsStatus; },

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
      this.wsDisconnect();
    },

    // ── SSE (legacy, mantenido para compatibilidad) ────────────────────────
    stopSSE() {},
    startSSE(_id: string, _secs = 10) {},

    // ── WebSocket ──────────────────────────────────────────────────────────

    /**
     * Conectar al WebSocket del proceso.
     * Llamar desde el componente que monta la vista del proceso.
     * Reconecta automáticamente con backoff exponencial.
     */
    wsConnect(processId: string) {
      if (wsProcessId === processId && wsStatus === 'connected') return;
      wsProcessId = processId;
      this._wsOpen(processId, 1000);
    },

    wsDisconnect() {
      if (wsReconnectTimer) { clearTimeout(wsReconnectTimer); wsReconnectTimer = null; }
      if (wsSocket) {
        wsSocket.onclose = null; // no reconectar al cerrar intencionalmente
        wsSocket.close();
        wsSocket = null;
      }
      wsStatus = 'disconnected';
      wsProcessId = null;
      mqttAckHandlers.clear();
    },

    /**
     * Registrar un handler para ACKs MQTT de un actuador específico.
     * El ActuatorRow llama esto al enviar un comando.
     * Se desregistra automáticamente cuando el actuador termina el tracking.
     */
    onMqttAck(actuatorId: string, handler: (msg: string) => void) {
      mqttAckHandlers.set(actuatorId, handler);
    },

    offMqttAck(actuatorId: string) {
      mqttAckHandlers.delete(actuatorId);
    },

    /**
     * Enviar un comando por WebSocket (no usa fetch).
     * Equivalente a processStore.command() pero sin HTTP round-trip.
     */
    wsSend(msg: Record<string, unknown>) {
      if (wsSocket && wsSocket.readyState === WebSocket.OPEN) {
        wsSocket.send(JSON.stringify(msg));
        return true;
      }
      return false;
    },

    _wsOpen(processId: string, delay: number) {
      if (wsReconnectTimer) { clearTimeout(wsReconnectTimer); wsReconnectTimer = null; }
      wsStatus = 'connecting';

      wsReconnectTimer = setTimeout(() => {
        wsReconnectTimer = null;
        const API = (import.meta as any).env?.VITE_API_BASE ?? '';
        const wsBase = API
          ? API.replace(/^https/, 'wss').replace(/^http/, 'ws')
          : `${window.location.protocol === 'https:' ? 'wss' : 'ws'}://${window.location.host}`;
        const wsUrl = wsBase + `/api/v1/processes/${processId}/ws`;

        const socket = new WebSocket(wsUrl);
        wsSocket = socket;

        socket.onopen = () => {
          wsStatus = 'connected';
          // keepalive ping cada 25s
          const ping = setInterval(() => {
            if (socket.readyState === WebSocket.OPEN) {
              socket.send(JSON.stringify({ type: 'ping' }));
            } else {
              clearInterval(ping);
            }
          }, 25_000);
          (socket as any)._pingInterval = ping;
        };

        socket.onmessage = (e: MessageEvent) => {
          try {
            const msg = JSON.parse(e.data);
            switch (msg.type) {
              case 'pipeline_state': {
                const prevCycle = pipelineStates[msg.pipeline_id]?.cycle ?? 0;
                const newCycle  = msg.state?.cycle ?? 0;
                pipelineStates = { ...pipelineStates, [msg.pipeline_id]: msg.state };
                if (newCycle > prevCycle) lastCycle += 1;
                break;
              }
              case 'process_status': {
                if (process) {
                  process = { ...process, status: msg.status, last_seen_at: msg.last_seen_at ?? null };
                }
                break;
              }
              case 'mqtt_ack': {
                const handler = mqttAckHandlers.get(msg.actuator_id);
                if (handler) handler(msg.msg as string);
                break;
              }
              case 'log': {
                logs = [...logs.slice(-199), {
                  ts: new Date().toISOString(),
                  level: msg.level,
                  source: msg.source,
                  message: msg.message,
                }];
                break;
              }
              case 'pong':
                break; // keepalive OK
              case 'error':
                console.warn('[WS] server error:', msg.message);
                break;
            }
          } catch {}
        };

        socket.onerror = () => { wsStatus = 'error'; };

        socket.onclose = () => {
          const interval = (socket as any)._pingInterval;
          if (interval) clearInterval(interval);
          wsSocket = null;

          // Solo reconectar si es el mismo proceso y la desconexión no fue intencional
          if (wsProcessId === processId) {
            wsStatus = 'error';
            const nextDelay = Math.min(delay * 1.5, 30_000);
            this._wsOpen(processId, nextDelay);
          }
        };
      }, delay);
    },

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
      id: string, pipelineId: string, sinceHours = 6, limit = 500,
      since?: Date, until?: Date,
    ): Promise<ProcessReading[]> {
      const sinceDate = since ?? new Date(Date.now() - sinceHours * 3_600_000);
      let url = `${API}/api/v1/processes/${id}/readings?pipeline_id=${pipelineId}&since=${sinceDate.toISOString()}&limit=${limit}`;
      if (until) url += `&until=${until.toISOString()}`;
      const res = await fetch(url, { credentials: 'include' });
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

// Todos los componentes leen del objeto directamente:
//   processStore.canAdmin, processStore.canOperate, etc.
// No hay derived stores separados — el compilador de Svelte 5
// procesa las runes correctamente en archivos .svelte.ts