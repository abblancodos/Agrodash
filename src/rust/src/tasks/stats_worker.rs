// src/tasks/stats_worker.rs
//
// Tarea tokio que corre en background cada INTERVAL_SECS segundos.
// Calcula estadísticas por sensor y correlaciones entre pares,
// y las guarda en sensor_stats y sensor_correlations.
//
// Diseño anti-hammering:
//   - Solo hace queries cada INTERVAL_SECS (300s = 5 min por defecto).
//   - Las queries usan ventanas de tiempo acotadas (24h, 1h).
//   - Usa UPSERT para no acumular filas.
//   - Si la tarea tarda más de INTERVAL_SECS, la siguiente espera igual.

use chrono::Utc;
use sqlx::PgPool;
use tracing::{error, info, warn};

const INTERVAL_SECS: u64 = 300;   // 5 minutos
const CR_OFFSET_SECS: i64 = 6 * 3600;
const CORR_THRESHOLD: f64 = 0.85; // |r| mínimo para guardar correlación
const MIN_POINTS_CORR: usize = 10; // puntos mínimos para calcular Pearson

pub async fn run(pool: PgPool) {
    info!("stats_worker: iniciando, intervalo = {}s", INTERVAL_SECS);
    loop {
        if let Err(e) = compute_and_store(&pool).await {
            error!("stats_worker: error en ciclo: {}", e);
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(INTERVAL_SECS)).await;
    }
}

async fn compute_and_store(pool: &PgPool) -> Result<(), sqlx::Error> {
    let started = std::time::Instant::now();
    info!("stats_worker: calculando estadísticas...");

    // ── 1. Obtener todos los sensores con su box_id ───────────────────────────
    let sensors = sqlx::query!(
        r#"
        SELECT s.id AS sensor_id, s.box_id, s.type AS sensor_type
        FROM sensors s
        "#
    )
    .fetch_all(pool)
    .await?;

    let now_cr = Utc::now().naive_utc()
        - chrono::Duration::seconds(CR_OFFSET_SECS);
    let window_24h = now_cr - chrono::Duration::hours(24);
    let window_1h  = now_cr - chrono::Duration::hours(1);

    // ── 2. Calcular stats por sensor y hacer UPSERT ──────────────────────────
    for sensor in &sensors {
        // Stats de ventana 24h + último valor
        let stats = sqlx::query!(
            r#"
            SELECT
                AVG(value)::float8          AS "mean_24h: f64",
                STDDEV(value)::float8       AS "stddev_24h: f64",
                MIN(value)::float8          AS "min_24h: f64",
                MAX(value)::float8          AS "max_24h: f64",
                COUNT(*)::int4              AS "count_24h: i32"
            FROM readings
            WHERE sensor_id = $1
              AND created_at >= $2
              AND created_at <= $3
              AND created_at BETWEEN '2020-01-01' AND NOW()
            "#,
            sensor.sensor_id,
            window_24h,
            now_cr,
        )
        .fetch_optional(pool)
        .await?;

        // Último valor registrado
        let last = sqlx::query!(
            r#"
            SELECT
                value::float8   AS "value: f64",
                created_at      AS "created_at: chrono::NaiveDateTime"
            FROM readings
            WHERE sensor_id = $1
              AND created_at BETWEEN '2020-01-01' AND NOW()
            ORDER BY created_at DESC
            LIMIT 1
            "#,
            sensor.sensor_id,
        )
        .fetch_optional(pool)
        .await?;

        // Valor hace ~1h (para rate of change)
        let prev_1h = sqlx::query!(
            r#"
            SELECT value::float8 AS "value: f64"
            FROM readings
            WHERE sensor_id = $1
              AND created_at >= $2
              AND created_at <= $3
              AND created_at BETWEEN '2020-01-01' AND NOW()
            ORDER BY ABS(EXTRACT(EPOCH FROM (created_at - $2)))
            LIMIT 1
            "#,
            sensor.sensor_id,
            window_1h,
            now_cr,
        )
        .fetch_optional(pool)
        .await?;

        let last_value    = last.as_ref().map(|r| r.value);
        // Convertir timestamp CR → UTC para guardar
        let last_seen_utc = last.as_ref().map(|r| {
            chrono::DateTime::<Utc>::from_naive_utc_and_offset(
                r.created_at + chrono::Duration::seconds(CR_OFFSET_SECS),
                Utc,
            )
        });

        let (mean_24h, stddev_24h, min_24h, max_24h, count_24h) = match &stats {
            Some(s) => (s.mean_24h, s.stddev_24h, s.min_24h, s.max_24h, s.count_24h),
            None    => (None, None, None, None, None),
        };

        // Anomaly score: |last - mean| / stddev, solo si count >= 5
        let anomaly_score = match (last_value, mean_24h, stddev_24h, count_24h) {
            (Some(lv), Some(m), Some(sd), Some(c)) if c >= 5 && sd > 1e-10 => {
                Some((lv - m).abs() / sd)
            }
            _ => None,
        };

        // Rate of change: (last - prev_1h) / 1.0 hora
        let rate_of_change = match (last_value, prev_1h.as_ref().map(|r| r.value)) {
            (Some(lv), Some(pv)) => Some(lv - pv),
            _ => None,
        };

        sqlx::query!(
            r#"
            INSERT INTO sensor_stats (
                sensor_id, computed_at,
                last_value, last_seen_at,
                mean_24h, stddev_24h, min_24h, max_24h, count_24h,
                anomaly_score, rate_of_change
            ) VALUES ($1, NOW(), $2, $3, $4, $5, $6, $7, $8, $9, $10)
            ON CONFLICT (sensor_id) DO UPDATE SET
                computed_at     = EXCLUDED.computed_at,
                last_value      = EXCLUDED.last_value,
                last_seen_at    = EXCLUDED.last_seen_at,
                mean_24h        = EXCLUDED.mean_24h,
                stddev_24h      = EXCLUDED.stddev_24h,
                min_24h         = EXCLUDED.min_24h,
                max_24h         = EXCLUDED.max_24h,
                count_24h       = EXCLUDED.count_24h,
                anomaly_score   = EXCLUDED.anomaly_score,
                rate_of_change  = EXCLUDED.rate_of_change
            "#,
            sensor.sensor_id,
            last_value,
            last_seen_utc,
            mean_24h,
            stddev_24h,
            min_24h,
            max_24h,
            count_24h,
            anomaly_score,
            rate_of_change,
        )
        .execute(pool)
        .await?;
    }

    // ── 3. Correlaciones por (caja, tipo de sensor) ──────────────────────────
    // Agrupar sensores por (box_id, sensor_type)
    let mut groups: std::collections::HashMap<(uuid::Uuid, String), Vec<uuid::Uuid>> =
        std::collections::HashMap::new();

    for s in &sensors {
        groups
            .entry((s.box_id, s.sensor_type.to_lowercase()))
            .or_default()
            .push(s.sensor_id);
    }

    for ((box_id, sensor_type), ids) in &groups {
        if ids.len() < 2 {
            continue; // necesitamos al menos un par
        }

        // Cargar ventana de datos para todos los sensores del grupo
        // Usamos una query por sensor para simplicidad (son pocos por grupo)
        let mut series: Vec<(uuid::Uuid, Vec<f64>)> = Vec::new();
        for &sid in ids {
            let vals = sqlx::query!(
                r#"
                SELECT AVG(value)::float8 AS "v: f64"
                FROM readings
                WHERE sensor_id = $1
                  AND created_at >= $2
                  AND created_at <= $3
                  AND created_at BETWEEN '2020-01-01' AND NOW()
                GROUP BY date_trunc('hour', created_at)
                ORDER BY 1
                "#,
                sid,
                window_24h,
                now_cr,
            )
            .fetch_all(pool)
            .await?
            .into_iter()
            .filter_map(|r| r.v)
            .collect::<Vec<f64>>();

            if vals.len() >= MIN_POINTS_CORR {
                series.push((sid, vals));
            }
        }

        // Calcular Pearson para cada par
        for i in 0..series.len() {
            for j in (i + 1)..series.len() {
                let (sid_a, va) = &series[i];
                let (sid_b, vb) = &series[j];

                // Alinear longitudes (tomar mínimo)
                let n = va.len().min(vb.len());
                if n < MIN_POINTS_CORR { continue; }

                let r = pearson(&va[..n], &vb[..n]);

                if r.abs() < CORR_THRESHOLD { continue; }

                // Garantizar orden a < b para el CHECK constraint
                let (a, b) = if sid_a < sid_b { (sid_a, sid_b) } else { (sid_b, sid_a) };

                sqlx::query!(
                    r#"
                    INSERT INTO sensor_correlations
                        (box_id, sensor_type, sensor_id_a, sensor_id_b, pearson_r, computed_at)
                    VALUES ($1, $2, $3, $4, $5, NOW())
                    ON CONFLICT (sensor_id_a, sensor_id_b, sensor_type) DO UPDATE SET
                        pearson_r    = EXCLUDED.pearson_r,
                        computed_at  = EXCLUDED.computed_at
                    "#,
                    *box_id,
                    sensor_type,
                    *a,
                    *b,
                    r,
                )
                .execute(pool)
                .await?;
            }
        }
    }

    let elapsed = started.elapsed();
    if elapsed.as_secs() > INTERVAL_SECS / 2 {
        warn!(
            "stats_worker: ciclo tardó {}ms — considera aumentar INTERVAL_SECS",
            elapsed.as_millis()
        );
    } else {
        info!("stats_worker: ciclo completado en {}ms", elapsed.as_millis());
    }

    Ok(())
}

/// Coeficiente de correlación de Pearson entre dos slices del mismo largo.
fn pearson(a: &[f64], b: &[f64]) -> f64 {
    let n = a.len() as f64;
    let mean_a = a.iter().sum::<f64>() / n;
    let mean_b = b.iter().sum::<f64>() / n;

    let num: f64 = a.iter().zip(b.iter())
        .map(|(x, y)| (x - mean_a) * (y - mean_b))
        .sum();

    let den_a: f64 = a.iter().map(|x| (x - mean_a).powi(2)).sum::<f64>().sqrt();
    let den_b: f64 = b.iter().map(|y| (y - mean_b).powi(2)).sum::<f64>().sqrt();

    let den = den_a * den_b;
    if den < 1e-10 { return 0.0; }
    (num / den).clamp(-1.0, 1.0)
}