<script lang="ts">
  import { type Box, normaliseSensorLabel } from '$lib/api';
  import SensorChart from './SensorChart.svelte';
  import MultiSensorChart from './MultiSensorChart.svelte';
  import SensorSelect from './SensorSelect.svelte';

  interface Props {
    box: Box;
    from: Date; to: Date; live?: boolean;
    activeTypes: Set<string>;
  }
  let { box, from, to, live = false, activeTypes }: Props = $props();

  type Tab = 'individual' | 'combinado';
  let activeTab = $state<Tab>('individual');
  let expanded = $state(true);

  // Group sensors by sensor_number
  const sensorRows = $derived.by(() => {
    const map = new Map<number, { id: string; type: string }[]>();
    for (const s of box.sensors) {
      if (!map.has(s.sensor_number)) map.set(s.sensor_number, []);
      map.get(s.sensor_number)!.push({ id: s.id, type: s.type });
    }
    return [...map.entries()]
      .sort((a, b) => a[0] - b[0])
      .map(([number, types]) => ({ number, types }));
  });

  // active: sensor_number → Set<type_lowercase> — all on by default
  let active = $state<Map<number, Set<string>>>(
    new Map(sensorRows.map(r => [
      r.number,
      new Set(r.types.map(t => t.type.toLowerCase()))
    ]))
  );

  // Check if a sensor+type combo is active
  function isActive(num: number, type: string) {
    return active.get(num)?.has(type.toLowerCase()) ?? true;
  }

  // Sensors for individual view — all sensor rows, filtered by active types
  const individualRows = $derived(
    sensorRows.map(r => ({
      number: r.number,
      sensors: box.sensors.filter(s =>
        s.sensor_number === r.number &&
        activeTypes.has(s.type.toLowerCase()) &&
        isActive(r.number, s.type)
      )
    })).filter(r => r.sensors.length > 0)
  );

  // Sensors for combinado
  const combinadoSensors = $derived(
    box.sensors.filter(s =>
      activeTypes.has(s.type.toLowerCase()) &&
      isActive(s.sensor_number, s.type)
    )
  );
</script>

<article class="box-card">
  <div class="box-card__header">
    <div class="title-row" role="button" tabindex="0"
      onclick={() => expanded = !expanded}
      onkeydown={(e) => e.key === 'Enter' && (expanded = !expanded)}>
      <span class="chevron" class:open={expanded}>▸</span>
      <h2 class="name">{box.name}</h2>
      <span class="count">{box.sensors.length} sensores</span>
    </div>
  </div>

  {#if expanded}
    <div class="box-card__body">

      <!-- Sensor selector -->
      <div class="selector-bar">
        <SensorSelect
          rows={sensorRows}
          {active}
          onchange={(a) => active = a}
        />
      </div>

      <!-- Tabs -->
      <div class="tabs">
        <button class="tab" class:active={activeTab === 'individual'}
          onclick={() => activeTab = 'individual'}>Gráficas individuales</button>
        <button class="tab" class:active={activeTab === 'combinado'}
          onclick={() => activeTab = 'combinado'}>Gráfica combinada</button>
      </div>

      <!-- Individual -->
      <div class:hidden={activeTab !== 'individual'}>
        {#if individualRows.length === 0}
          <div class="empty">Sin sensores o tipos seleccionados.</div>
        {:else}
          {#each individualRows as row (row.number)}
            <div class="sensor-row">
              <div class="sensor-row__label">#{row.number}</div>
              <div class="sensor-row__charts">
                {#each row.sensors as sensor (sensor.id)}
                  <div class="chart-wrap">
                    <SensorChart
                      sensorId={sensor.id}
                      sensorType={sensor.type}
                      label={normaliseSensorLabel(sensor.type)}
                      {from} {to} {live}
                    />
                  </div>
                {/each}
              </div>
            </div>
          {/each}
        {/if}
      </div>

      <!-- Combinado -->
      <div class="combined" class:hidden={activeTab !== 'combinado'}>
        {#if combinadoSensors.length === 0}
          <div class="empty">Sin sensores o tipos seleccionados.</div>
        {:else}
          <MultiSensorChart sensors={combinadoSensors} {from} {to} {live} />
        {/if}
      </div>
    </div>
  {/if}
</article>

<style>
  .box-card {
    background: var(--bg-surface);
    border: 1px solid var(--border-subtle);
    border-radius: 6px; overflow: hidden;
    transition: border-color 0.15s, background 0.2s;
  }
  .box-card:hover { border-color: var(--border-default); }

  .box-card__header { padding: 0.75rem 1rem 0.625rem; }

  .title-row {
    display: flex; align-items: center; gap: 0.5rem;
    cursor: pointer; user-select: none;
  }
  .chevron { font-size: 0.625rem; color: var(--text-muted); transition: transform .2s; }
  .chevron.open { transform: rotate(90deg); }
  .name {
    font-family: 'DM Mono', monospace; font-size: 0.75rem; font-weight: 600;
    letter-spacing: .08em; color: var(--text-primary);
    text-transform: uppercase; margin: 0; flex: 1;
  }
  .count { font-family: 'DM Mono', monospace; font-size: 0.5625rem; color: var(--text-faint); letter-spacing: .06em; }

  .box-card__body { border-top: 1px solid var(--border-subtle); padding: 0 1rem 1rem; }

  .selector-bar { padding: 0.625rem 0; }

  /* Tabs */
  .tabs { display: flex; margin: 0 -1rem 0.75rem; border-bottom: 1px solid var(--border-subtle); }
  .tab {
    padding: 0.5rem 1.125rem; background: none; border: none;
    border-bottom: 2px solid transparent;
    color: var(--text-muted); font-family: 'DM Mono', monospace;
    font-size: 0.625rem; letter-spacing: .1em; text-transform: uppercase;
    cursor: pointer; transition: all .12s; margin-bottom: -1px;
  }
  .tab:hover { color: var(--text-secondary); }
  .tab.active { color: var(--text-primary); border-bottom-color: var(--border-strong); }

  /* Sensor rows */
  .sensor-row {
    display: flex; gap: 0.625rem; align-items: flex-start;
    padding: 0.625rem 0; border-bottom: 1px solid var(--border-subtle);
  }
  .sensor-row:last-child { border-bottom: none; }
  .sensor-row__label {
    font-family: 'DM Mono', monospace; font-size: 0.625rem;
    color: var(--text-faint); letter-spacing: .08em;
    min-width: 1.75rem; padding-top: 0.375rem; flex-shrink: 0; text-align: right;
  }
  .sensor-row__charts {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(12.5rem, 1fr));
    gap: 0.625rem; flex: 1;
  }
  .chart-wrap {
    background: var(--bg-inset); border: 1px solid var(--border-subtle);
    border-radius: 4px; padding: 0.625rem 0.75rem;
  }

  .combined {
    margin-top: 0.25rem; background: var(--bg-inset);
    border: 1px solid var(--border-subtle); border-radius: 4px; padding: 0.75rem 0.875rem;
  }

  .empty {
    padding: 1.5rem; text-align: center;
    font-family: 'DM Mono', monospace; font-size: 0.625rem;
    color: var(--text-faint); letter-spacing: .06em;
  }

  .hidden { display: none; }
</style>