-- migrations/008_experiment_features.sql
--
-- Agrega:
--   1. experiment_collaborators — permisos por experimento
--   2. Columna corrects_event_id en experiment_events — audit de correcciones
--   3. experiment_definitions — constantes, expresiones, objetivos, pasos como items
--   4. experiment_objectives — objetivos con condición y goto

-- ── 1. Colaboradores ──────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS experiment_collaborators (
    experiment_id UUID        NOT NULL REFERENCES experiments(id) ON DELETE CASCADE,
    user_id       UUID        NOT NULL REFERENCES users(id)       ON DELETE CASCADE,
    role          TEXT        NOT NULL DEFAULT 'viewer'
                  CHECK (role IN ('viewer', 'editor', 'admin')),
    added_by      UUID        REFERENCES users(id),
    added_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (experiment_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_collab_experiment ON experiment_collaborators (experiment_id);
CREATE INDEX IF NOT EXISTS idx_collab_user       ON experiment_collaborators (user_id);

-- ── 2. Correcciones en experiment_events ─────────────────────────────────────
--
-- corrects_event_id apunta al event_id que esta entry anula.
-- correction_reason es obligatorio si corrects_event_id está presente.
-- recorded_by guarda el user_id del autor.

ALTER TABLE experiment_events
    ADD COLUMN IF NOT EXISTS corrects_event_id UUID REFERENCES experiment_events(id),
    ADD COLUMN IF NOT EXISTS correction_reason  TEXT,
    ADD COLUMN IF NOT EXISTS recorded_by        UUID REFERENCES users(id),
    ADD COLUMN IF NOT EXISTS is_voided          BOOLEAN NOT NULL DEFAULT false;

CREATE INDEX IF NOT EXISTS idx_events_corrects ON experiment_events (corrects_event_id)
    WHERE corrects_event_id IS NOT NULL;

-- ── 3. Definitions — constantes, expresiones, pasos ──────────────────────────
--
-- type: 'constant' | 'expression' | 'step' | 'csv_schema'
-- payload JSONB varía por tipo:
--
--   constant:   { value: f64, unit: str, comment: str }
--   expression: { formula: str, unit: str, comment: str }
--   step:       { label: str, fields: [...], script: str, next: [...] }
--   csv_schema: { columns: [{key, label, unit, type}] }

CREATE TABLE IF NOT EXISTS experiment_definitions (
    id            UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    experiment_id UUID        NOT NULL REFERENCES experiments(id) ON DELETE CASCADE,
    key           TEXT        NOT NULL,   -- identificador interno único por experimento
    type          TEXT        NOT NULL
                  CHECK (type IN ('constant', 'expression', 'step', 'csv_schema')),
    label         TEXT        NOT NULL,
    payload       JSONB       NOT NULL DEFAULT '{}',
    sort_order    INTEGER     NOT NULL DEFAULT 0,
    created_by    UUID        REFERENCES users(id),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (experiment_id, key)
);

CREATE INDEX IF NOT EXISTS idx_defs_experiment ON experiment_definitions (experiment_id);
CREATE INDEX IF NOT EXISTS idx_defs_type       ON experiment_definitions (experiment_id, type);

-- ── 4. Objectives ─────────────────────────────────────────────────────────────
--
-- condition_type: 'range' | 'expression'
--
-- range:      { variable: str, min: f64|null, max: f64|null, unit: str }
-- expression: { expr: str }   — evaluada como Rhai, debe devolver bool
--
-- severity: 'info' | 'warning' | 'critical'
-- on_ok   / on_violation: step key para goto (opcional)

CREATE TABLE IF NOT EXISTS experiment_objectives (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    experiment_id   UUID        NOT NULL REFERENCES experiments(id) ON DELETE CASCADE,
    name            TEXT        NOT NULL,
    condition_type  TEXT        NOT NULL CHECK (condition_type IN ('range', 'expression')),
    condition       JSONB       NOT NULL,
    severity        TEXT        NOT NULL DEFAULT 'warning'
                    CHECK (severity IN ('info', 'warning', 'critical')),
    goto_ok         TEXT,       -- step key si se cumple
    goto_violation  TEXT,       -- step key si se viola
    sort_order      INTEGER     NOT NULL DEFAULT 0,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_objectives_experiment ON experiment_objectives (experiment_id);
