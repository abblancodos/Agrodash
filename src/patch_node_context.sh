#!/usr/bin/env bash
# patch_node_context.sh
# Aplica el refactor de NodeContext al agente en el servidor.
# Correr desde la raíz del workspace: bash patch_node_context.sh
# 
# Qué hace:
#   1. Agrega NodeContext a nodes/mod.rs
#   2. Agrega _ctx al trait NodeInstance::execute()
#   3. Agrega _ctx a todos los impl execute() en cada nodo
#   4. Actualiza run_cycle() en scheduler.rs para recibir y pasar el ctx
#   5. Actualiza la llamada en main.rs

set -euo pipefail
AGENT="src/rust/agent/src"

echo "=== Patch NodeContext ==="

# ─────────────────────────────────────────────────────────────────────────────
# 1. nodes/mod.rs — agregar NodeContext struct + actualizar trait
# ─────────────────────────────────────────────────────────────────────────────
python3 - <<'PYEOF'
import re, sys

path = "src/rust/agent/src/nodes/mod.rs"
content = open(path).read()

# Agregar NodeContext antes del trait
if "pub struct NodeContext" not in content:
    ctx_struct = '''/// Contexto del pipeline pasado a cada nodo en cada ciclo.
#[derive(Clone, Debug)]
pub struct NodeContext {
    pub process_id:  String,
    pub pipeline_id: String,
    pub node_id:     String,
    pub node_label:  Option<String>,
}

'''
    content = content.replace(
        "// ── Trait ─────────",
        ctx_struct + "// ── Trait ─────────"
    )
    print("  + NodeContext struct añadida")
else:
    print("  - NodeContext ya existe")

# Actualizar la firma del trait
old_sig = "    async fn execute(\n        &mut self,\n        inputs: Vec<Signal>,\n        dt: f64,\n        pool: &PgPool,\n    ) -> Result<Option<Signal>>;"
new_sig = "    async fn execute(\n        &mut self,\n        inputs: Vec<Signal>,\n        dt: f64,\n        pool: &PgPool,\n        ctx: &NodeContext,\n    ) -> Result<Option<Signal>>;"

if "ctx: &NodeContext," not in content:
    if old_sig in content:
        content = content.replace(old_sig, new_sig, 1)
        print("  + trait execute() actualizado")
    else:
        print("  ! No se encontró la firma del trait — revisar manualmente", file=sys.stderr)
else:
    print("  - trait execute() ya tiene ctx")

open(path, "w").write(content)
PYEOF

# ─────────────────────────────────────────────────────────────────────────────
# 2. Todos los nodos — agregar _ctx a los impl execute()
# ─────────────────────────────────────────────────────────────────────────────
python3 - <<'PYEOF'
import os, re

node_files = [
    "src/rust/agent/src/nodes/decisions.rs",
    "src/rust/agent/src/nodes/filters.rs",
    "src/rust/agent/src/nodes/source.rs",
    "src/rust/agent/src/nodes/subscriber.rs",
    "src/rust/agent/src/nodes/utils.rs",
    "src/rust/agent/src/nodes/watchdog.rs",
]

# Patrones de parámetro pool con diferentes nombres/mutabilidades
PATTERNS = [
    ("        _pool: &PgPool,\n    ) -> Result<Option<Signal>>",
     "        _pool: &PgPool,\n        _ctx: &super::NodeContext,\n    ) -> Result<Option<Signal>>"),
    ("        pool: &PgPool,\n    ) -> Result<Option<Signal>>",
     "        pool: &PgPool,\n        _ctx: &super::NodeContext,\n    ) -> Result<Option<Signal>>"),
    ("        _inputs: Vec<Signal>,\n        _dt: f64,\n        pool: &PgPool,\n    ) -> Result<Option<Signal>>",
     "        _inputs: Vec<Signal>,\n        _dt: f64,\n        pool: &PgPool,\n        _ctx: &super::NodeContext,\n    ) -> Result<Option<Signal>>"),
    ("        _inputs: Vec<Signal>,\n        _dt: f64,\n        _pool: &PgPool,\n    ) -> Result<Option<Signal>>",
     "        _inputs: Vec<Signal>,\n        _dt: f64,\n        _pool: &PgPool,\n        _ctx: &super::NodeContext,\n    ) -> Result<Option<Signal>>"),
]

for path in node_files:
    content = open(path).read()
    orig = content
    for old, new in PATTERNS:
        content = content.replace(old, new)
    if content != orig:
        open(path, "w").write(content)
        print(f"  + {os.path.basename(path)} actualizado")
    else:
        # Check if already updated
        if "_ctx: &super::NodeContext" in content:
            print(f"  - {os.path.basename(path)} ya tiene ctx")
        else:
            print(f"  ! {os.path.basename(path)} — no se encontró el patrón, revisar manualmente")
PYEOF

# ─────────────────────────────────────────────────────────────────────────────
# 3. actuators.rs — execute() usa ctx (no _ctx)
# ─────────────────────────────────────────────────────────────────────────────
python3 - <<'PYEOF'
path = "src/rust/agent/src/nodes/actuators.rs"
content = open(path).read()
orig = content

# MqttActuatorNode execute()
old = "        inputs: Vec<Signal>,\n        _dt: f64,\n        pool: &PgPool,\n    ) -> Result<Option<Signal>> {\n        let action = match inputs.first() {\n            Some(Signal::Action(a)) => a.clone(),\n            _ => return Ok(None),\n        };\n        self.execute_override_with_pool(action, pool).await\n    }"
new = "        inputs: Vec<Signal>,\n        _dt: f64,\n        pool: &PgPool,\n        ctx: &super::NodeContext,\n    ) -> Result<Option<Signal>> {\n        let action = match inputs.first() {\n            Some(Signal::Action(a)) => a.clone(),\n            _ => return Ok(None),\n        };\n        self.execute_with_ctx(action, pool, ctx).await\n    }"

if old in content:
    content = content.replace(old, new, 1)
    print("  + MqttActuatorNode::execute() actualizado")

# HttpActuatorNode execute()
old_h = "    async fn execute(&mut self, inputs: Vec<Signal>, _dt: f64, _pool: &PgPool) -> Result<Option<Signal>>"
new_h = "    async fn execute(&mut self, inputs: Vec<Signal>, _dt: f64, _pool: &PgPool, _ctx: &super::NodeContext) -> Result<Option<Signal>>"
if old_h in content:
    content = content.replace(old_h, new_h, 1)
    print("  + HttpActuatorNode::execute() actualizado")

# execute_override_with_pool → execute_with_ctx
old_fn = "    async fn execute_override_with_pool(\n        &mut self,\n        action: NodeAction,\n        pool:   &PgPool,\n    ) -> Result<Option<Signal>>"
new_fn = "    async fn execute_with_ctx(\n        &mut self,\n        action: NodeAction,\n        pool:   &PgPool,\n        ctx:    &super::NodeContext,\n    ) -> Result<Option<Signal>>"
if old_fn in content:
    content = content.replace(old_fn, new_fn, 1)
    print("  + execute_override_with_pool → execute_with_ctx")

# Fix spawn_stage_wait call
old_spawn = '                    "unknown_process".into(),   // ← se resuelve en el siguiente paso\n                    "unknown_pipeline".into(),  // ← se resuelve en el siguiente paso'
new_spawn = '                    ctx.process_id.clone(),\n                    ctx.pipeline_id.clone(),'
if old_spawn in content:
    content = content.replace(old_spawn, new_spawn, 1)
    print("  + spawn_stage_wait usa ctx.process_id/pipeline_id")
elif "ctx.process_id.clone()" in content:
    print("  - spawn_stage_wait ya usa ctx")

if content != orig:
    open(path, "w").write(content)
PYEOF

# ─────────────────────────────────────────────────────────────────────────────
# 4. scheduler.rs — run_cycle + run_cycle_dry
# ─────────────────────────────────────────────────────────────────────────────
python3 - <<'PYEOF'
path = "src/rust/agent/src/scheduler.rs"
content = open(path).read()
orig = content

# Fix import
old_import = "        use super::NodeContext;"
new_import = "        use crate::nodes::NodeContext;"
content = content.replace(old_import, new_import)

# Add process_id/pipeline_id params to run_cycle if not present
if "process_id: &str," not in content:
    old_sig = "    pub async fn run_cycle(\n        &mut self,\n        pool: &PgPool,\n        dt: f64,\n        overrides: &HashMap<String, NodeAction>,\n    ) -> Result<HashMap<String, Signal>>"
    new_sig = "    pub async fn run_cycle(\n        &mut self,\n        pool: &PgPool,\n        dt: f64,\n        overrides: &HashMap<String, NodeAction>,\n        process_id: &str,\n        pipeline_id: &str,\n    ) -> Result<HashMap<String, Signal>>"
    if old_sig in content:
        content = content.replace(old_sig, new_sig, 1)
        print("  + run_cycle() params añadidos")
    else:
        print("  ! run_cycle() signature no encontrada — revisar manualmente")
else:
    print("  - run_cycle() ya tiene process_id/pipeline_id")

# Add NodeContext construction and pass to execute() in run_cycle
# Replace the execute call inside run_cycle
old_exec = "            let output = if node.is_actuator() {\n                if let Some(action) = overrides.get(node_id) {\n                    node.execute_override(action.clone()).await?\n                } else {\n                    node.execute(inputs, dt, pool).await?\n                }\n            } else {\n                node.execute(inputs, dt, pool).await?\n            };"

new_exec = """            let ns = node.save_state();
            let node_label = ns.data.get("label")
                .and_then(|v| v.as_str()).map(String::from);
            let ctx = crate::nodes::NodeContext {
                process_id:  process_id.to_string(),
                pipeline_id: pipeline_id.to_string(),
                node_id:     node_id.clone(),
                node_label,
            };
            let output = if node.is_actuator() {
                if let Some(action) = overrides.get(node_id) {
                    node.execute_override(action.clone()).await?
                } else {
                    node.execute(inputs, dt, pool, &ctx).await?
                }
            } else {
                node.execute(inputs, dt, pool, &ctx).await?
            };"""

if old_exec in content:
    content = content.replace(old_exec, new_exec, 1)
    print("  + run_cycle() execute calls actualizados")
elif "node.execute(inputs, dt, pool, &ctx)" in content:
    print("  - run_cycle() execute ya usa ctx")
else:
    print("  ! execute pattern en run_cycle no encontrado")

# Fix run_cycle_dry execute call (no ctx needed — use empty)
old_dry = "            let output = node.execute(inputs, dt, pool).await?;"
new_dry = """            let ctx = crate::nodes::NodeContext {
                process_id: String::new(), pipeline_id: String::new(),
                node_id: node_id.clone(), node_label: None,
            };
            let output = node.execute(inputs, dt, pool, &ctx).await?;"""
if old_dry in content:
    content = content.replace(old_dry, new_dry, 1)
    print("  + run_cycle_dry execute actualizado")

if content != orig:
    open(path, "w").write(content)
PYEOF

# ─────────────────────────────────────────────────────────────────────────────
# 5. main.rs — pasar process_id y pipeline_id a run_cycle
# ─────────────────────────────────────────────────────────────────────────────
python3 - <<'PYEOF'
path = "src/rust/agent/src/main.rs"
content = open(path).read()
orig = content

old_call = "                .run_cycle(pool, interval.as_secs_f64(), &ov_snapshot)"
new_call = "                .run_cycle(\n                        pool,\n                        interval.as_secs_f64(),\n                        &ov_snapshot,\n                        &shared.process_id.to_string(),\n                        &shared.pipeline_id,\n                    )"

if old_call in content:
    content = content.replace(old_call, new_call, 1)
    open(path, "w").write(content)
    print("  + main.rs run_cycle call actualizado")
elif "shared.process_id.to_string()" in content:
    print("  - main.rs ya tiene process_id en run_cycle")
else:
    print("  ! main.rs run_cycle call no encontrado — revisar manualmente")
PYEOF

echo ""
echo "=== Patch completo. Ahora: cargo build ==="
