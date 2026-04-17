-- migrations/003_sensor_stats.sql
--
-- Tabla de estadísticas pre-calculadas por sensor.
-- La tarea tokio en background la actualiza cada 5 minutos.
-- El frontend lee de aquí — nunca de readings directamente para el dashboard.

CREATE TABLE IF NOT EXISTS sensor_stats (
    sensor_id       UUID        NOT NULL REFERENCES sensors(id) ON DELETE CASCADE,
    computed_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Último dato registrado
    last_value      FLOAT8,
    last_seen_at    TIMESTAMPTZ,           -- en UTC real

    -- Estadísticas sobre ventana de 24h
    mean_24h        FLOAT8,
    stddev_24h      FLOAT8,
    min_24h         FLOAT8,
    max_24h         FLOAT8,
    count_24h       INTEGER,

    -- Score de anomalía: |last_value - mean_24h| / stddev_24h
    -- NULL si stddev=0 o count<5
    anomaly_score   FLOAT8,

    -- Rate of change: (last_value - value hace 1h) / 1h
    -- NULL si no hay dato previo suficiente
    rate_of_change  FLOAT8,

    PRIMARY KEY (sensor_id)
);

-- Índice para ordenar por anomaly_score descendente (vista compacta)
CREATE INDEX IF NOT EXISTS idx_sensor_stats_anomaly
    ON sensor_stats (anomaly_score DESC NULLS LAST);

-- Tabla de correlaciones entre pares de sensores de la misma caja
-- para la misma variable (tipo). Se usa para agrupar al final de la card.
CREATE TABLE IF NOT EXISTS sensor_correlations (
    box_id          UUID        NOT NULL REFERENCES boxes(id) ON DELETE CASCADE,
    sensor_type     TEXT        NOT NULL,   -- tipo normalizado en minúsculas
    sensor_id_a     UUID        NOT NULL REFERENCES sensors(id) ON DELETE CASCADE,
    sensor_id_b     UUID        NOT NULL REFERENCES sensors(id) ON DELETE CASCADE,
    pearson_r       FLOAT8      NOT NULL,
    computed_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    PRIMARY KEY (sensor_id_a, sensor_id_b, sensor_type),
    CHECK (sensor_id_a < sensor_id_b)      -- evita duplicados (a,b) y (b,a)
);

CREATE INDEX IF NOT EXISTS idx_corr_box_type
    ON sensor_correlations (box_id, sensor_type);