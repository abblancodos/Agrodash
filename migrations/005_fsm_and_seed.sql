-- migrations/005_fsm_and_seed.sql
--
-- 1. Agrega must_change_pw a users (inferido del hash pero útil tener explícito)
-- 2. Crea el primer usuario admin desde seed
-- 3. Documenta el schema FSM de steps

-- must_change_pw: true si el usuario nunca cambió la contraseña temporal
ALTER TABLE users ADD COLUMN IF NOT EXISTS
    must_change_pw BOOLEAN NOT NULL DEFAULT true;

-- Script de seed para crear el primer admin
-- Ejecutar separado con: psql $DATABASE_URL -c "..."
-- La contraseña "Estacion2" hasheada con bcrypt cost=12 se genera al correr
-- el binario de Rust con el subcomando seed, o manualmente:
--
--   curl -X POST http://localhost:3000/api/v1/admin/seed \
--     -H "Content-Type: application/json" \
--     -d '{"secret": "<SEED_SECRET>", "email": "admin@lab.tec.ac.cr", "display_name": "Admin"}'
--
-- SEED_SECRET se define en .env y es solo para el primer arranque.

-- Índice para búsqueda por must_change_pw
CREATE INDEX IF NOT EXISTS idx_users_must_change ON users (must_change_pw) WHERE must_change_pw = true;

-- ── Comentario de referencia: schema FSM de steps ─────────────────────────────
--
-- Un step en experiment_templates.steps (JSONB array) tiene esta forma:
--
-- {
--   "key":   "medir_peso",          -- identificador único en la plantilla
--   "label": "Medir peso actual",   -- texto para el usuario
--   "type":  "measurement",         -- measurement | calculation | note | repeat | upload_csv | branch
--   "unit":  "g",                   -- para measurement
--
--   -- Para type = "calculation":
--   "formula": "masa_con_agua - masa_maceta - masa_sensor",
--                                   -- expresión evaluada contra valores del experimento
--                                   -- variables disponibles: steps.<key>, constants.<key>
--
--   -- Para type = "upload_csv":
--   "csv_schema": ["freq_hz", "mag_db", "phase_deg"],
--
--   -- Transiciones (FSM):
--   "next": [
--     {
--       "condition": "steps.theta_actual.value < constants.theta_objetivo - 0.02",
--       "goto": "calcular_agua",
--       "label": "Suelo necesita agua"
--     },
--     {
--       "condition": "true",         -- fallback / else
--       "goto": "registrar_sensor",
--       "label": "Suelo OK"
--     }
--   ]
-- }
--
-- Si "next" está vacío o ausente, el paso es terminal (fin del flujo).
-- Un "goto" puede apuntar a un paso anterior → loop natural.
-- El frontend evalúa las condiciones en orden y navega al primer match.
