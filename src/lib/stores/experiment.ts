// src/lib/stores/experiment.ts
//
// Estado central del experimento activo.
// Se inicializa en [id]/+layout.svelte y se consume en las tres pestañas.

import { writable, derived, get } from 'svelte/store';
import { auth } from '$lib/stores/auth';

const API = typeof window !== 'undefined'
  ? (import.meta.env?.VITE_API_BASE ?? '')
  : '';

// ── Tipos ─────────────────────────────────────────────────────────────────────

export interface Experiment {
  id:           string;
  owner_id:     string;
  template_id:  string | null;
  title:        string;
  description:  string | null;
  public:       boolean;
  constants:    Record<string, unknown>;
  status:       'active' | 'completed' | 'archived';
  created_at:   string;
  user_role:    'admin' | 'editor' | 'viewer' | null;
  columns:      ExperimentColumn[];
  schema_notes: string | null;
}

export interface ExperimentEvent {
  id:                string;
  experiment_id:     string;
  step_key:          string;
  event_type:        string;
  soil_id:           string | null;
  iteration:         number | null;
  data:              Record<string, unknown>;
  note:              string | null;
  recorded_by:       string | null;
  recorded_by_name:  string | null;
  corrects_event_id: string | null;
  correction_reason: string | null;
  is_voided:         boolean;
  recorded_at:       string;
}

export interface Definition {
  id:            string;
  experiment_id: string;
  key:           string;
  type:          'constant' | 'expression' | 'step' | 'csv_schema' | 'variable';
  label:         string;
  payload:       Record<string, unknown>;
  sort_order:    number;
  var_type:      'numeric' | 'vector_csv' | 'text' | 'qualitative' | null;
  options:       string[] | null;
  created_at:    string;
}

export interface ExperimentColumn {
  key:     string;
  type:    'variable' | 'expression' | 'constant' | 'system';
  order:   number;
  visible: boolean;
  width?:  number;
}

export interface EntryValue {
  id:             string;
  entry_id:       string;
  definition_key: string;
  value_numeric:  number | null;
  value_text:     string | null;
  value_csv_data: unknown[] | null;
}

export interface Objective {
  id:             string;
  experiment_id:  string;
  name:           string;
  condition_type: 'range' | 'expression';
  condition:      Record<string, unknown>;
  severity:       'info' | 'warning' | 'critical';
  goto_ok:        string | null;
  goto_violation: string | null;
  sort_order:     number;
}

export interface Collaborator {
  user_id:      string;
  email:        string;
  display_name: string;
  role:         'viewer' | 'editor' | 'admin';
  added_at:     string;
}

export interface ObjectiveEvaluation {
  objective:  Objective;
  status:     'ok' | 'warning' | 'violation' | 'unknown';
  value:      number | null;
  goto:       string | null;
}

interface ExperimentState {
  experiment:    Experiment | null;
  events:        ExperimentEvent[];
  definitions:   Definition[];
  objectives:    Objective[];
  collaborators: Collaborator[];
  entryValues:   Record<string, Record<string, unknown>>;  // entry_id → {key → value}
  loading:       boolean;
  error:         string;
  lastActivity:  number;
}

// ── Store ─────────────────────────────────────────────────────────────────────

function createExperimentStore() {
  const { subscribe, set, update } = writable<ExperimentState>({
    experiment:    null,
    events:        [],
    definitions:   [],
    objectives:    [],
    collaborators: [],
    entryValues:   {},
    loading:       false,
    error:         '',
    lastActivity:  0,
  });

  async function fetchJson(path: string) {
    const res = await fetch(`${API}${path}`, { credentials: 'include' });
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    return res.json();
  }

  return {
    subscribe,

    // ── Cargar todo el experimento ───────────────────────────────────────────
    async load(id: string) {
      update(s => ({ ...s, loading: true, error: '' }));
      try {
        const [experiment, events, definitions, objectives, collaborators, entryValues] =
          await Promise.all([
            fetchJson(`/api/v1/experiments/${id}`),
            fetchJson(`/api/v1/experiments/${id}/events`),
            fetchJson(`/api/v1/experiments/${id}/definitions`),
            fetchJson(`/api/v1/experiments/${id}/objectives`),
            fetchJson(`/api/v1/experiments/${id}/collaborators`).catch(() => []),
            fetchJson(`/api/v1/experiments/${id}/values`).catch(() => ({})),
          ]);

        // Calcular user_role si no viene del experimento
                let user_role = experiment.user_role ?? null;

        update(s => ({
          ...s,
          experiment: { ...experiment, user_role },
          events,
          definitions,
          objectives,
          collaborators,
          loading: false,
          lastActivity: events.length > 0
            ? new Date(events[events.length - 1].recorded_at).getTime()
            : 0,
        }));
      } catch (e: any) {
        update(s => ({ ...s, loading: false, error: e.message ?? 'Error cargando experimento' }));
      }
    },

    // ── Recargar solo los eventos (polling para conflictos) ──────────────────
    async reloadEvents(id: string) {
      try {
        const events: ExperimentEvent[] = await fetchJson(`/api/v1/experiments/${id}/events`);
        update(s => {
          const newLast = events.length > 0
            ? new Date(events[events.length - 1].recorded_at).getTime()
            : 0;
          const hasNewActivity = newLast > s.lastActivity && s.lastActivity > 0;
          return { ...s, events, lastActivity: newLast,
            // Señal de conflicto: hay actividad nueva mientras el usuario está activo
            _hasConflict: hasNewActivity } as any;
        });
      } catch { /* silencioso */ }
    },

    // ── Agregar evento optimistamente ─────────────────────────────────────────
    addEvent(event: ExperimentEvent) {
      update(s => ({
        ...s,
        events: [...s.events, event],
        lastActivity: new Date(event.recorded_at).getTime(),
      }));
    },

    // ── Marcar evento como anulado y agregar corrección ───────────────────────
    applyCorrection(voidedId: string, newEvent: ExperimentEvent) {
      update(s => ({
        ...s,
        events: s.events
          .map(e => e.id === voidedId ? { ...e, is_voided: true } : e)
          .concat(newEvent),
      }));
    },

    // ── CRUD definitions ──────────────────────────────────────────────────────
    addDefinition(def: Definition) {
      update(s => ({ ...s, definitions: [...s.definitions, def] }));
    },
    updateDefinition(def: Definition) {
      update(s => ({
        ...s,
        definitions: s.definitions.map(d => d.id === def.id ? def : d),
      }));
    },
    removeDefinition(id: string) {
      update(s => ({ ...s, definitions: s.definitions.filter(d => d.id !== id) }));
    },

    // ── CRUD objectives ───────────────────────────────────────────────────────
    addObjective(obj: Objective) {
      update(s => ({ ...s, objectives: [...s.objectives, obj] }));
    },
    removeObjective(id: string) {
      update(s => ({ ...s, objectives: s.objectives.filter(o => o.id !== id) }));
    },

    // ── Colaboradores ─────────────────────────────────────────────────────────
    addCollaborator(c: Collaborator) {
      update(s => ({ ...s, collaborators: [...s.collaborators, c] }));
    },
    removeCollaborator(userId: string) {
      update(s => ({
        ...s,
        collaborators: s.collaborators.filter(c => c.user_id !== userId),
      }));
    },

    updateColumns(columns: ExperimentColumn[]) {
      update(s => ({
        ...s,
        experiment: s.experiment ? { ...s.experiment, columns } : null,
      }));
    },

    setEntryValues(entryId: string, values: Record<string, unknown>) {
      update(s => ({
        ...s,
        entryValues: { ...s.entryValues, [entryId]: values },
      }));
    },

    reset() {
      set({ experiment: null, events: [], definitions: [], objectives: [],
            collaborators: [], entryValues: {}, loading: false, error: '', lastActivity: 0 });
    },
  };
}

export const experimentStore = createExperimentStore();

// ── Derived ───────────────────────────────────────────────────────────────────

// Solo entries activas (no anuladas) ordenadas por timestamp
export const activeEvents = derived(experimentStore, $s =>
  $s.events
    .filter(e => !e.is_voided)
    .sort((a, b) => new Date(a.recorded_at).getTime() - new Date(b.recorded_at).getTime())
);

// Entries anuladas con su corrección agrupadas para mostrar en tabla
export const eventsWithCorrections = derived(experimentStore, $s => {
  const voided = new Set($s.events.filter(e => e.is_voided).map(e => e.id));
  const corrections = new Map(
    $s.events
      .filter(e => e.corrects_event_id)
      .map(e => [e.corrects_event_id!, e])
  );
  return $s.events
    .sort((a, b) => new Date(a.recorded_at).getTime() - new Date(b.recorded_at).getTime())
    .map(e => ({
      event:      e,
      correction: corrections.get(e.id) ?? null,
    }))
    .filter(({ event }) => !event.corrects_event_id); // no mostrar las correcciones como filas independientes
});

// Constantes del experimento como objeto plano para los scripts
export const constantsMap = derived(experimentStore, $s => {
  const base = ($s.experiment?.constants ?? {}) as Record<string, unknown>;
  const fromDefs = $s.definitions
    .filter(d => d.type === 'constant')
    .reduce((acc, d) => {
      acc[d.key] = (d.payload as any).value;
      return acc;
    }, {} as Record<string, unknown>);
  return { ...base, ...fromDefs };
});

// Rol del usuario actual
export const userRole = derived(experimentStore, $s =>
  $s.experiment?.user_role ?? null
);

export const canEdit = derived(userRole, r => r === 'editor' || r === 'admin');
export const canAdmin = derived(userRole, r => r === 'admin');

// ── Evaluador de expresiones genérico ────────────────────────────────────────
//
// Resuelve el grafo:  constantes → expresiones → variables de entries
// Las expresiones usan sintaxis simple: operadores +,-,*,/,**,() y referencias
// a otras keys por nombre. Evaluación en orden topológico.
//
// NO ejecuta Rhai — eso es trabajo del backend. Esto es solo para mostrar
// valores calculados en tiempo real en el frontend sin roundtrip al servidor.

function buildContext(
  experiment: Experiment | null,
  defs: Definition[],
  events: ExperimentEvent[],
): Record<string, number> {
  const ctx: Record<string, number> = {};

  // 1. Constantes del experimento (constants JSONB)
  if (experiment?.constants) {
    for (const [k, v] of Object.entries(experiment.constants)) {
      if (typeof v === 'number') ctx[k] = v;
    }
  }

  // 2. Constantes definidas en definitions
  for (const d of defs.filter(d => d.type === 'constant')) {
    const val = (d.payload as any).value;
    if (typeof val === 'number') ctx[d.key] = val;
  }

  // 3. Variables de entries — último valor de cada step_key y cada output del script
  for (const ev of events) {
    if (ev.is_voided) continue;
    const data = ev.data as Record<string, unknown>;
    // valor principal del step
    if (typeof data.value === 'number') ctx[ev.step_key] = data.value;
    // outputs nombrados del script (guardados en data por el backend)
    for (const [k, v] of Object.entries(data)) {
      if (typeof v === 'number' && k !== 'value') ctx[k] = v;
    }
  }

  // 4. Expresiones — evaluadas en orden, permitiendo referencias a anteriores
  //    Máximo 10 pasadas para resolver dependencias en cadena
  const exprs = defs.filter(d => d.type === 'expression');
  for (let pass = 0; pass < 10; pass++) {
    let resolved = 0;
    for (const d of exprs) {
      if (d.key in ctx) continue; // ya resuelta
      const formula = (d.payload as any).formula as string | undefined;
      if (!formula) continue;
      try {
        const result = evalFormula(formula, ctx);
        if (result !== null) { ctx[d.key] = result; resolved++; }
      } catch { /* dependencia aún no disponible */ }
    }
    if (resolved === 0) break; // nada nuevo resuelto
  }

  return ctx;
}

// Evaluador de fórmulas seguro — solo operadores matemáticos y referencias
// No usa eval() — parsea manualmente las referencias y delega a Function
function evalFormula(formula: string, ctx: Record<string, number>): number | null {
  // Reemplazar referencias (identificadores) por sus valores del contexto
  // Orden: más largo primero para evitar reemplazos parciales
  const keys = Object.keys(ctx).sort((a, b) => b.length - a.length);
  let expr = formula;
  for (const k of keys) {
    // Reemplazar solo palabras completas (no substrings)
    expr = expr.replace(new RegExp(`\b${k}\b`, 'g'), String(ctx[k]));
  }
  // Verificar que solo queden números y operadores seguros
  if (!/^[\d\s\+\-\*\/\.\(\)\^%]+$/.test(expr)) return null;
  // Reemplazar ^ por ** para potencias
  expr = expr.replace(/\^/g, '**');
  try {
    // eslint-disable-next-line no-new-func
    const result = new Function(`return (${expr})`)();
    return typeof result === 'number' && isFinite(result) ? result : null;
  } catch {
    return null;
  }
}

// ── Contexto completo del experimento ─────────────────────────────────────────
// Todas las constantes + expresiones resueltas + variables de entries

export const experimentContext = derived(
  [experimentStore, activeEvents],
  ([$s, $events]) => buildContext($s.experiment, $s.definitions, $events)
);

// ── Evaluación de objetivos — completamente genérica ─────────────────────────

export const objectiveEvaluations = derived(
  [experimentStore, experimentContext],
  ([$s, ctx]) => {
    return $s.objectives.map(obj => {
      const cond = obj.condition as Record<string, unknown>;
      let status: 'ok' | 'warning' | 'violation' | 'unknown' = 'unknown';
      let value: number | null = null;

      if (obj.condition_type === 'range') {
        const varName = cond.variable as string;
        value = ctx[varName] ?? null;
        if (value !== null) {
          const min = (cond.min as number) ?? -Infinity;
          const max = (cond.max as number) ?? Infinity;
          if (value < min || value > max) {
            status = 'violation';
          } else {
            const range = max - min;
            const nearEdge = range > 0 &&
              (value < min + range * 0.1 || value > max - range * 0.1);
            status = nearEdge ? 'warning' : 'ok';
          }
        }
      } else if (obj.condition_type === 'expression') {
        // La expresión se evalúa igual que las demás usando el contexto
        const formula = cond.expr as string | undefined;
        if (formula) {
          const result = evalFormula(formula, ctx);
          if (result !== null) {
            // Convenio: resultado > 0 = ok, <= 0 = violation
            status = result > 0 ? 'ok' : 'violation';
            value = result;
          }
        }
      }

      return {
        objective: obj,
        status,
        value,
        goto: status === 'ok' ? obj.goto_ok : obj.goto_violation,
      } as ObjectiveEvaluation;
    });
  }
);