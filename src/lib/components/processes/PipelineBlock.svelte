<!-- src/lib/components/processes/PipelineBlock.svelte -->
<!-- svelte-ignore a11y_label_has_associated_control -->
<script lang="ts">
  let {
    node,
    color,
    category,
    isExpanded,
    availableSensors,
    canEdit,
    onexpand,
    onremove,
    onchange,
    onstartdrag,
    onresize,
    inputPorts = [],
    outputPorts = [],
  }: {
    node: any;
    color: string;
    category: string;
    isExpanded: boolean;
    availableSensors: { id: string; label: string }[];
    canEdit: boolean;
    onexpand: () => void;
    onremove: () => void;
    onchange: () => void;
    onstartdrag?: (e: MouseEvent) => void;
    onresize?: (e: MouseEvent) => void;
    inputPorts?: string[];
    outputPorts?: string[];
  } = $props();

  function set(field: string, value: any) {
    node[field] = value;
    onchange();
  }

  function fmtType(t: string) {
    return t.replace(/_/g, ' ');
  }

  // Hints por tipo de nodo
  const HINTS: Record<string, string> = {
    postgres_sensor:  'Cada sensor que agregues se convierte en una dimensión del vector de salida. El orden importa si usás Select o Mahalanobis.',
    kalman:           'Q controla cuánto confiás en el modelo (bajo = más suave). R controla cuánto confiás en la medición (alto = más suave). Empezá con Q=1e-5, R=1e-3.',
    moving_avg:       'Promedia las últimas N muestras. Más simple que Kalman pero introduce un retardo de N/2 muestras. Útil para señales sin ruido abrupto.',
    ewma:             'Alpha cercano a 0 = muy suave pero lento. Alpha cercano a 1 = rápido pero ruidoso. Alpha=0.1 es un buen punto de partida.',
    lowpass:          'Tau es la constante de tiempo en segundos. La señal tarda ~3τ en seguir un escalón. Equivalente a EWMA con alpha = dt/(tau+dt).',
    passthrough:      'No modifica la señal. Útil para conectar nodos sin filtrado o para debug del pipeline.',
    concat:           'Une los vectores de todas sus entradas en uno solo. Si tenés dos sensores de dim=1 cada uno, la salida es dim=2.',
    weighted_mean:    'Los pesos deben coincidir en cantidad con las dimensiones de entrada. Si los pesos no suman 1, la salida estará sesgada.',
    mahalanobis:      'La distancia de Mahalanobis mide qué tan lejos está la señal del vector objetivo en unidades de desviación estándar. threshold_act > threshold_deact genera histéresis.',
    hysteresis:       'Enciende cuando la señal baja de "low" y apaga cuando sube de "high". La banda muerta (high-low) evita que el actuador oscile con señales ruidosas.',
    sprt:             'Acumula evidencia estadística antes de decidir. Más robusto que histéresis para señales con ruido gaussiano. alpha y beta controlan la tasa de error.',
    actuator:         'Nodo unificado: decisor + actuador. Seleccioná el método de decisión (Hysteresis/Mahalanobis/SPRT), el protocolo (MQTT/HTTP) y las etapas de confirmación opcionales.',
    mqtt_actuator:    'Publica el payload en el topic cuando la señal decide ON u OFF. El broker se configura en la sección de conexión compartida abajo.',
    http_actuator:    'Hace POST a path_on cuando la señal es ON y a path_off cuando es OFF. El base_url se configura en la conexión compartida.',
    mqtt_subscriber:  'Lee el estado del actuador desde un topic MQTT para dárselo al Watchdog como feedback. Conectar al puerto "feedback" del Watchdog con un edge de tipo Feedback.',
    watchdog:         'Good: verifica que el actuador reportó el estado correcto vía MQTT. Bad: verifica que la señal cambió en la dirección esperada. Ugly: pide confirmación manual antes de actuar.',
    logger:           'Registra la señal en process_readings con el tag dado. El Monitor Tab usa estos tags para mostrar las gráficas.',
    select:           'Extrae las dimensiones indicadas del vector. Índices base 0. Por ejemplo [0,2] de un vector dim=3 produce un vector dim=2.',
    linear_scale:     'Aplica y = a·x + b componente a componente. Útil para convertir unidades (ej: voltios a humedad) o normalizar señales.',
  };
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="block-inner" style="--nc:{color}">

  <!-- Header — drag handle + expand/remove -->
  <div class="block-header"
    ondblclick={onexpand}
    onmousedown={(e) => {
      if ((e.target as HTMLElement).closest('button')) return;
      onstartdrag?.(e);
    }}
    style="cursor:grab"
  >
    <span class="drag-handle">⠿</span>
    <span class="cat">{category}</span>
    <span class="type-label">{fmtType(node.type)}</span>
    <div class="header-actions">
      <button class="btn-expand" onclick={onexpand} title={isExpanded ? 'colapsar' : 'expandir'}>
        {isExpanded ? '▲' : '▼'}
      </button>
      {#if canEdit}
        <button class="btn-remove" onclick={onremove} title="eliminar">✕</button>
      {/if}
    </div>
  </div>

  <!-- Port names row — visible when collapsed -->
  {#if !isExpanded && ((inputPorts?.length ?? 0) > 0 || (outputPorts?.length ?? 0) > 0)}
    <div class="port-names-row">
      <div class="port-names-side port-names-in">
        {#each (inputPorts ?? []) as p (p)}
          <span class="pn pn--in">{p}</span>
        {/each}
      </div>
      <div class="port-names-side port-names-out">
        {#each (outputPorts ?? []) as p (p)}
          <span class="pn pn--out">{p}</span>
        {/each}
      </div>
    </div>
  {/if}

  <!-- Expanded body -->
  {#if isExpanded}
    <div class="block-body">

      <!-- ── postgres_sensor ── -->
      {#if node.type === 'postgres_sensor'}
        <div class="field-group">
          <div class="field-label">sensores</div>
          {#each (node.sensors ?? []) as s, i (i)}
            <div class="sensor-row">
              <select
                class="inp inp--select"
                value={s.id}
                onchange={(e) => {
                  const val = (e.target as HTMLSelectElement).value;
                  const found = availableSensors.find(x => x.id === val);
                  const sensors = [...(node.sensors ?? [])];
                  sensors[i] = { id: val, label: found?.label?.split('·')[2]?.trim() ?? '' };
                  set('sensors', sensors);
                }}>
                <option value="">— elegir —</option>
                {#each availableSensors as opt (opt.id)}
                  <option value={opt.id}>{opt.label}</option>
                {/each}
              </select>
              <input class="inp inp--label" value={s.label}
                oninput={(e) => {
                  const sensors = [...(node.sensors ?? [])];
                  sensors[i] = { ...sensors[i], label: (e.target as HTMLInputElement).value };
                  set('sensors', sensors);
                }} placeholder="etiqueta" />
              {#if canEdit}
                <button class="btn-sm" onclick={() => set('sensors', (node.sensors ?? []).filter((_: any, j: number) => j !== i))}>✕</button>
              {/if}
            </div>
          {/each}
          {#if canEdit}
            <button class="btn-add" onclick={() => set('sensors', [...(node.sensors ?? []), { id: '', label: '' }])}>+ sensor</button>
          {/if}
        </div>

      <!-- ── kalman ── -->
      {:else if node.type === 'kalman'}
        <div class="field-row">
          <div class="field">
            <label>Q</label>
            <input class="inp mono" value={node.Q ?? 1e-5}
              oninput={(e) => set('Q', parseFloat((e.target as HTMLInputElement).value))} />
          </div>
          <div class="field">
            <label>R</label>
            <input class="inp mono" value={node.R ?? 1e-3}
              oninput={(e) => set('R', parseFloat((e.target as HTMLInputElement).value))} />
          </div>
          <div class="field">
            <label>P₀</label>
            <input class="inp mono" value={node.P0 ?? 1.0}
              oninput={(e) => set('P0', parseFloat((e.target as HTMLInputElement).value))} />
          </div>
          <div class="field">
            <label>warmup</label>
            <input class="inp mono" type="number" value={node.warmup_samples ?? 10}
              oninput={(e) => set('warmup_samples', parseInt((e.target as HTMLInputElement).value))} />
          </div>
        </div>
        <div class="field">
          <label>convergence threshold</label>
          <input class="inp mono" value={node.convergence_threshold ?? 5e-4}
            oninput={(e) => set('convergence_threshold', parseFloat((e.target as HTMLInputElement).value))} />
        </div>

      <!-- ── moving_avg ── -->
      {:else if node.type === 'moving_avg'}
        <div class="field-row">
          <div class="field">
            <label>ventana (N)</label>
            <input class="inp mono" type="number" value={node.window_n ?? 10}
              oninput={(e) => set('window_n', parseInt((e.target as HTMLInputElement).value))} />
          </div>
          <div class="field">
            <label>warmup</label>
            <input class="inp mono" type="number" value={node.warmup_samples ?? 10}
              oninput={(e) => set('warmup_samples', parseInt((e.target as HTMLInputElement).value))} />
          </div>
        </div>

      <!-- ── ewma ── -->
      {:else if node.type === 'ewma'}
        <div class="field-row">
          <div class="field">
            <label>alpha (0–1)</label>
            <input class="inp mono" type="number" step="0.01" min="0.01" max="0.99" value={node.alpha ?? 0.1}
              oninput={(e) => set('alpha', parseFloat((e.target as HTMLInputElement).value))} />
          </div>
          <div class="field">
            <label>warmup</label>
            <input class="inp mono" type="number" value={node.warmup_samples ?? 10}
              oninput={(e) => set('warmup_samples', parseInt((e.target as HTMLInputElement).value))} />
          </div>
        </div>

      <!-- ── lowpass ── -->
      {:else if node.type === 'lowpass'}
        <div class="field-row">
          <div class="field">
            <label>tau (segundos)</label>
            <input class="inp mono" type="number" step="1" min="1" value={node.tau_seconds ?? 120}
              oninput={(e) => set('tau_seconds', parseFloat((e.target as HTMLInputElement).value))} />
          </div>
          <div class="field">
            <label>warmup</label>
            <input class="inp mono" type="number" value={node.warmup_samples ?? 10}
              oninput={(e) => set('warmup_samples', parseInt((e.target as HTMLInputElement).value))} />
          </div>
        </div>

      <!-- ── weighted_mean ── -->
      {:else if node.type === 'weighted_mean'}
        <div class="field">
          <label>pesos <span class="hint">separados por comas, deben sumar 1.0</span></label>
          <input class="inp mono"
            value={(node.weights ?? []).join(', ')}
            oninput={(e) => set('weights',
              (e.target as HTMLInputElement).value.split(',')
                .map((v: string) => parseFloat(v.trim()))
                .filter((v: number) => !isNaN(v))
            )} placeholder="0.5, 0.3, 0.2" />
        </div>

      <!-- ── mahalanobis ── -->
      {:else if node.type === 'mahalanobis'}
        <div class="field">
          <label>target <span class="hint">separado por comas</span></label>
          <input class="inp mono"
            value={(node.target ?? []).join(', ')}
            oninput={(e) => set('target',
              (e.target as HTMLInputElement).value.split(',')
                .map((v: string) => parseFloat(v.trim()))
                .filter((v: number) => !isNaN(v))
            )} />
        </div>
        <div class="field-row">
          <div class="field">
            <label>threshold act</label>
            <input class="inp mono" type="number" step="0.1" value={node.threshold_act ?? 2.5}
              oninput={(e) => set('threshold_act', parseFloat((e.target as HTMLInputElement).value))} />
          </div>
          <div class="field">
            <label>threshold deact</label>
            <input class="inp mono" type="number" step="0.1" value={node.threshold_deact ?? 1.0}
              oninput={(e) => set('threshold_deact', parseFloat((e.target as HTMLInputElement).value))} />
          </div>
        </div>
        <div class="field checkbox-field">
          <input type="checkbox" id="use-P-{node.id}" checked={node.use_kalman_P ?? true}
            onchange={(e) => set('use_kalman_P', (e.target as HTMLInputElement).checked)} />
          <label for="use-P-{node.id}">usar P del Kalman como Σ</label>
        </div>

      <!-- ── hysteresis ── -->
      {:else if node.type === 'hysteresis'}
        <div class="field-row">
          <div class="field">
            <label>reducción</label>
            <select class="inp" value={node.reduction?.type ?? 'mean'}
              onchange={(e) => set('reduction', { type: (e.target as HTMLSelectElement).value })}>
              <option value="mean">media</option>
              <option value="min">mínimo</option>
              <option value="max">máximo</option>
              <option value="weighted_by_p">pond. P</option>
            </select>
          </div>
          <div class="field">
            <label>low</label>
            <input class="inp mono" type="number" step="0.001" value={node.low ?? 0.08}
              oninput={(e) => set('low', parseFloat((e.target as HTMLInputElement).value))} />
          </div>
          <div class="field">
            <label>high</label>
            <input class="inp mono" type="number" step="0.001" value={node.high ?? 0.085}
              oninput={(e) => set('high', parseFloat((e.target as HTMLInputElement).value))} />
          </div>
        </div>
        <div class="field-row">
          <div class="field">
            <label>acción &lt; low</label>
            <select class="inp" value={node.action_below_low ?? 'on'}
              onchange={(e) => set('action_below_low', (e.target as HTMLSelectElement).value)}>
              <option value="on">ON</option>
              <option value="off">OFF</option>
              <option value="hold">HOLD</option>
            </select>
          </div>
          <div class="field">
            <label>acción &gt; high</label>
            <select class="inp" value={node.action_above_high ?? 'off'}
              onchange={(e) => set('action_above_high', (e.target as HTMLSelectElement).value)}>
              <option value="on">ON</option>
              <option value="off">OFF</option>
              <option value="hold">HOLD</option>
            </select>
          </div>
        </div>

      <!-- ── sprt ── -->
      {:else if node.type === 'sprt'}
        <div class="field-row">
          <div class="field">
            <label>μ H₀ (normal)</label>
            <input class="inp mono" type="number" step="0.01" value={node.mu_H0 ?? 0.0}
              oninput={(e) => set('mu_H0', parseFloat((e.target as HTMLInputElement).value))} />
          </div>
          <div class="field">
            <label>μ H₁ (anómalo)</label>
            <input class="inp mono" type="number" step="0.01" value={node.mu_H1 ?? 1.0}
              oninput={(e) => set('mu_H1', parseFloat((e.target as HTMLInputElement).value))} />
          </div>
          <div class="field">
            <label>σ</label>
            <input class="inp mono" type="number" step="0.01" value={node.sigma ?? 0.1}
              oninput={(e) => set('sigma', parseFloat((e.target as HTMLInputElement).value))} />
          </div>
        </div>
        <div class="field-row">
          <div class="field">
            <label>α (falso positivo)</label>
            <input class="inp mono" type="number" step="0.01" min="0.01" max="0.2" value={node.alpha ?? 0.05}
              oninput={(e) => set('alpha', parseFloat((e.target as HTMLInputElement).value))} />
          </div>
          <div class="field">
            <label>β (falso negativo)</label>
            <input class="inp mono" type="number" step="0.01" min="0.01" max="0.2" value={node.beta ?? 0.05}
              oninput={(e) => set('beta', parseFloat((e.target as HTMLInputElement).value))} />
          </div>
        </div>
        <div class="field-row">
          <div class="field">
            <label>reducción</label>
            <select class="inp" value={node.reduction?.type ?? 'mean'}
              onchange={(e) => set('reduction', { type: (e.target as HTMLSelectElement).value })}>
              <option value="mean">media</option>
              <option value="min">mínimo</option>
              <option value="max">máximo</option>
            </select>
          </div>
          <div class="field checkbox-field" style="justify-content:flex-end; padding-top:14px">
            <input type="checkbox" id="sprt-reset-{node.id}" checked={node.reset_on_action ?? true}
              onchange={(e) => set('reset_on_action', (e.target as HTMLInputElement).checked)} />
            <label for="sprt-reset-{node.id}">reset al actuar</label>
          </div>
        </div>

      <!-- ── mqtt_actuator ── -->
      {:else if node.type === 'mqtt_actuator'}
        <div class="field">
          <label>nombre descriptivo <span class="hint">se muestra en el monitor</span></label>
          <input class="inp" value={node.label ?? ''}
            oninput={(e) => set('label', (e.target as HTMLInputElement).value)}
            placeholder="ej: Válvula zona norte" />
        </div>
        <div class="field">
          <label>topic</label>
          <input class="inp mono" value={node.topic ?? ''}
            oninput={(e) => set('topic', (e.target as HTMLInputElement).value)}
            placeholder="relay/control" />
        </div>
        <div class="field-row">
          <div class="field">
            <label>payload ON</label>
            <input class="inp mono" value={node.payload_on ?? 'on'}
              oninput={(e) => set('payload_on', (e.target as HTMLInputElement).value)} />
          </div>
          <div class="field">
            <label>payload OFF</label>
            <input class="inp mono" value={node.payload_off ?? 'off'}
              oninput={(e) => set('payload_off', (e.target as HTMLInputElement).value)} />
          </div>
        </div>
        <div class="actuator-switch">
          <span class="switch-label">override</span>
          <button class="sw-btn sw-on">ON</button>
          <button class="sw-btn sw-off">OFF</button>
          <button class="sw-btn sw-auto">↺ auto</button>
        </div>

      <!-- ── http_actuator ── -->
      {:else if node.type === 'http_actuator'}
        <div class="field-row">
          <div class="field">
            <label>path ON</label>
            <input class="inp mono" value={node.path_on ?? '/on'}
              oninput={(e) => set('path_on', (e.target as HTMLInputElement).value)} />
          </div>
          <div class="field">
            <label>path OFF</label>
            <input class="inp mono" value={node.path_off ?? '/off'}
              oninput={(e) => set('path_off', (e.target as HTMLInputElement).value)} />
          </div>
        </div>

      <!-- ── actuator (nodo unificado) ── -->
      {:else if node.type === 'actuator'}

        <div class="field">
          <label>nombre <span class="hint">se muestra en el monitor</span></label>
          <input class="inp" value={node.label ?? ''}
            oninput={(e) => set('label', (e.target as HTMLInputElement).value)}
            placeholder="Válvula zona norte" />
        </div>

        <!-- Método de decisión -->
        <div class="subpanel-title">Decisión</div>
        <div class="field">
          <label>método</label>
          <select class="inp" value={node.decision?.method ?? 'hysteresis'}
            onchange={(e) => {
              const m = (e.target as HTMLSelectElement).value;
              if (m === 'hysteresis')    set('decision', { method:'hysteresis', reduction:{type:'mean'}, low:0.08, high:0.085, action_below:'on', action_above:'off' });
              if (m === 'mahalanobis')   set('decision', { method:'mahalanobis', target:[], threshold_act:2.5, threshold_deact:1.0 });
              if (m === 'sprt')          set('decision', { method:'sprt', mu_h0:0.0, mu_h1:1.0, sigma:0.1, alpha:0.05, beta:0.05, reduction:{type:'mean'}, reset_on_action:true });
              if (m === 'robust_group')  set('decision', { method:'robust_group', low:0.38, high:0.45, central_method:'weighted', reference_index:0, outlier_k:2.0, max_spread_ratio:0.3, confirmation_cycles:3, action_below:'on', action_above:'off' });
            }}>
            <option value="hysteresis">Hysteresis</option>
            <option value="mahalanobis">Mahalanobis</option>
            <option value="sprt">SPRT</option>
            <option value="robust_group">Grupo robusto</option>
          </select>
        </div>

        {#if node.decision?.method === 'hysteresis'}
          <div class="subpanel">
            <div class="field-row">
              <div class="field">
                <label>low</label>
                <input class="inp mono" type="number" step="0.001"
                  value={node.decision.low ?? 0.08}
                  oninput={(e) => set('decision', { ...node.decision, low: parseFloat((e.target as HTMLInputElement).value) })} />
              </div>
              <div class="field">
                <label>high</label>
                <input class="inp mono" type="number" step="0.001"
                  value={node.decision.high ?? 0.085}
                  oninput={(e) => set('decision', { ...node.decision, high: parseFloat((e.target as HTMLInputElement).value) })} />
              </div>
            </div>
            <div class="field-row">
              <div class="field">
                <label>acción &lt; low</label>
                <select class="inp" value={node.decision.action_below ?? 'on'}
                  onchange={(e) => set('decision', { ...node.decision, action_below: (e.target as HTMLSelectElement).value })}>
                  <option value="on">ON</option><option value="off">OFF</option>
                </select>
              </div>
              <div class="field">
                <label>acción &gt; high</label>
                <select class="inp" value={node.decision.action_above ?? 'off'}
                  onchange={(e) => set('decision', { ...node.decision, action_above: (e.target as HTMLSelectElement).value })}>
                  <option value="on">ON</option><option value="off">OFF</option>
                </select>
              </div>
            </div>
          </div>
        {/if}

        {#if node.decision?.method === 'robust_group'}
          <div class="subpanel">
            <!-- Umbrales -->
            <div class="field-row">
              <div class="field">
                <label>low</label>
                <input class="inp mono" type="number" step="0.001"
                  value={node.decision.low ?? 0.38}
                  oninput={(e) => set('decision', { ...node.decision, low: parseFloat((e.target as HTMLInputElement).value) })} />
              </div>
              <div class="field">
                <label>high</label>
                <input class="inp mono" type="number" step="0.001"
                  value={node.decision.high ?? 0.45}
                  oninput={(e) => set('decision', { ...node.decision, high: parseFloat((e.target as HTMLInputElement).value) })} />
              </div>
            </div>
            <!-- Método central -->
            <div class="field">
              <label>valor central</label>
              <select class="inp" value={node.decision.central_method ?? 'weighted'}
                onchange={(e) => set('decision', { ...node.decision, central_method: (e.target as HTMLSelectElement).value })}>
                <option value="weighted">Media ponderada por P (recomendado)</option>
                <option value="median">Mediana</option>
                <option value="mean">Media simple</option>
                <option value="reference">Sensor de referencia</option>
              </select>
            </div>
            {#if node.decision.central_method === 'reference'}
              <div class="field">
                <label>índice de referencia <span class="hint">0 = primer sensor</span></label>
                <input class="inp mono" type="number" min="0" step="1"
                  value={node.decision.reference_index ?? 0}
                  oninput={(e) => set('decision', { ...node.decision, reference_index: parseInt((e.target as HTMLInputElement).value) })} />
              </div>
            {/if}
            <!-- Outlier rejection -->
            <div class="field-row">
              <div class="field">
                <label>outlier k <span class="hint">σ de Kalman, vacío = off</span></label>
                <input class="inp mono" type="number" step="0.1" min="0"
                  value={node.decision.outlier_k ?? ''}
                  placeholder="2.0"
                  oninput={(e) => {
                    const v = parseFloat((e.target as HTMLInputElement).value);
                    set('decision', { ...node.decision, outlier_k: isNaN(v) ? null : v });
                  }} />
              </div>
              <div class="field">
                <label>spread máx <span class="hint">fracción, vacío = off</span></label>
                <input class="inp mono" type="number" step="0.05" min="0" max="1"
                  value={node.decision.max_spread_ratio ?? ''}
                  placeholder="0.30"
                  oninput={(e) => {
                    const v = parseFloat((e.target as HTMLInputElement).value);
                    set('decision', { ...node.decision, max_spread_ratio: isNaN(v) ? null : v });
                  }} />
              </div>
            </div>
            <!-- Confirmation window -->
            <div class="field">
              <label>ciclos de confirmación <span class="hint">N ciclos consecutivos bajo/sobre umbral</span></label>
              <input class="inp mono" type="number" min="1" max="20" step="1"
                value={node.decision.confirmation_cycles ?? 3}
                oninput={(e) => set('decision', { ...node.decision, confirmation_cycles: parseInt((e.target as HTMLInputElement).value) })} />
            </div>
            <!-- Acciones -->
            <div class="field-row">
              <div class="field">
                <label>acción &lt; low</label>
                <select class="inp" value={node.decision.action_below ?? 'on'}
                  onchange={(e) => set('decision', { ...node.decision, action_below: (e.target as HTMLSelectElement).value })}>
                  <option value="on">ON</option><option value="off">OFF</option>
                </select>
              </div>
              <div class="field">
                <label>acción &gt; high</label>
                <select class="inp" value={node.decision.action_above ?? 'off'}
                  onchange={(e) => set('decision', { ...node.decision, action_above: (e.target as HTMLSelectElement).value })}>
                  <option value="on">ON</option><option value="off">OFF</option>
                </select>
              </div>
            </div>
          </div>
        {/if}

        {#if node.decision?.method === 'mahalanobis'}
          <div class="subpanel">
            <div class="field">
              <label>target <span class="hint">vector separado por comas</span></label>
              <input class="inp mono" value={(node.decision.target ?? []).join(',')}
                oninput={(e) => set('decision', { ...node.decision, target: (e.target as HTMLInputElement).value.split(',').map(Number).filter(isFinite) })}
                placeholder="0.5,0.5" />
            </div>
            <div class="field-row">
              <div class="field">
                <label>umbral activar</label>
                <input class="inp mono" type="number" step="0.1" value={node.decision.threshold_act ?? 2.5}
                  oninput={(e) => set('decision', { ...node.decision, threshold_act: parseFloat((e.target as HTMLInputElement).value) })} />
              </div>
              <div class="field">
                <label>umbral desactivar</label>
                <input class="inp mono" type="number" step="0.1" value={node.decision.threshold_deact ?? 1.0}
                  oninput={(e) => set('decision', { ...node.decision, threshold_deact: parseFloat((e.target as HTMLInputElement).value) })} />
              </div>
            </div>
          </div>
        {/if}

        {#if node.decision?.method === 'sprt'}
          <div class="subpanel">
            <div class="field-row">
              <div class="field"><label>μ H0</label>
                <input class="inp mono" type="number" step="0.01" value={node.decision.mu_h0 ?? 0}
                  oninput={(e) => set('decision', { ...node.decision, mu_h0: parseFloat((e.target as HTMLInputElement).value) })} /></div>
              <div class="field"><label>μ H1</label>
                <input class="inp mono" type="number" step="0.01" value={node.decision.mu_h1 ?? 1}
                  oninput={(e) => set('decision', { ...node.decision, mu_h1: parseFloat((e.target as HTMLInputElement).value) })} /></div>
              <div class="field"><label>σ</label>
                <input class="inp mono" type="number" step="0.01" value={node.decision.sigma ?? 0.1}
                  oninput={(e) => set('decision', { ...node.decision, sigma: parseFloat((e.target as HTMLInputElement).value) })} /></div>
            </div>
            <div class="field-row">
              <div class="field"><label>α (FP)</label>
                <input class="inp mono" type="number" step="0.01" value={node.decision.alpha ?? 0.05}
                  oninput={(e) => set('decision', { ...node.decision, alpha: parseFloat((e.target as HTMLInputElement).value) })} /></div>
              <div class="field"><label>β (FN)</label>
                <input class="inp mono" type="number" step="0.01" value={node.decision.beta ?? 0.05}
                  oninput={(e) => set('decision', { ...node.decision, beta: parseFloat((e.target as HTMLInputElement).value) })} /></div>
            </div>
          </div>
        {/if}

        <!-- Protocolo de salida -->
        <div class="subpanel-title">Salida</div>
        <div class="field">
          <label>protocolo</label>
          <select class="inp" value={node.output?.protocol ?? 'mqtt'}
            onchange={(e) => {
              const p = (e.target as HTMLSelectElement).value;
              if (p === 'mqtt') set('output', { protocol:'mqtt', connection:'shared', topic:'', payload_on:'on,1', payload_off:'off,1' });
              if (p === 'http') set('output', { protocol:'http', connection:'shared', path_on:'/on', path_off:'/off' });
            }}>
            <option value="mqtt">MQTT</option>
            <option value="http">HTTP</option>
          </select>
        </div>

        {#if node.output?.protocol === 'mqtt'}
          <div class="subpanel">
            <div class="field">
              <label>broker <span class="hint">"shared" o URL inline mqtt://host:port</span></label>
              <input class="inp mono" value={typeof node.output.connection === 'string' ? node.output.connection : JSON.stringify(node.output.connection)}
                oninput={(e) => {
                  const v = (e.target as HTMLInputElement).value.trim();
                  set('output', { ...node.output, connection: v.startsWith('{') ? JSON.parse(v) : v });
                }}
                placeholder="shared" />
            </div>
            <div class="field">
              <label>topic</label>
              <input class="inp mono" value={node.output.topic ?? ''}
                oninput={(e) => set('output', { ...node.output, topic: (e.target as HTMLInputElement).value })}
                placeholder="control/valvula" />
            </div>
            <div class="field-row">
              <div class="field">
                <label>payload ON</label>
                <input class="inp mono" value={node.output.payload_on ?? 'on,1'}
                  oninput={(e) => set('output', { ...node.output, payload_on: (e.target as HTMLInputElement).value })} />
              </div>
              <div class="field">
                <label>payload OFF</label>
                <input class="inp mono" value={node.output.payload_off ?? 'off,1'}
                  oninput={(e) => set('output', { ...node.output, payload_off: (e.target as HTMLInputElement).value })} />
              </div>
            </div>
            <div class="field">
              <label>ack topic <span class="hint">topic donde llegan las confirmaciones. Vacío = sin ACK</span></label>
              <input class="inp mono" value={node.output.ack_topic ?? ''}
                oninput={(e) => {
                  const v = (e.target as HTMLInputElement).value.trim();
                  set('output', { ...node.output, ack_topic: v || null });
                }}
                placeholder="ack/valvula" />
            </div>
          </div>
        {/if}

        {#if node.output?.protocol === 'http'}
          <div class="subpanel">
            <div class="field-row">
              <div class="field">
                <label>path ON</label>
                <input class="inp mono" value={node.output.path_on ?? '/on'}
                  oninput={(e) => set('output', { ...node.output, path_on: (e.target as HTMLInputElement).value })} />
              </div>
              <div class="field">
                <label>path OFF</label>
                <input class="inp mono" value={node.output.path_off ?? '/off'}
                  oninput={(e) => set('output', { ...node.output, path_off: (e.target as HTMLInputElement).value })} />
              </div>
            </div>
          </div>
        {/if}

        <!-- Etapas de confirmación -->
        <div class="subpanel-title">
          Etapas de confirmación
          <span class="hint">opcional — requiere ack topic en MQTT</span>
        </div>
        {#each (node.stages ?? []) as stage, si (si)}
          <div class="subpanel stage-row">
            <div class="field-row">
              <div class="field">
                <label>nombre</label>
                <input class="inp" value={stage.name ?? ''}
                  oninput={(e) => {
                    const stages = [...(node.stages ?? [])];
                    stages[si] = { ...stage, name: (e.target as HTMLInputElement).value };
                    set('stages', stages);
                  }} placeholder="Gateway" />
              </div>
              <div class="field">
                <label>timeout (s)</label>
                <input class="inp mono" type="number" min="1" value={stage.timeout_secs ?? 5}
                  oninput={(e) => {
                    const stages = [...(node.stages ?? [])];
                    stages[si] = { ...stage, timeout_secs: parseFloat((e.target as HTMLInputElement).value) };
                    set('stages', stages);
                  }} />
              </div>
              <button class="btn-remove-field"
                onclick={() => set('stages', (node.stages ?? []).filter((_: any, j: number) => j !== si))}>✕</button>
            </div>
            <div class="field">
              <label>match prefix <span class="hint">{'{action}'} {'{valve}'} {'{payload}'}</span></label>
              <input class="inp mono" value={stage.match_prefix ?? ''}
                oninput={(e) => {
                  const stages = [...(node.stages ?? [])];
                  stages[si] = { ...stage, match_prefix: (e.target as HTMLInputElement).value || null };
                  set('stages', stages);
                }} placeholder={"MQTT_RECIBIDO:{action},{valve}"} />
            </div>
            <div class="field-row">
              <div class="field">
                <label>error prefix</label>
                <input class="inp mono" value={stage.error_prefix ?? ''}
                  oninput={(e) => {
                    const stages = [...(node.stages ?? [])];
                    stages[si] = { ...stage, error_prefix: (e.target as HTMLInputElement).value || null };
                    set('stages', stages);
                  }} placeholder="LORA_ERROR" />
              </div>
              <div class="field" style="flex:0;min-width:80px">
                <label>terminal</label>
                <input type="checkbox" checked={stage.terminal ?? false}
                  onchange={(e) => {
                    const stages = [...(node.stages ?? [])];
                    stages[si] = { ...stage, terminal: (e.target as HTMLInputElement).checked };
                    set('stages', stages);
                  }} />
              </div>
            </div>
          </div>
        {/each}
        <button class="btn-add-field"
          onclick={() => set('stages', [...(node.stages ?? []), { name:'', match_prefix:null, error_prefix:null, timeout_secs:5, terminal:false }])}>
          + etapa
        </button>

        <!-- Coherence check -->
        <div class="subpanel-title">
          Coherencia sensor↔actuación
          <span class="hint">opcional — alerta si el sensor no responde</span>
        </div>
        {#if node.coherence}
          <div class="subpanel">
            <div class="field-row">
              <div class="field">
                <label>tendencia ON</label>
                <select class="inp" value={node.coherence.expected_on ?? 'ascending'}
                  onchange={(e) => set('coherence', { ...node.coherence, expected_on: (e.target as HTMLSelectElement).value })}>
                  <option value="ascending">↑ sube</option>
                  <option value="descending">↓ baja</option>
                  <option value="stable">— estable</option>
                </select>
              </div>
              <div class="field">
                <label>tendencia OFF</label>
                <select class="inp" value={node.coherence.expected_off ?? 'descending'}
                  onchange={(e) => set('coherence', { ...node.coherence, expected_off: (e.target as HTMLSelectElement).value })}>
                  <option value="ascending">↑ sube</option>
                  <option value="descending">↓ baja</option>
                  <option value="stable">— estable</option>
                </select>
              </div>
            </div>
            <div class="field-row">
              <div class="field">
                <label>delta mínimo</label>
                <input class="inp mono" type="number" step="0.001" value={node.coherence.min_delta ?? 0.02}
                  oninput={(e) => set('coherence', { ...node.coherence, min_delta: parseFloat((e.target as HTMLInputElement).value) })} />
              </div>
              <div class="field">
                <label>ventana (s)</label>
                <input class="inp mono" type="number" step="10" value={node.coherence.window_secs ?? 300}
                  oninput={(e) => set('coherence', { ...node.coherence, window_secs: parseFloat((e.target as HTMLInputElement).value) })} />
              </div>
              <div class="field" style="flex:0;min-width:70px">
                <label>relativo %</label>
                <input type="checkbox" checked={node.coherence.relative ?? false}
                  onchange={(e) => set('coherence', { ...node.coherence, relative: (e.target as HTMLInputElement).checked })} />
              </div>
            </div>
            <div class="field">
              <label>texto de alerta</label>
              <input class="inp" value={node.coherence.alert_label ?? ''}
                oninput={(e) => set('coherence', { ...node.coherence, alert_label: (e.target as HTMLInputElement).value || null })}
                placeholder="Sensor no respondió al riego" />
            </div>

            <!-- Consulta MQTT de estado -->
            <div class="field">
              <label>consulta de estado <span class="hint">topic donde publicar ASK,N</span></label>
              <input class="inp mono" value={node.coherence.query_topic ?? ''}
                oninput={(e) => set('coherence', { ...node.coherence, query_topic: (e.target as HTMLInputElement).value || null })}
                placeholder="ej. control/valvula" />
            </div>
            {#if node.coherence.query_topic}
              <div class="field-row">
                <div class="field">
                  <label>válvula <span class="hint">número para ASK,N</span></label>
                  <input class="inp mono" value={node.coherence.query_valve ?? ''}
                    oninput={(e) => set('coherence', { ...node.coherence, query_valve: (e.target as HTMLInputElement).value || null })}
                    placeholder="1" />
                </div>
                <div class="field">
                  <label>timeout respuesta (s)</label>
                  <input class="inp mono" type="number" step="1" min="1"
                    value={node.coherence.query_timeout_secs ?? 10}
                    oninput={(e) => set('coherence', { ...node.coherence, query_timeout_secs: parseFloat((e.target as HTMLInputElement).value) })} />
                </div>
              </div>
            {/if}
            </div>
            <button class="btn-remove-field" onclick={() => set('coherence', null)}>quitar coherencia</button>
          </div>
        {:else}
          <button class="btn-add-field" onclick={() => set('coherence', { expected_on:'ascending', expected_off:'descending', min_delta:0.02, window_secs:300, relative:false })}>
            + configurar coherencia
          </button>
        {/if}

      <!-- ── mqtt_subscriber ── -->
      {:else if node.type === 'mqtt_subscriber'}
        <div class="field">
          <label>topic</label>
          <input class="inp mono" value={node.topic ?? ''}
            oninput={(e) => set('topic', (e.target as HTMLInputElement).value)}
            placeholder="relay/estado" />
        </div>
        <div class="field-row">
          <div class="field">
            <label>payload ON</label>
            <input class="inp mono" value={node.payload_on ?? 'on'}
              oninput={(e) => set('payload_on', (e.target as HTMLInputElement).value)} />
          </div>
          <div class="field">
            <label>payload OFF</label>
            <input class="inp mono" value={node.payload_off ?? 'off'}
              oninput={(e) => set('payload_off', (e.target as HTMLInputElement).value)} />
          </div>
        </div>
        <div class="field">
          <label>stale after <span class="hint">segundos sin mensaje → Hold. Vacío = nunca expira</span></label>
          <input class="inp mono" type="number" min="0"
            value={node.stale_after_secs ?? ''}
            oninput={(e) => {
              const v = (e.target as HTMLInputElement).value;
              set('stale_after_secs', v === '' ? null : parseInt(v));
            }} placeholder="120" />
        </div>

      <!-- ── watchdog ── -->
      {:else if node.type === 'watchdog'}
        <div class="field">
          <label>actuator_id <span class="hint">ID del nodo actuador que protege</span></label>
          <input class="inp mono" value={node.actuator_id ?? ''}
            oninput={(e) => set('actuator_id', (e.target as HTMLInputElement).value)}
            placeholder="mqtt_actuator_..." />
        </div>
        <div class="field-row">
          <div class="field">
            <label>modo</label>
            <select class="inp" value={node.mode?.level ?? 'good'}
              onchange={(e) => {
                const level = (e.target as HTMLSelectElement).value;
                if (level === 'good')  set('mode', { level: 'good' });
                if (level === 'bad')   set('mode', { level: 'bad', expected_on_trend: 'ascending', expected_off_trend: 'descending', min_change_pct: 0.05, component: null });
                if (level === 'ugly')  set('mode', { level: 'ugly', notify_message: null });
              }}>
              <option value="good">Good — feedback MQTT</option>
              <option value="bad">Bad — tendencia de señal</option>
              <option value="ugly">Ugly — confirmación manual</option>
            </select>
          </div>
          <div class="field">
            <label>timeout (s)</label>
            <input class="inp mono" type="number" min="0" value={node.action_timeout_secs ?? 30}
              oninput={(e) => set('action_timeout_secs', parseInt((e.target as HTMLInputElement).value))} />
          </div>
          <div class="field">
            <label>max retries</label>
            <input class="inp mono" type="number" min="0" value={node.max_retries ?? 3}
              oninput={(e) => set('max_retries', parseInt((e.target as HTMLInputElement).value))} />
          </div>
        </div>

        {#if node.mode?.level === 'bad'}
          <div class="subpanel">
            <div class="field-row">
              <div class="field">
                <label>tendencia ON</label>
                <select class="inp" value={node.mode.expected_on_trend ?? 'ascending'}
                  onchange={(e) => set('mode', { ...node.mode, expected_on_trend: (e.target as HTMLSelectElement).value })}>
                  <option value="ascending">↑ ascendente</option>
                  <option value="descending">↓ descendente</option>
                  <option value="stable">— estable</option>
                </select>
              </div>
              <div class="field">
                <label>tendencia OFF</label>
                <select class="inp" value={node.mode.expected_off_trend ?? 'descending'}
                  onchange={(e) => set('mode', { ...node.mode, expected_off_trend: (e.target as HTMLSelectElement).value })}>
                  <option value="ascending">↑ ascendente</option>
                  <option value="descending">↓ descendente</option>
                  <option value="stable">— estable</option>
                </select>
              </div>
            </div>
            <div class="field-row">
              <div class="field">
                <label>cambio mínimo %</label>
                <input class="inp mono" type="number" step="0.01" min="0.01" max="1"
                  value={node.mode.min_change_pct ?? 0.05}
                  oninput={(e) => set('mode', { ...node.mode, min_change_pct: parseFloat((e.target as HTMLInputElement).value) })} />
              </div>
              <div class="field">
                <label>componente <span class="hint">vacío = norma L2</span></label>
                <input class="inp mono" type="number" min="0"
                  value={node.mode.component ?? ''}
                  oninput={(e) => {
                    const v = (e.target as HTMLInputElement).value;
                    set('mode', { ...node.mode, component: v === '' ? null : parseInt(v) });
                  }} placeholder="0" />
              </div>
            </div>
          </div>
        {/if}

        {#if node.mode?.level === 'ugly'}
          <div class="field">
            <label>mensaje de notificación <span class="hint">opcional</span></label>
            <input class="inp" value={node.mode.notify_message ?? ''}
              oninput={(e) => set('mode', { ...node.mode, notify_message: (e.target as HTMLInputElement).value || null })}
              placeholder="Verificar presión manual" />
          </div>
        {/if}

      <!-- ── logger ── -->
      {:else if node.type === 'logger'}
        <div class="field">
          <label>tag</label>
          <input class="inp mono" value={node.tag ?? ''}
            oninput={(e) => set('tag', (e.target as HTMLInputElement).value)}
            placeholder="humedad" />
        </div>

      <!-- ── select ── -->
      {:else if node.type === 'select'}
        <div class="field">
          <label>índices <span class="hint">base 0, separados por comas</span></label>
          <input class="inp mono"
            value={(node.indices ?? []).join(', ')}
            oninput={(e) => set('indices',
              (e.target as HTMLInputElement).value.split(',')
                .map((v: string) => parseInt(v.trim()))
                .filter((v: number) => !isNaN(v))
            )} placeholder="0, 2" />
        </div>

      <!-- ── linear_scale ── -->
      {:else if node.type === 'linear_scale'}
        <div class="field-row">
          <div class="field">
            <label>a (escala)</label>
            <input class="inp mono" type="number" step="0.01" value={node.a ?? 1.0}
              oninput={(e) => set('a', parseFloat((e.target as HTMLInputElement).value))} />
          </div>
          <div class="field">
            <label>b (offset)</label>
            <input class="inp mono" type="number" step="0.01" value={node.b ?? 0.0}
              oninput={(e) => set('b', parseFloat((e.target as HTMLInputElement).value))} />
          </div>
        </div>

      {:else}
        <div class="field">
          <span class="hint">tipo: {node.type}</span>
        </div>
      {/if}

      <!-- Hint explicativo -->
      {#if HINTS[node.type]}
        <div class="node-hint">{HINTS[node.type]}</div>
      {/if}

    </div>

    <!-- Resize handle -->
    {#if onresize}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="resize-handle"
        title="arrastrar para redimensionar"
        onmousedown={(e) => { e.stopPropagation(); onresize?.(e); }}
      >⌟</div>
    {/if}
  {/if}

</div>

<style>
  .block-inner {
    background: var(--bg-surface);
    border: 2px solid var(--nc);
    border-radius: 10px;
    min-width: 220px;
    overflow: visible;
    display: flex;
    flex-direction: column;
  }

  .block-header {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 10px;
    cursor: pointer;
    border-radius: 8px 8px 0 0;
    background: color-mix(in srgb, var(--nc) 8%, var(--bg-surface));
    flex-shrink: 0;
  }
  .block-inner:not(.expanded) .block-header { border-radius: 8px; }

  .cat {
    font-size: 8px;
    text-transform: uppercase;
    letter-spacing: .08em;
    color: var(--nc);
    font-family: 'DM Mono', monospace;
    flex-shrink: 0;
  }
  .type-label {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-primary);
    flex: 1;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .header-actions { display: flex; gap: 3px; flex-shrink: 0; }
  .btn-expand, .btn-remove {
    width: 18px; height: 18px; border: none; background: none;
    cursor: pointer; font-size: 10px; color: var(--text-muted);
    border-radius: 3px; padding: 0;
    display: flex; align-items: center; justify-content: center;
  }
  .drag-handle { font-size: 12px; color: var(--text-muted); opacity: 0.4; flex-shrink: 0; }
  .block-header:hover .drag-handle { opacity: 0.8; }
  .btn-expand:hover { background: var(--interactive-hover); }
  .btn-remove:hover { background: var(--error-bg); color: var(--error-color); }

  .port-names-row {
    display: flex;
    justify-content: space-between;
    padding: 4px 8px 6px;
    gap: 4px;
    border-top: 0.5px solid color-mix(in srgb, var(--nc) 15%, transparent);
    background: color-mix(in srgb, var(--nc) 4%, var(--bg-surface));
    border-radius: 0 0 8px 8px;
    min-height: 42px;
  }
  .port-names-side { display: flex; flex-direction: column; gap: 2px; }
  .port-names-out { align-items: flex-end; }
  .pn {
    font-size: 7.5px;
    font-family: 'DM Mono', monospace;
    padding: 1px 4px;
    border-radius: 2px;
    line-height: 1.3;
  }
  .pn--in  { background: #EFF6FF; color: #1D4ED8; }
  .pn--out { background: #F0FDF4; color: #166534; }

  .block-body {
    padding: 8px 10px;
    cursor: default;
    display: flex;
    flex-direction: column;
    gap: 6px;
    border-top: 1px solid color-mix(in srgb, var(--nc) 20%, transparent);
    flex: 1;
    overflow-y: auto;
  }

  .field { display: flex; flex-direction: column; gap: 2px; }
  .field-row { display: flex; gap: 6px; }
  .field-row .field { flex: 1; min-width: 0; }
  .field-group { display: flex; flex-direction: column; gap: 4px; }
  .field-label { font-size: 10px; color: var(--text-muted); font-weight: 500; }
  label { font-size: 10px; color: var(--text-muted); }
  .hint { font-size: 9px; color: var(--text-muted); font-weight: 400; }

  .inp {
    padding: 3px 6px;
    border: 0.5px solid var(--border-default);
    border-radius: 4px;
    background: var(--bg-elevated);
    color: var(--text-primary);
    font-size: 11px;
    outline: none;
    width: 100%;
    box-sizing: border-box;
  }
  .inp:focus { border-color: var(--nc); }
  .inp--select { font-size: 10px; }
  .inp--label { width: 72px; flex-shrink: 0; }
  .mono { font-family: 'DM Mono', monospace; }

  .sensor-row { display: flex; gap: 4px; align-items: center; }
  .btn-sm { border: none; background: none; cursor: pointer; color: var(--text-muted); font-size: 11px; padding: 0 2px; flex-shrink: 0; }
  .btn-add { align-self: flex-start; font-size: 10px; color: var(--text-secondary); background: none; border: 0.5px dashed var(--border-default); border-radius: 4px; padding: 2px 6px; cursor: pointer; }

  .checkbox-field { flex-direction: row; align-items: center; gap: 6px; }
  .checkbox-field label { font-size: 11px; color: var(--text-secondary); }

  .subpanel {
    background: color-mix(in srgb, var(--nc) 5%, var(--bg-elevated));
    border: 0.5px solid color-mix(in srgb, var(--nc) 20%, transparent);
    border-radius: 6px;
    padding: 6px 8px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .subpanel-title { font-size:calc(9px * var(--font-scale)); font-family:'DM Mono',monospace; font-weight:500; letter-spacing:.06em; text-transform:uppercase; color:var(--text-muted); padding:calc(8px * var(--font-scale)) 0 calc(4px * var(--font-scale)); }
  .stage-row { display:flex; flex-direction:column; gap:4px; }
  .actuator-switch {
    display: flex; align-items: center; gap: 4px;
    padding-top: 4px;
    border-top: 0.5px solid var(--border-subtle);
    margin-top: 2px;
  }
  .switch-label { font-size: 9px; color: var(--text-muted); font-family: 'DM Mono', monospace; flex: 1; }
  .sw-btn { padding: 2px 7px; border-radius: 4px; border: 0.5px solid var(--border-default); background: none; cursor: pointer; font-size: 10px; font-family: 'DM Mono', monospace; }
  .sw-on  { color: #3da85a; border-color: #3da85a44; }
  .sw-on:hover  { background: #EAF3DE; }
  .sw-off { color: #e05454; border-color: #e0545444; }
  .sw-off:hover { background: #FCEBEB; }
  .sw-auto:hover { background: var(--interactive-hover); }

  .node-hint {
    margin-top: 4px;
    padding: 6px 8px;
    background: var(--bg-elevated);
    border-left: 2px solid color-mix(in srgb, var(--nc) 40%, transparent);
    border-radius: 0 4px 4px 0;
    font-size: 9px;
    color: var(--text-muted);
    line-height: 1.5;
  }

  /* Resize handle */
  .resize-handle {
    position: absolute;
    bottom: 2px;
    right: 4px;
    font-size: 14px;
    color: var(--text-muted);
    opacity: 0.4;
    cursor: nwse-resize;
    user-select: none;
    line-height: 1;
    transition: opacity .15s;
  }
  .resize-handle:hover { opacity: 0.9; }
</style>