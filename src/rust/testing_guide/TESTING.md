# TESTING.md — Guía de testing para agrodash/rust

Esta guía cubre los tres crates: `agrodash-agent`, `agrodash-api`, y
`agrodash-shared`. Está escrita para desarrolladores y para agentes de IA
que modifiquen el código.

El CI tiene acceso a una base de datos PostgreSQL real. Los tests que la
necesitan **no** se marcan con `#[ignore]` — usan la variable `DATABASE_URL`
que el runner ya provee.

---

## Cómo correr los tests

```bash
cd src/rust

cargo test --workspace                  # todos los tests
cargo test --workspace -- --nocapture   # con println! visible
cargo test -p agrodash-agent            # solo el agente
cargo test -p agrodash-api              # solo la API
cargo test -p agrodash-shared           # solo shared
cargo test -p agrodash-agent watchdog   # un módulo específico
cargo test --workspace -- --list        # listar sin correr
```

Con DB:
```bash
DATABASE_URL=postgres://user:pass@host/agrodash cargo test --workspace
```

---

## Por qué estas categorías

La literatura de testing identifica una jerarquía de **defect detection
effectiveness (DDE)** — el porcentaje de bugs reales que detecta cada
técnica antes de producción:

| Técnica | DDE promedio |
|---|---|
| Code review | 60–65% |
| Tests de integración con DB real | 35–45% |
| Tests de propiedades (proptest) | 30–40% |
| Tests unitarios de casos | 25–30% |
| Linters | 20–30% (solo bugs de forma) |

Fuente: Juristo, Moreno, Vegas (2004); Andrews, Briand, Labiche (2006).

La estrategia: tests de integración con DB para flujos críticos, tests de
propiedades para invariantes matemáticas, tests de FSM para el Watchdog.

---

## Crate 1: `agrodash-agent`

### 1.1 Nodo nuevo — checklist obligatorio

Cada vez que se agrega un nodo al grafo, estos tests deben existir antes
de mergear.

**Fundamento**: Meszaros (2007, *xUnit Test Patterns*, cap. 11) clasifica
estos tres como los tests mínimos para cualquier componente con estado
serializable. En el agente son críticos porque un nodo que restaura estado
incorrecto puede tomar decisiones erróneas durante el warmup que sigue al
reinicio — sin que nadie lo note hasta que el actuador hace algo inesperado.

```rust
// Plantilla — copiar a nodes/<nuevo_nodo>.rs en el bloque #[cfg(test)]

#[cfg(test)]
mod tests {
    use super::*;
    use agrodash_shared::Signal;

    // ── Helper de pool ───────────────────────────────────────────────────────
    // El CI provee DATABASE_URL. Para nodos que no usan la DB (filtros,
    // decisores, watchdog) se usa connect_lazy que no conecta hasta que
    // se llama execute()/fetch_*. Si un test falla con "connection refused",
    // el nodo sí usa el pool y hay que asegurarse de que DATABASE_URL esté
    // disponible.
    async fn pool() -> sqlx::PgPool {
        match std::env::var("DATABASE_URL") {
            Ok(url) => sqlx::PgPool::connect(&url).await
                .expect("DATABASE_URL disponible pero no se pudo conectar"),
            Err(_)  => sqlx::PgPool::connect_lazy("postgres://localhost/agrodash_test")
                .expect("URL de fallback inválida"),
        }
    }

    // ── Test 1: ejecución con entrada válida ─────────────────────────────────
    #[tokio::test]
    async fn ejecuta_con_entrada_valida() {
        let mut nodo = MiNodo::new("t".into(), config_test());
        let res = nodo.execute(vec![Signal::Vector(vec![1.0, 2.0])], 1.0, &pool().await).await;
        assert!(res.is_ok(), "execute falló con entrada válida: {:?}", res);
    }

    // ── Test 2: entrada vacía sin pánico ─────────────────────────────────────
    // Ocurre cuando el nodo upstream no emitió señal (ej: durante warmup).
    // Debe retornar Ok(None) o Err descriptivo, nunca panic!().
    #[tokio::test]
    async fn entrada_vacia_retorna_none_o_err() {
        let mut nodo = MiNodo::new("t".into(), config_test());
        let res = nodo.execute(vec![], 1.0, &pool().await).await;
        match res {
            Ok(None) | Err(_) => {}
            Ok(Some(sig)) => panic!("retornó señal con entrada vacía: {:?}", sig),
        }
    }

    // ── Test 3: round-trip save/load state ───────────────────────────────────
    // La invariante fundamental del sistema de persistencia:
    // guardar y restaurar el estado debe producir el mismo comportamiento.
    //
    // Cómo falla en la práctica:
    //   - Se agrega un campo nuevo al struct pero no a load_state()
    //   - Se cambia el nombre de un campo JSON sin actualizar load_state()
    //   - Un buffer (VecDeque, TrendTracker) no se restaura correctamente
    #[tokio::test]
    async fn round_trip_save_load_state() {
        let pool = pool().await;
        let mut nodo = MiNodo::new("t".into(), config_test());

        // Warm up: suficientes ciclos para estado no trivial
        for v in &[1.0f64, 1.3, 0.9, 1.1, 1.4, 1.2, 0.8, 1.5, 1.0, 1.2] {
            nodo.execute(vec![Signal::Vector(vec![*v])], 1.0, &pool).await.ok();
        }

        let estado = nodo.save_state();
        let mut nodo2 = MiNodo::new("t".into(), config_test());
        nodo2.load_state(&estado);

        let input = vec![Signal::Vector(vec![1.2])];
        let out1  = nodo.execute(input.clone(), 1.0, &pool).await.unwrap();
        let out2  = nodo2.execute(input, 1.0, &pool).await.unwrap();

        assert_signals_approx_eq(out1, out2);
    }

    // ── Test 4: vector multidimensional ──────────────────────────────────────
    // Los nodos deben manejar dim=1, dim=3 sin errores de indexación.
    #[tokio::test]
    async fn maneja_vector_multidimensional() {
        let mut nodo = MiNodo::new("t".into(), config_test());
        let res = nodo.execute(vec![Signal::Vector(vec![1.0, 2.0, 3.0])], 1.0, &pool().await).await;
        assert!(res.is_ok(), "falló con dim=3: {:?}", res);
    }

    // ── Test 5: is_ready transición correcta ─────────────────────────────────
    // Para nodos con warmup_samples > 0.
    // Invariante: is_ready() es false durante el warmup, true después,
    // y nunca vuelve a false.
    #[tokio::test]
    async fn is_ready_warmup_monotonico() {
        let pool   = pool().await;
        let warmup = 5usize;
        let mut nodo = MiNodo::new("t".into(), config_con_warmup(warmup));

        for i in 0..warmup {
            assert!(!nodo.is_ready(), "is_ready true antes del warmup, ciclo {i}");
            nodo.execute(vec![Signal::Vector(vec![1.0])], 1.0, &pool).await.ok();
        }
        assert!(nodo.is_ready(), "is_ready false tras {warmup} muestras");

        for _ in 0..5 {
            nodo.execute(vec![Signal::Vector(vec![1.0])], 1.0, &pool).await.ok();
            assert!(nodo.is_ready(), "is_ready volvió a false tras el warmup");
        }
    }

    // ── Test 6: compatibilidad con estado antiguo ────────────────────────────
    // Los estados existentes en la DB no tienen los campos nuevos.
    // load_state() debe aceptarlos usando el valor default del campo.
    // Guardar aquí el JSON de la versión anterior al cambio y no borrarlo.
    #[test]
    fn load_state_acepta_formato_antiguo() {
        let estado_antiguo = serde_json::json!({
            "node_id":   "t",
            "node_type": "mi_nodo",
            "is_ready":  true,
            "data": {
                "campo_existente": 42.0
                // campo_nuevo ausente — debe usar default
            }
        });
        let node_state: agrodash_shared::NodeState =
            serde_json::from_value(estado_antiguo).expect("NodeState no deserializa");
        let mut nodo = MiNodo::new("t".into(), config_test());
        nodo.load_state(&node_state); // no debe fallar
    }

    fn assert_signals_approx_eq(a: Option<Signal>, b: Option<Signal>) {
        match (a, b) {
            (Some(Signal::Vector(v1)), Some(Signal::Vector(v2))) => {
                assert_eq!(v1.len(), v2.len(), "dimensiones distintas tras round-trip");
                for (i, (x, y)) in v1.iter().zip(v2.iter()).enumerate() {
                    assert!(
                        (x - y).abs() < 1e-10,
                        "componente {i} difiere: {x} vs {y}"
                    );
                }
            }
            (Some(Signal::Action(a1)), Some(Signal::Action(a2))) => {
                assert_eq!(a1, a2, "acción distinta tras round-trip");
            }
            (None, None) => {}
            (a, b) => panic!("tipos de señal distintos: {a:?} vs {b:?}"),
        }
    }
}
```

### 1.2 Tests de propiedades para filtros y decisores

**Fundamento**: Claessen & Hughes (2000, *QuickCheck*, ICFP) muestran que para
sistemas numéricos los casos edge que los humanos no pensamos —valores extremos,
secuencias monótonas, una sola muestra— son donde se concentran los bugs.
`proptest` genera estos casos y encoge el contraejemplo al mínimo reproducible.

Agregar a `agent/Cargo.toml`:
```toml
[dev-dependencies]
proptest = "1"
```

```rust
use proptest::prelude::*;

// Propiedad: cualquier filtro lineal estable no amplifica la señal.
// Kalman, EWMA, MovingAvg y Lowpass tienen ganancia DC ≤ 1 por diseño.
// Si la salida sale del rango de la entrada, hay un bug de indexación.
proptest! {
    #[test]
    fn filtro_no_amplifica_la_señal(
        alpha  in 0.01f64..0.99,
        valores in prop::collection::vec(-1e6f64..1e6f64, 5..30),
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let pool = pool().await;
            let mut nodo = EwmaNode::new("t".into(), agrodash_shared::EwmaConfig {
                alpha: agrodash_shared::VecOrScalar::Scalar(alpha),
                warmup_samples: 3,
            });
            let min = valores.iter().cloned().fold(f64::INFINITY,     f64::min);
            let max = valores.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            for v in &valores {
                if let Ok(Some(Signal::Vector(out))) = nodo
                    .execute(vec![Signal::Vector(vec![*v])], 1.0, &pool).await
                {
                    prop_assert!(
                        out[0] >= min - 1e-9 && out[0] <= max + 1e-9,
                        "EWMA amplificó: out={} fuera de [{min}, {max}]", out[0]
                    );
                }
            }
        });
    }
}

// Propiedad: Hysteresis no oscila con señal en zona neutra.
// Un bug que causa oscilación haría que el actuador se encienda y apague
// en cada ciclo con sensores ruidosos.
proptest! {
    #[test]
    fn hysteresis_estable_en_zona_neutra(
        low   in  5.0f64..40.0,
        high  in 60.0f64..95.0,
        ruido in  0.0f64..0.4,
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let pool = pool().await;
            let cfg = agrodash_shared::HysteresisConfig {
                reduction: agrodash_shared::Reduction::Mean,
                low, high,
                action_below_low:  agrodash_shared::NodeAction::On,
                action_above_high: agrodash_shared::NodeAction::Off,
                trend: None,
            };
            let mut nodo = HysteresisNode::new("t".into(), cfg);
            nodo.execute(vec![Signal::Vector(vec![low - 1.0])], 1.0, &pool).await.ok();
            let estado_base = nodo.save_state().data["state"].clone();

            let amplitud = (high - low) * ruido;
            let centro   = (low + high) / 2.0;
            for delta in [-amplitud, 0.0, amplitud] {
                nodo.execute(vec![Signal::Vector(vec![centro + delta])], 1.0, &pool).await.ok();
                prop_assert_eq!(
                    &estado_base,
                    &nodo.save_state().data["state"],
                    "Hysteresis oscila en zona neutra: val={:.2}", centro + delta
                );
            }
        });
    }
}
```

### 1.3 Tests FSM del Watchdog — una prueba por transición

**Fundamento**: Utting & Legeard (2006, *Practical Model-Based Testing*,
cap. 4) demuestran que para FSMs la cobertura de transiciones supera a la
cobertura de líneas en DDE. Con el Watchdog (4 estados, ~9 transiciones),
un test por fila de esta tabla cubre el espacio completo.

| # | Estado origen | Evento                 | Estado destino | Output   |
|---|---|---|---|---|
| T1 | Ok           | feedback correcto      | Ok             | decisión |
| T2 | Ok           | feedback discrepa      | Retrying       | decisión |
| T3 | Retrying     | feedback correcto      | Ok             | decisión |
| T4 | Retrying     | discrepa × max_retries | Blocked        | Hold     |
| T5 | Blocked      | cualquier input        | Blocked        | Hold     |
| T6 | Blocked      | ClearWatchdog          | Ok             | —        |
| T7 | Ok           | acción nueva (Ugly)    | PendingUser    | Hold     |
| T8 | PendingUser  | ConfirmWatchdog        | Ok             | acción   |
| T9 | PendingUser  | cualquier input        | PendingUser    | Hold     |

Si se agrega un modo nuevo o una transición nueva: agregar la fila a esta
tabla y el test correspondiente antes de mergear.

### 1.4 Interacción entre nodos — tests del scheduler

**Cuándo escribirlos**: cuando el nodo nuevo tiene comportamiento que depende
de qué nodo está conectado upstream, o usa puertos (`to_port`) específicos.

```rust
// En agent/src/scheduler.rs — módulo #[cfg(test)]
#[cfg(test)]
mod integration {
    use super::*;
    use agrodash_shared::*;

    // Helper: construir grafo sin nodos que usen la DB
    async fn grafo_sin_db(
        nodes: Vec<NodeConfig>,
        edges: Vec<EdgeConfig>,
    ) -> PipelineGraph {
        let pool = sqlx::PgPool::connect_lazy("postgres://localhost/x").unwrap();
        PipelineGraph::build(&nodes, &edges, &None, &pool)
            .await
            .expect("grafo de test no se pudo construir")
    }

    // Test: el Watchdog recibe inputs en el orden canónico (decision antes que feedback)
    // Verifica la lógica de scheduler::collect_inputs con to_port.
    // Si falla: el Watchdog confunde la señal del decisor con la del subscriber.
    #[tokio::test]
    async fn watchdog_puertos_en_orden_correcto() {
        let nodes = vec![
            NodeConfig { id: "dec".into(), kind: NodeKind::Passthrough },
            NodeConfig { id: "sub".into(), kind: NodeKind::Passthrough },
            NodeConfig { id: "wd".into(),  kind: NodeKind::Watchdog(WatchdogConfig {
                actuator_id:         "act".into(),
                mode:                WatchdogMode::Good,
                action_timeout_secs: 0,
                max_retries:         0,
            })},
        ];
        let edges = vec![
            EdgeConfig { from: "dec".into(), to: "wd".into(),
                         from_port: None, to_port: Some("decision".into()),
                         kind: EdgeKind::Decision },
            EdgeConfig { from: "sub".into(), to: "wd".into(),
                         from_port: None, to_port: Some("feedback".into()),
                         kind: EdgeKind::Feedback },
        ];
        let graph = grafo_sin_db(nodes, edges).await;
        // El grafo debe construirse sin error — los puertos son válidos
        assert!(graph.has_watchdog());
    }

    // Test: un pipeline recién construido empieza en warmup (is_ready = false)
    #[tokio::test]
    async fn pipeline_nuevo_empieza_en_warmup() {
        let nodes = vec![
            NodeConfig { id: "src".into(), kind: NodeKind::Passthrough },
            NodeConfig { id: "flt".into(), kind: NodeKind::Kalman(KalmanConfig {
                Q: VecOrScalar::Scalar(0.01),
                R: VecOrScalar::Scalar(0.1),
                P0: 1.0,
                convergence_threshold: 0.01,
                warmup_samples: 10,
            })},
        ];
        let edges = vec![
            EdgeConfig { from: "src".into(), to: "flt".into(),
                         from_port: None, to_port: None, kind: EdgeKind::Data },
        ];
        let graph = grafo_sin_db(nodes, edges).await;
        assert!(!graph.is_ready(), "pipeline recién construido debería estar en warmup");
    }
}
```

---

## Crate 2: `agrodash-api`

### 2.1 Tests de handlers HTTP

**Fundamento**: Freeman & Pryce (2009, *Growing Object-Oriented Software,
Guided by Tests*) argumentan que los handlers se testean con tests de
**contrato** — dado este request, la respuesta tiene esta forma. El CI
tiene DB, así que estos tests pueden ser de integración completa.

Agregar a `api/Cargo.toml`:
```toml
[dev-dependencies]
axum-test = "15"
```

```rust
// En api/src/routes/processes.rs — módulo #[cfg(test)]
#[cfg(test)]
mod tests {
    use super::*;

    async fn db_pool() -> sqlx::PgPool {
        sqlx::PgPool::connect(
            &std::env::var("DATABASE_URL").expect("CI debe proveer DATABASE_URL")
        ).await.expect("no se pudo conectar a DB de test")
    }

    // Test de contrato: body sin pipeline_id → 400, no 500
    // Verifica que la validación del input ocurre antes de la DB.
    #[tokio::test]
    async fn send_command_sin_pipeline_id_retorna_400() {
        // Sin axum-test, usar tower::ServiceExt directamente:
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use tower::ServiceExt;

        let pool    = db_pool().await;
        let manager = crate::agent_manager::AgentManager::new(
            crate::agent_manager::AgentManagerConfig::default(),
            pool.clone(),
        );
        let state = crate::AppState { pool, manager };
        let app   = crate::build_router(state); // extraer Router a fn pública

        let fake_id = uuid::Uuid::new_v4();
        let req = Request::builder()
            .method("POST")
            .uri(&format!("/api/v1/processes/{fake_id}/command"))
            .header("content-type", "application/json")
            .body(Body::from(r#"{"cmd":"Checkpoint"}"#)) // sin pipeline_id
            .unwrap();

        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    }

    // Test: proceso inexistente → 404
    #[tokio::test]
    async fn get_proceso_inexistente_retorna_404() {
        // similar al anterior — usar tower::ServiceExt
        todo!("implementar con tower::ServiceExt");
    }

    // Test de contrato para ClearWatchdog:
    // comando válido → 200 con {accepted: true}
    // comando desconocido → 400
    #[tokio::test]
    async fn clear_watchdog_retorna_accepted() {
        todo!("implementar tras crear fixture de proceso en DB de test");
    }
}
```

### 2.2 Tests de queries — rendimiento y correctitud en DB

**Fundamento**: Winand (2012, *Use The Index, Luke*, cap. 2) demuestra que
en tablas time-series que crecen continuamente, `ORDER BY ts DESC LIMIT N`
sin índice pasa de O(log N) a O(N). Las dos tablas más críticas aquí son
`process_readings` y `process_logs`, que crecen con cada ciclo del agente.

#### a) Verificar que los índices críticos existen

Estos tests deben estar en CI. Si fallan, la migración no se aplicó.

Los índices necesarios, basados en las queries del codebase:

```sql
-- process_readings: WHERE process_id=$1 [AND pipeline_id=$2] ORDER BY ts DESC LIMIT N
CREATE INDEX IF NOT EXISTS idx_process_readings_lookup
    ON process_readings (process_id, pipeline_id, ts DESC);

-- process_logs: WHERE process_id=$1 [AND level/source] ORDER BY ts DESC LIMIT N
CREATE INDEX IF NOT EXISTS idx_process_logs_lookup
    ON process_logs (process_id, ts DESC);

-- pipeline_states: WHERE process_id=$1 AND pipeline_id=$2 (lookup puntual)
CREATE INDEX IF NOT EXISTS idx_pipeline_states_lookup
    ON pipeline_states (process_id, pipeline_id);

-- readings: WHERE sensor_id=$1 [AND created_at BETWEEN] ORDER BY created_at DESC
CREATE INDEX IF NOT EXISTS idx_readings_sensor_time
    ON readings (sensor_id, created_at DESC);
```

```rust
#[cfg(test)]
mod db_tests {
    async fn pool() -> sqlx::PgPool {
        sqlx::PgPool::connect(
            &std::env::var("DATABASE_URL").expect("CI debe proveer DATABASE_URL")
        ).await.expect("no se pudo conectar")
    }

    #[tokio::test]
    async fn indices_criticos_existen() {
        let pool = pool().await;

        // (tabla, columna que debe aparecer en el indexdef)
        let checks = [
            ("process_readings", "process_id"),
            ("process_logs",     "process_id"),
            ("pipeline_states",  "process_id"),
            ("readings",         "sensor_id"),
        ];

        for (tabla, columna) in &checks {
            let existe: bool = sqlx::query_scalar!(
                r#"SELECT EXISTS(
                    SELECT 1 FROM pg_indexes
                    WHERE tablename = $1 AND indexdef LIKE $2
                ) AS "existe!""#,
                tabla,
                format!("%{columna}%"),
            )
            .fetch_one(&pool)
            .await
            .unwrap_or(false);

            assert!(
                existe,
                "falta índice en '{columna}' de '{tabla}'\n\
                 Agregar en migración:\n  \
                 CREATE INDEX IF NOT EXISTS idx_{tabla}_lookup ON {tabla} ({columna}, ...);"
            );
        }
    }

    // ── Verificar que las queries críticas usan el índice ─────────────────────
    // EXPLAIN con ANALYZE false — solo el plan, sin ejecutar la query.
    // Corre en CI porque solo necesita el schema, no datos.

    #[tokio::test]
    async fn get_readings_usa_indice() {
        let pool    = pool().await;
        let fake_id = uuid::Uuid::nil(); // no existe en DB, plan es el mismo

        let plan: Option<serde_json::Value> = sqlx::query_scalar!(
            r#"EXPLAIN (FORMAT JSON, ANALYZE false)
               SELECT ts, raw, filtered, actuator FROM process_readings
               WHERE process_id = $1
               ORDER BY ts DESC LIMIT 500"#,
            fake_id,
        )
        .fetch_one(&pool)
        .await
        .expect("EXPLAIN falló");

        let plan_str = plan.map(|v| v.to_string()).unwrap_or_default();
        assert!(
            !plan_str.contains("Seq Scan"),
            "get_readings hace Seq Scan — falta índice en process_readings\n\
             Plan: {plan_str}"
        );
    }

    #[tokio::test]
    async fn get_logs_usa_indice() {
        let pool    = pool().await;
        let fake_id = uuid::Uuid::nil();

        let plan: Option<serde_json::Value> = sqlx::query_scalar!(
            r#"EXPLAIN (FORMAT JSON, ANALYZE false)
               SELECT ts, level, source, message FROM process_logs
               WHERE process_id = $1
               ORDER BY ts DESC LIMIT 100"#,
            fake_id,
        )
        .fetch_one(&pool)
        .await
        .expect("EXPLAIN falló");

        let plan_str = plan.map(|v| v.to_string()).unwrap_or_default();
        assert!(
            !plan_str.contains("Seq Scan"),
            "get_logs hace Seq Scan — falta índice en process_logs\n\
             Plan: {plan_str}"
        );
    }

    // ── stats_worker: tiempo de ciclo ────────────────────────────────────────
    // El worker actual hace 3 queries por sensor (stats 24h, last value, prev 1h)
    // más queries de correlación. Con 50 sensores son ~150 queries por ciclo.
    //
    // Fundamento: Hamilton (2007, USENIX LISA) documenta que las "chatty queries"
    // son la causa más común de degradación bajo carga. La refactorización
    // correcta es una sola query con GROUP BY sensor_id.
    //
    // Este test mide el tiempo real del ciclo. Si supera 10s con los datos
    // de la DB de test, hay que consolidar las queries.
    #[tokio::test]
    async fn stats_worker_ciclo_bajo_10_segundos() {
        let pool  = pool().await;
        let start = std::time::Instant::now();

        crate::tasks::stats_worker::compute_and_store(&pool)
            .await
            .expect("ciclo de stats falló");

        let elapsed = start.elapsed();
        assert!(
            elapsed.as_secs() < 10,
            "stats_worker tardó {}ms — demasiado lento con {} queries por sensor.\n\
             Refactorizar a GROUP BY sensor_id.",
            elapsed.as_millis(),
            3  // queries actuales por sensor
        );
    }

    // ── Retención de datos — tablas time-series ───────────────────────────────
    // Fundamento: Nygard (2007, *Release It!*, cap. 4) documenta el patrón
    // "unbounded result sets" como causa de degradación gradual. process_logs
    // y process_readings crecen con cada ciclo del agente. Sin retención, las
    // queries se vuelven lentas aunque haya índice.
    //
    // Este test no verifica que la retención ocurra, sino que el índice
    // necesario para hacer DELETEs eficientes existe.
    #[tokio::test]
    async fn tablas_timeseries_tienen_indice_en_timestamp() {
        let pool = pool().await;
        let checks = [
            ("process_logs",     "ts"),
            ("process_readings", "ts"),
            ("readings",         "created_at"),
        ];

        for (tabla, columna) in &checks {
            let existe: bool = sqlx::query_scalar!(
                r#"SELECT EXISTS(
                    SELECT 1 FROM pg_indexes
                    WHERE tablename = $1 AND indexdef LIKE $2
                ) AS "existe!""#,
                tabla,
                format!("%{columna}%"),
            )
            .fetch_one(&pool)
            .await
            .unwrap_or(false);

            assert!(
                existe,
                "'{tabla}' no tiene índice en '{columna}' — \
                 los DELETEs de retención serán lentos con datos históricos"
            );
        }
    }
}
```

### 2.3 Funcionalidad nueva en la DB — qué testear

**Fundamento**: Ambler & Sadalage (2006, *Refactoring Databases*) identifican
que los bugs más comunes en cambios de schema son: NULLs inesperados, FK
faltantes, y defaults incorrectos para datos existentes. Los tests de schema
son baratos de escribir y se ejecutan en CI sin fixtures.

Para cada tabla nueva o columna nueva, cubrir estos tres puntos:

```rust
// 1. El índice existe y tiene las columnas correctas
#[tokio::test]
async fn nueva_tabla_tiene_indice() {
    // Ver patrón en sección 2.2.a
    todo!("adaptar al schema específico");
}

// 2. Las foreign keys existen con la política ON DELETE correcta
#[tokio::test]
async fn nueva_tabla_fk_on_delete_correcto() {
    let pool = pool().await;
    let policy: Option<String> = sqlx::query_scalar!(
        r#"
        SELECT rc.delete_rule
        FROM information_schema.referential_constraints rc
        JOIN information_schema.table_constraints tc
          ON tc.constraint_name = rc.constraint_name
        WHERE tc.table_name = 'mi_tabla_nueva'
        LIMIT 1
        "#
    )
    .fetch_optional(&pool)
    .await
    .unwrap_or(None)
    .flatten();

    assert!(policy.is_some(), "mi_tabla_nueva no tiene FK definida");
    // CASCADE para datos que deben borrarse con el padre
    // RESTRICT para datos que no deben huerfanarse silenciosamente
    assert_eq!(policy.unwrap(), "CASCADE",
        "política ON DELETE incorrecta — considerar el impacto en datos existentes");
}

// 3. Una columna nueva con DEFAULT no rompe filas existentes
#[tokio::test]
async fn nueva_columna_con_default_no_rompe_filas_existentes() {
    let pool = pool().await;
    // Verificar que la columna existe y tiene un default definido
    let tiene_default: bool = sqlx::query_scalar!(
        r#"SELECT column_default IS NOT NULL AS "tiene!"
           FROM information_schema.columns
           WHERE table_name = 'mi_tabla' AND column_name = 'mi_columna_nueva'"#
    )
    .fetch_one(&pool)
    .await
    .unwrap_or(false);

    assert!(
        tiene_default,
        "mi_columna_nueva no tiene DEFAULT — las filas existentes tendrán NULL \
         si la columna es NOT NULL"
    );
}
```

### 2.4 AgentManager — tests de gestión de procesos

El `AgentManager` gestiona procesos del sistema operativo. Los bugs silenciosos
aquí son los más costosos: un proceso que debería reiniciarse no lo hace, o uno
que debería detenerse queda como zombie.

```rust
// En api/src/agent_manager.rs — módulo #[cfg(test)]
#[cfg(test)]
mod tests {
    use super::*;

    async fn manager_test() -> Arc<AgentManager> {
        let pool = sqlx::PgPool::connect(
            &std::env::var("DATABASE_URL").expect("CI debe proveer DATABASE_URL")
        ).await.unwrap();
        AgentManager::new(AgentManagerConfig::default(), pool)
    }

    // Test: stop_process remueve la key del HashMap
    // Si no la remueve, el monitor intenta reiniciar un proceso detenido.
    #[tokio::test]
    async fn stop_process_remueve_del_mapa() {
        let manager = manager_test().await;
        let fake_id = uuid::Uuid::new_v4();

        // stop_process con proceso inexistente no debe fallar
        manager.stop_process(fake_id).await;

        let activos = manager.active_pipelines(fake_id).await;
        assert!(activos.is_empty(), "stop_process dejó entradas en el mapa");
    }

    // Test: notify no falla si el agente no está escuchando
    // El NOTIFY es fire-and-forget — el desired state en DB garantiza ejecución.
    #[tokio::test]
    async fn notify_no_falla_sin_agente_activo() {
        let manager = manager_test().await;
        let key     = AgentKey {
            process_id:  uuid::Uuid::new_v4(),
            pipeline_id: "pl_test".into(),
        };
        let result = manager.notify(&key, agrodash_shared::AgentCommand::Checkpoint).await;
        assert!(result.is_ok(), "notify falló: {:?}", result);
    }
}
```

---

## Crate 3: `agrodash-shared`

### 3.1 Round-trip de serialización para todos los tipos de config

**Fundamento**: Beck (2002, *TDD by Example*) y Fowler (2018, *Refactoring*,
2ª ed.) argumentan que para tipos que cruzan fronteras de sistema —aquí:
serializados a JSON y guardados en PostgreSQL como JSONB— el test de
round-trip es el más valioso. Si `deserialize(serialize(x)) != x`, hay
un bug silencioso que solo aparece al reiniciar el agente o recargar la config.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // Macro para evitar repetición
    macro_rules! test_rt {
        ($nombre:ident, $tipo:ty, $valor:expr) => {
            #[test]
            fn $nombre() {
                let original          = $valor;
                let json              = serde_json::to_string(&original)
                    .expect("serialize falló");
                let parsed: $tipo     = serde_json::from_str(&json)
                    .expect("deserialize falló — ¿falta #[serde(default)]?");
                let json2             = serde_json::to_string(&parsed)
                    .expect("re-serialize falló");
                assert_eq!(json, json2,
                    "round-trip produjo JSON diferente — serialización no simétrica");
            }
        };
    }

    test_rt!(watchdog_good_rt, WatchdogConfig, WatchdogConfig {
        actuator_id: "bomba_1".into(),
        mode: WatchdogMode::Good,
        action_timeout_secs: 30,
        max_retries: 3,
    });

    test_rt!(watchdog_bad_rt, WatchdogConfig, WatchdogConfig {
        actuator_id: "bomba_1".into(),
        mode: WatchdogMode::Bad {
            expected_on_trend:  Trend::Ascending,
            expected_off_trend: Trend::Descending,
            min_change_pct:     0.05,
            component:          Some(0),
        },
        action_timeout_secs: 60,
        max_retries: 2,
    });

    test_rt!(watchdog_ugly_rt, WatchdogConfig, WatchdogConfig {
        actuator_id: "valvula_2".into(),
        mode: WatchdogMode::Ugly { notify_message: Some("verificar presión manual".into()) },
        action_timeout_secs: 0,
        max_retries: 0,
    });

    test_rt!(mqtt_subscriber_rt, MqttSubscriberConfig, MqttSubscriberConfig {
        connection:       ConnectionRef::Named("shared".into()),
        topic:            "sensor/estado".into(),
        payload_on:       "ON".into(),
        payload_off:      "OFF".into(),
        stale_after_secs: Some(120),
    });

    test_rt!(edge_con_kind_rt, EdgeConfig, EdgeConfig {
        from: "dec1".into(), to: "wd1".into(),
        from_port: None,
        to_port:   Some("decision".into()),
        kind:      EdgeKind::Decision,
    });

    // ── Compatibilidad hacia atrás ────────────────────────────────────────────
    // Los configs existentes en la DB no tienen los campos nuevos.
    // Cada campo nuevo DEBE tener #[serde(default)] o la DB se vuelve
    // incompatible con el código nuevo tras un deploy.
    // Estos tests no se borran — son la documentación de qué formato antiguo
    // debe seguir funcionando.

    #[test]
    fn hysteresis_antigua_sin_trend_funciona() {
        let json = r#"{
            "reduction": {"type": "mean"},
            "low": 30.0, "high": 60.0,
            "action_below_low": "on",
            "action_above_high": "off"
        }"#;
        let cfg: HysteresisConfig = serde_json::from_str(json)
            .expect("HysteresisConfig antigua no deserializa — \
                     agregar #[serde(default)] al campo 'trend'");
        assert!(cfg.trend.is_none());
    }

    #[test]
    fn edge_antiguo_sin_kind_es_data() {
        let json   = r#"{"from": "a", "to": "b"}"#;
        let edge: EdgeConfig = serde_json::from_str(json)
            .expect("EdgeConfig sin 'kind' no deserializa");
        assert_eq!(edge.kind, EdgeKind::Data,
            "EdgeKind default no es Data — rompe todos los pipelines existentes");
    }

    #[test]
    fn sprt_antiguo_sin_trend_funciona() {
        let json = r#"{
            "mu_H0": 30.0, "mu_H1": 70.0, "sigma": 5.0,
            "alpha": 0.05, "beta": 0.05,
            "reduction": {"type": "mean"},
            "reset_on_action": true
        }"#;
        let _cfg: SprtConfig = serde_json::from_str(json)
            .expect("SprtConfig antigua sin 'trend' no deserializa");
    }

    // ── AgentCommand — payload del PG NOTIFY ─────────────────────────────────
    // Si el tag de serde cambia, el agente ignora el comando silenciosamente.
    // Estos tests son la especificación del protocolo NOTIFY.

    #[test]
    fn agent_commands_tags_correctos() {
        let casos: &[(AgentCommand, &str)] = &[
            (AgentCommand::Stop,                                  "stop"),
            (AgentCommand::Checkpoint,                            "checkpoint"),
            (AgentCommand::Reload,                                "reload"),
            (AgentCommand::ClearWatchdog { actuator_id: None },   "clear_watchdog"),
            (AgentCommand::ConfirmWatchdog { actuator_id: None }, "confirm_watchdog"),
        ];

        for (cmd, tag) in casos {
            let json = serde_json::to_string(cmd).unwrap();
            assert!(
                json.contains(&format!("\"cmd\":\"{tag}\"")),
                "{tag} no serializa con tag correcto: {json}"
            );
        }
    }

    #[test]
    fn agent_commands_deserializan_payloads_notify() {
        let payloads = [
            r#"{"cmd":"stop"}"#,
            r#"{"cmd":"checkpoint"}"#,
            r#"{"cmd":"reload"}"#,
            r#"{"cmd":"clear_watchdog","actuator_id":null}"#,
            r#"{"cmd":"confirm_watchdog","actuator_id":"bomba_1"}"#,
            r#"{"cmd":"override","action":"on","actuator_id":null}"#,
        ];

        for payload in &payloads {
            let result: Result<AgentCommand, _> = serde_json::from_str(payload);
            assert!(result.is_ok(),
                "payload NOTIFY no deserializa: {payload}\nError: {:?}", result.err());
        }
    }

    // ── VecOrScalar ──────────────────────────────────────────────────────────

    #[test]
    fn vecorscalar_scalar_expande() {
        let s = VecOrScalar::Scalar(0.5);
        assert_eq!(s.expand(3), vec![0.5, 0.5, 0.5]);
        assert_eq!(s.expand(0), vec![]);
    }

    #[test]
    fn vecorscalar_vec_devuelve_clon() {
        let v = VecOrScalar::Vec(vec![1.0, 2.0, 3.0]);
        assert_eq!(v.expand(3), vec![1.0, 2.0, 3.0]);
        // Nota: expand() no valida que n == len() cuando es Vec.
    }
}
```

---

## Integración con CI/CD

El workflow `.gitea/workflows/rust.yml` corre en este orden:

```
fmt   → clippy   → test (con DATABASE_URL)   → build   → deploy
```

Ningún step posterior corre si el anterior falla. El deploy no ocurre si
hay tests fallando.

Los tests `#[ignore]` están reservados para tests de carga que tardan minutos
y no deben correr en cada push. Documentar la razón en el comentario.

---

## Referencia bibliográfica

- Ambler, S., Sadalage, P. (2006). *Refactoring Databases*. Addison-Wesley.
- Andrews, L., Briand, L., Labiche, Y. (2006). *Is mutation an appropriate
  tool for testing experiments?* ICSE 2006.
- Beck, K. (2002). *Test-Driven Development by Example*. Addison-Wesley.
- Claessen, K., Hughes, J. (2000). *QuickCheck: A Lightweight Tool for Random
  Testing of Haskell Programs*. ICFP 2000.
- Fowler, M. (2018). *Refactoring*, 2ª ed. Addison-Wesley.
- Freeman, S., Pryce, N. (2009). *Growing Object-Oriented Software, Guided
  by Tests*. Addison-Wesley.
- Hamilton, J. (2007). *On Designing and Deploying Internet-Scale Services*.
  USENIX LISA 2007.
- Juristo, N., Moreno, A., Vegas, S. (2004). *Reviewing 25 Years of Testing
  Technique Experiments*. Empirical Software Engineering.
- Meszaros, G. (2007). *xUnit Test Patterns*. Addison-Wesley.
- Nygard, M. (2007). *Release It!*. Pragmatic Bookshelf.
- Utting, M., Legeard, B. (2006). *Practical Model-Based Testing*.
  Morgan Kaufmann.
- Winand, M. (2012). *Use The Index, Luke*. https://use-the-index-luke.com
