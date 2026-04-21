-- migrations/004_auth_experiments.sql
--
-- Auth + sistema de experimentos con plantillas tipo "legos"
--
-- Estructura de plantillas:
--   experiment_templates define los pasos como JSONB
--   experiments son instancias de una plantilla
--   experiment_events son el registro cronológico (una fila por evento)
--   experiment_constants son los parámetros fijos de una instancia (masa seco, volumen, etc.)

-- ── Usuarios ──────────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS users (
    id            UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    email         TEXT        NOT NULL UNIQUE,
    display_name  TEXT        NOT NULL,
    password_hash TEXT        NOT NULL,
    role          TEXT        NOT NULL DEFAULT 'user'  -- 'user' | 'admin'
                  CHECK (role IN ('user', 'admin')),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_users_email ON users (email);

-- ── Plantillas de experimento ─────────────────────────────────────────────────
--
-- steps es un array JSONB de pasos, cada uno con:
-- {
--   "key":         "masa_maceta_vacia",   -- identificador interno
--   "label":       "Masa maceta vacía",   -- texto para el usuario
--   "type":        "measurement",         -- measurement | upload_csv | calculation | note | repeat
--   "unit":        "g",                   -- unidad (para measurement)
--   "formula":     "...",                 -- expresión para calculation (referencia a otros keys)
--   "csv_schema":  ["freq_hz","mag_db"],  -- columnas esperadas (para upload_csv)
--   "required":    true
-- }
--
-- constants_schema define los parámetros fijos de la instancia (con sus labels y unidades)

CREATE TABLE IF NOT EXISTS experiment_templates (
    id               UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    owner_id         UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name             TEXT        NOT NULL,
    description      TEXT,
    public           BOOLEAN     NOT NULL DEFAULT false,
    steps            JSONB       NOT NULL DEFAULT '[]',
    constants_schema JSONB       NOT NULL DEFAULT '[]',
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_templates_owner  ON experiment_templates (owner_id);
CREATE INDEX IF NOT EXISTS idx_templates_public ON experiment_templates (public) WHERE public = true;

-- ── Experimentos (instancias de plantilla) ─────────────────────────────────────

CREATE TABLE IF NOT EXISTS experiments (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    template_id UUID        NOT NULL REFERENCES experiment_templates(id),
    owner_id    UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title       TEXT        NOT NULL,
    description TEXT,
    public      BOOLEAN     NOT NULL DEFAULT false,
    -- Constantes de esta instancia (valores concretos para los campos de constants_schema)
    -- Ej: {"masa_maceta": 305, "masa_seco_aire": 8270, "v_suelo": 6059, ...}
    constants   JSONB       NOT NULL DEFAULT '{}',
    status      TEXT        NOT NULL DEFAULT 'active'
                CHECK (status IN ('active', 'completed', 'archived')),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_experiments_owner    ON experiments (owner_id);
CREATE INDEX IF NOT EXISTS idx_experiments_template ON experiments (template_id);
CREATE INDEX IF NOT EXISTS idx_experiments_public   ON experiments (public) WHERE public = true;

-- ── Registro cronológico de eventos ──────────────────────────────────────────
--
-- Una fila por evento. El campo data es flexible según el tipo de evento.
-- Para tipo "measurement": {"valor": 8270, "unidad": "g"}
-- Para tipo "sensor":      {"theta_cs655": 0.059, "temp": 25.1, "ec": 0.12}
-- Para tipo "irrigation":  {"agua_calculada": 365, "agua_real": 360, "duracion_s": 49}
-- Para tipo "note":        {"texto": "..."}
-- Para tipo "calculation": {"theta_grav": 0.10, "saturacion": 0.65, "agua_añadir": 290}

CREATE TABLE IF NOT EXISTS experiment_events (
    id            UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    experiment_id UUID        NOT NULL REFERENCES experiments(id) ON DELETE CASCADE,
    step_key      TEXT        NOT NULL,  -- referencia al key del paso en la plantilla
    event_type    TEXT        NOT NULL,  -- measurement | sensor | irrigation | note | calculation | upload_csv
    soil_id       TEXT,                  -- 'FA' | 'AR' | null (para experimentos con varios suelos)
    iteration     INTEGER,               -- número de iteración (para pasos repetibles)
    data          JSONB       NOT NULL DEFAULT '{}',
    note          TEXT,
    recorded_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_events_experiment ON experiment_events (experiment_id);
CREATE INDEX IF NOT EXISTS idx_events_step       ON experiment_events (experiment_id, step_key);
CREATE INDEX IF NOT EXISTS idx_events_time       ON experiment_events (experiment_id, recorded_at);

-- ── Series temporales (pesadas periódicas entre irrigaciones) ─────────────────
--
-- Para los monitoreos continuos — muchos puntos en el tiempo.
-- Separado de experiment_events por eficiencia.

CREATE TABLE IF NOT EXISTS experiment_series (
    id            UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    experiment_id UUID        NOT NULL REFERENCES experiments(id) ON DELETE CASCADE,
    series_key    TEXT        NOT NULL,  -- ej: "peso_continuo_FA"
    soil_id       TEXT,
    value         FLOAT8      NOT NULL,
    unit          TEXT,
    note          TEXT,
    recorded_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_series_experiment ON experiment_series (experiment_id, series_key);
CREATE INDEX IF NOT EXISTS idx_series_time       ON experiment_series (experiment_id, recorded_at);

-- ── Archivos CSV subidos ──────────────────────────────────────────────────────
--
-- parsed_data contiene el CSV parseado como array de objetos JSON.
-- No guardamos el archivo raw, solo los datos limpios.

CREATE TABLE IF NOT EXISTS experiment_files (
    id            UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    experiment_id UUID        NOT NULL REFERENCES experiments(id) ON DELETE CASCADE,
    step_key      TEXT        NOT NULL,
    filename      TEXT        NOT NULL,
    row_count     INTEGER,
    columns       JSONB,       -- ["freq_hz", "mag_db", "phase_deg"]
    parsed_data   JSONB,       -- [{freq_hz: 1.0, mag_db: -3.2}, ...]
    uploaded_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_files_experiment ON experiment_files (experiment_id);
