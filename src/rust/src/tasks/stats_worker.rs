// src/tasks/stats_worker.rs

use chrono::Utc;
use sqlx::PgPool;
use tracing::{error, info, warn};

const INTERVAL_SECS: u64 = 300;
const CR_OFFSET_SECS: i64 = 6 * 3600;
const CORR_THRESHOLD: f64 = 0.85;
const MIN_POINTS_CORR: usize = 10;

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

    // ── 1. Sensores ───────────────────────────────────────────────────────────
    // Usamos query! y mapeamos manualmente para evitar que sqlx infiera
    // id/box_id como Option<Uuid> por el esquema de la tabla.
    let sensor_rows = sqlx::query!(
        r#"SELECT id AS "id!: uuid::Uuid", box_id AS "box_id!: uuid::Uuid", type AS "sensor_type!" FROM sensors"#
    )
    .fetch_all(pool)
    .await?;

    // Convertir a Vec de tuplas simples — tipos garantizados no-Option
    let sensors: Vec<(uuid::Uuid, uuid::Uuid, String)> = sensor_rows
        .into_iter()
        .map(|r| (r.id, r.box_id, r.sensor_type))
        .collect();

    let now_cr     = Utc::now().naive_utc() - chrono::Duration::seconds(CR_OFFSET_SECS);
    let window_24h = now_cr - chrono::Duration::hours(24);
    let window_1h  = now_cr - chrono::Duration::hours(1);

    // ── 2. Stats por sensor ───────────────────────────────────────────────────
    for (sensor_id, box_id, sensor_type) in &sensors {
        let stats = sqlx::query!(
            r#"
            SELECT
                AVG(value)::float8    AS "mean_24h: f64",
                STDDEV(value)::float8 AS "stddev_24h: f64",
                MIN(value)::float8    AS "min_24h: f64",
                MAX(value)::float8    AS "max_24h: f64",
                COUNT(*)::int4        AS "count_24h: i32"
            FROM readings
            WHERE sensor_id = $1
              AND created_at >= $2
              AND created_at <= $3
              AND created_at BETWEEN '2020-01-01' AND NOW()
            "#,
            sensor_id,
            window_24h,
            now_cr,
        )
        .fetch_one(pool)  // COUNT(*) nunca devuelve 0 filas, fetch_one es seguro
        .await?;

        let last = sqlx::query!(
            r#"
            SELECT
                value::float8 AS "value!: f64",
                created_at    AS "created_at!: chrono::NaiveDateTime"
            FROM readings
            WHERE sensor_id = $1
              AND created_at BETWEEN '2020-01-01' AND NOW()
            ORDER BY created_at DESC
            LIMIT 1
            "#,
            sensor_id,
        )
        .fetch_optional(pool)
        .await?;

        let prev_1h = sqlx::query!(
            r#"
            SELECT value::float8 AS "value!: f64"
            FROM readings
            WHERE sensor_id = $1
              AND created_at >= $2
              AND created_at <= $3
              AND created_at BETWEEN '2020-01-01' AND NOW()
            ORDER BY ABS(EXTRACT(EPOCH FROM (created_at - $2)))
            LIMIT 1
            "#,
            sensor_id,
            window_1h,
            now_cr,
        )
        .fetch_optional(pool)
        .await?;

        // Con "value!: f64" le decimos a sqlx que el campo es NOT NULL,
        // así r.value es f64, no Option<f64>
        let last_value: Option<f64>    = last.as_ref().map(|r| r.value);
        let prev_value: Option<f64>    = prev_1h.as_ref().map(|r| r.value);
        let last_seen_utc = last.as_ref().map(|r| {
            chrono::DateTime::<Utc>::from_naive_utc_and_offset(
                r.created_at + chrono::Duration::seconds(CR_OFFSET_SECS),
                Utc,
            )
        });

        let mean_24h:   Option<f64> = stats.mean_24h;
        let stddev_24h: Option<f64> = stats.stddev_24h;
        let min_24h:    Option<f64> = stats.min_24h;
        let max_24h:    Option<f64> = stats.max_24h;
        let count_24h:  Option<i32> = stats.count_24h;

        let anomaly_score: Option<f64> =
            match (last_value, mean_24h, stddev_24h, count_24h) {
                (Some(lv), Some(m), Some(sd), Some(c)) if c >= 5 && sd > 1e-10 => {
                    Some((lv - m).abs() / sd)
                }
                _ => None,
            };

        let rate_of_change: Option<f64> = match (last_value, prev_value) {
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
                computed_at    = EXCLUDED.computed_at,
                last_value     = EXCLUDED.last_value,
                last_seen_at   = EXCLUDED.last_seen_at,
                mean_24h       = EXCLUDED.mean_24h,
                stddev_24h     = EXCLUDED.stddev_24h,
                min_24h        = EXCLUDED.min_24h,
                max_24h        = EXCLUDED.max_24h,
                count_24h      = EXCLUDED.count_24h,
                anomaly_score  = EXCLUDED.anomaly_score,
                rate_of_change = EXCLUDED.rate_of_change
            "#,
            sensor_id,
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

    // ── 3. Correlaciones ──────────────────────────────────────────────────────
    let mut groups: std::collections::HashMap<(uuid::Uuid, String), Vec<uuid::Uuid>> =
        std::collections::HashMap::new();

    for (sensor_id, box_id, sensor_type) in &sensors {
        groups
            .entry((*box_id, sensor_type.to_lowercase()))
            .or_default()
            .push(*sensor_id);
    }

    for ((box_id, sensor_type), ids) in &groups {
        if ids.len() < 2 { continue; }

        let mut series: Vec<(uuid::Uuid, Vec<f64>)> = Vec::new();
        for &sid in ids {
            let vals = sqlx::query!(
                r#"
                SELECT AVG(value)::float8 AS "v!: f64"
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
            .map(|r| r.v)
            .collect::<Vec<f64>>();

            if vals.len() >= MIN_POINTS_CORR {
                series.push((sid, vals));
            }
        }

        for i in 0..series.len() {
            for j in (i + 1)..series.len() {
                let (sid_a, va) = &series[i];
                let (sid_b, vb) = &series[j];

                let n = va.len().min(vb.len());
                if n < MIN_POINTS_CORR { continue; }

                let r = pearson(&va[..n], &vb[..n]);
                if r.abs() < CORR_THRESHOLD { continue; }

                let (a, b) = if sid_a < sid_b { (sid_a, sid_b) } else { (sid_b, sid_a) };

                sqlx::query!(
                    r#"
                    INSERT INTO sensor_correlations
                        (box_id, sensor_type, sensor_id_a, sensor_id_b, pearson_r, computed_at)
                    VALUES ($1, $2, $3, $4, $5, NOW())
                    ON CONFLICT (sensor_id_a, sensor_id_b, sensor_type) DO UPDATE SET
                        pearson_r   = EXCLUDED.pearson_r,
                        computed_at = EXCLUDED.computed_at
                    "#,
                    box_id,
                    sensor_type,
                    a,
                    b,
                    r,
                )
                .execute(pool)
                .await?;
            }
        }
    }

    let elapsed = started.elapsed();
    if elapsed.as_secs() > INTERVAL_SECS / 2 {
        warn!("stats_worker: ciclo tardó {}ms", elapsed.as_millis());
    } else {
        info!("stats_worker: ciclo completado en {}ms", elapsed.as_millis());
    }

    Ok(())
}

fn pearson(a: &[f64], b: &[f64]) -> f64 {
    let n = a.len() as f64;
    let mean_a = a.iter().sum::<f64>() / n;
    let mean_b = b.iter().sum::<f64>() / n;
    let num: f64 = a.iter().zip(b.iter()).map(|(x, y)| (x - mean_a) * (y - mean_b)).sum();
    let den_a: f64 = a.iter().map(|x| (x - mean_a).powi(2)).sum::<f64>().sqrt();
    let den_b: f64 = b.iter().map(|y| (y - mean_b).powi(2)).sum::<f64>().sqrt();
    let den = den_a * den_b;
    if den < 1e-10 { return 0.0; }
    (num / den).clamp(-1.0, 1.0)
}