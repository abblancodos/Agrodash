// src/script_engine.rs
//
// Motor de scripting Rhai para los pasos de experimentos.
//
// Funciones disponibles en los scripts:
//   log(msg)                          — mensaje informativo
//   warn(msg)                         — advertencia
//   error(msg)                        — error
//   output(key, value, unit)          — valor calculado para mostrar como card
//   plot(id, config_map)              — declarar gráfica
//   table(id, config_map)             — declarar tabla
//   goto(step_key)                    — navegar al paso indicado
//
// Variables disponibles (solo lectura):
//   constants  — Map con los valores de las constantes del experimento
//   steps      — Map con los valores de pasos anteriores registrados

use rhai::{Engine, Map, Scope, Dynamic, EvalAltResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::{Arc, Mutex};

// ── Tipos de salida ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ScriptOutput {
    Log   { level: String, message: String },
    Value { key: String, value: f64, unit: String },
    Plot  { id: String, config: Value },
    Table { id: String, config: Value },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ScriptResult {
    pub outputs:   Vec<ScriptOutput>,
    pub goto:      Option<String>,
    pub error:     Option<String>,
}

// ── Estado compartido durante la ejecución ────────────────────────────────────

#[derive(Default, Clone)]
struct ScriptState {
    outputs: Vec<ScriptOutput>,
    goto:    Option<String>,
}

// ── Construcción del engine ───────────────────────────────────────────────────

pub fn run_script(
    script:    &str,
    constants: &Value,   // objeto JSON con las constantes del experimento
    steps:     &Value,   // objeto JSON con valores de pasos anteriores
) -> ScriptResult {
    let state = Arc::new(Mutex::new(ScriptState::default()));

    let mut engine = Engine::new();

    // Límites de seguridad
    engine.set_max_operations(50_000);
    engine.set_max_string_size(4_096);
    engine.set_max_array_size(1_000);
    engine.set_max_map_size(500);

    // Deshabilitar todo acceso externo — ya está por defecto en Engine::new()
    // pero lo hacemos explícito
    engine.disable_symbol("eval");

    // ── Registrar funciones ───────────────────────────────────────────────────

    let s = state.clone();
    engine.register_fn("log", move |msg: &str| {
        s.lock().unwrap().outputs.push(ScriptOutput::Log {
            level: "info".into(), message: msg.to_string(),
        });
    });

    let s = state.clone();
    engine.register_fn("warn", move |msg: &str| {
        s.lock().unwrap().outputs.push(ScriptOutput::Log {
            level: "warn".into(), message: msg.to_string(),
        });
    });

    let s = state.clone();
    engine.register_fn("error", move |msg: &str| {
        s.lock().unwrap().outputs.push(ScriptOutput::Log {
            level: "error".into(), message: msg.to_string(),
        });
    });

    // output(key, value, unit)
    let s = state.clone();
    engine.register_fn("output", move |key: &str, value: f64, unit: &str| {
        s.lock().unwrap().outputs.push(ScriptOutput::Value {
            key: key.to_string(), value, unit: unit.to_string(),
        });
    });

    // output sin unidad
    let s = state.clone();
    engine.register_fn("output", move |key: &str, value: f64| {
        s.lock().unwrap().outputs.push(ScriptOutput::Value {
            key: key.to_string(), value, unit: String::new(),
        });
    });

    // plot(id, config_map)
    let s = state.clone();
    engine.register_fn("plot", move |id: &str, config: Map| {
        let config_json = rhai_map_to_json(config);
        s.lock().unwrap().outputs.push(ScriptOutput::Plot {
            id: id.to_string(), config: config_json,
        });
    });

    // table(id, config_map)
    let s = state.clone();
    engine.register_fn("table", move |id: &str, config: Map| {
        let config_json = rhai_map_to_json(config);
        s.lock().unwrap().outputs.push(ScriptOutput::Table {
            id: id.to_string(), config: config_json,
        });
    });

    // goto(step_key)
    let s = state.clone();
    engine.register_fn("goto", move |step: &str| {
        s.lock().unwrap().goto = Some(step.to_string());
    });

    // ── Construir scope con constantes y pasos anteriores ─────────────────────

    let mut scope = Scope::new();
    scope.push("constants", json_to_rhai_map(constants));
    scope.push("steps",     json_to_rhai_map(steps));

    // ── Ejecutar ──────────────────────────────────────────────────────────────

    let exec_error = match engine.eval_with_scope::<Dynamic>(&mut scope, script) {
        Ok(_)  => None,
        Err(e) => Some(format!("{e}")),
    };

    let final_state = state.lock().unwrap().clone();
    ScriptResult {
        outputs: final_state.outputs,
        goto:    final_state.goto,
        error:   exec_error,
    }
}

// ── Conversión JSON ↔ Rhai Map ────────────────────────────────────────────────

fn json_to_rhai_map(val: &Value) -> Dynamic {
    match val {
        Value::Object(obj) => {
            let mut map = Map::new();
            for (k, v) in obj {
                map.insert(k.clone().into(), json_to_rhai_map(v));
            }
            Dynamic::from_map(map)
        }
        Value::Array(arr) => {
            let vec: rhai::Array = arr.iter().map(json_to_rhai_map).collect();
            Dynamic::from_array(vec)
        }
        Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                Dynamic::from_float(f)
            } else {
                Dynamic::UNIT
            }
        }
        Value::Bool(b)   => Dynamic::from(*b),
        Value::String(s) => Dynamic::from(s.clone()),
        Value::Null      => Dynamic::UNIT,
    }
}

fn rhai_map_to_json(map: Map) -> Value {
    let obj: serde_json::Map<String, Value> = map
        .into_iter()
        .map(|(k, v)| (k.to_string(), rhai_dynamic_to_json(v)))
        .collect();
    Value::Object(obj)
}

fn rhai_dynamic_to_json(val: Dynamic) -> Value {
    if val.is_string() {
        Value::String(val.cast::<String>())
    } else if val.is_float() {
        let f = val.cast::<f64>();
        serde_json::Number::from_f64(f)
            .map(Value::Number)
            .unwrap_or(Value::Null)
    } else if val.is_int() {
        Value::Number(val.cast::<i64>().into())
    } else if val.is_bool() {
        Value::Bool(val.cast::<bool>())
    } else if val.is::<Map>() {
        rhai_map_to_json(val.cast::<Map>())
    } else if val.is::<rhai::Array>() {
        let arr: Vec<Value> = val.cast::<rhai::Array>()
            .into_iter()
            .map(rhai_dynamic_to_json)
            .collect();
        Value::Array(arr)
    } else {
        Value::Null
    }
}