<script lang="ts">
  import { type Box, normaliseSensorLabel, sensorColor } from '$lib/api';
  import SensorChart from './SensorChart.svelte';
  import MultiSensorChart from './MultiSensorChart.svelte';

  interface Props {
    box: Box;
    from: Date; to: Date; live?: boolean;
    activeTypes: Set<string>; // types to show, controlled by parent
  }
  let { box, from, to, live = false, activeTypes }: Props = $props();

  type Tab = 'individual' | 'combinado';
  let activeTab = $state<Tab>('individual');
  let expanded = $state(true);

  // Group sensors by sensor_number, preserving order
  const sensorRows = $derived.by(() => {
    const map = new Map<number, typeof box.sensors>();
    for (const s of box.sensors) {
      if (!map.has(s.sensor_number)) map.set(s.sensor_number, []);
      map.get(s.sensor_number)!.push(s);
    }
    return [...map.entries()].sort((a, b) => a[0] - b[0]);
  });

  // Sensors filtered by active types
  const visibleSensors = $derived(
    box.sensors.filter(s => activeTypes.has(s.type.toLowerCase()))
  );

  const uniqueTypes = $derived([...new Set(box.sensors.map(s => s.type))]);
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
    <div class="pills">
      {#each uniqueTypes as type}
        {@const active = activeTypes.has(type.toLowerCase())}
        <span class="pill" class:pill--off={!active} style="--c:{sensorColor(type)}">
          {normaliseSensorLabel(type)}
        </span>
      {/each}
    </div>
  </div>

  {#if expanded}
    <div class="box-card__body">
      <div class="tabs">
        <button class="tab" class:active={activeTab === 'individual'}
          onclick={() => activeTab = 'individual'}>Individual</button>
        <button class="tab" class:active={activeTab === 'combinado'}
          onclick={() => activeTab = 'combinado'}>Combinado</button>
      </div>

      <!-- Individual: rows per sensor number -->
      <div class:hidden={activeTab !== 'individual'}>
        {#if visibleSensors.length === 0}
          <div class="empty-types">Seleccioná al menos un tipo de sensor.</div>
        {:else}
          {#each sensorRows as [num, sensors] (num)}
            {@const rowSensors = sensors.filter(s => activeTypes.has(s.type.toLowerCase()))}
            {#if rowSensors.length > 0}
              <div class="sensor-row">
                <div class="sensor-row__label">#{num}</div>
                <div class="sensor-row__charts">
                  {#each rowSensors as sensor (sensor.id)}
                    <div class="chart-wrap">
                      <SensorChart sensorId={sensor.id} sensorType={sensor.type}
                        label={normaliseSensorLabel(sensor.type)}
                        {from} {to} {live} />
                    </div>
                  {/each}
                </div>
              </div>
            {/if}
          {/each}
        {/if}
      </div>

      <!-- Combinado -->
      <div class="combined" class:hidden={activeTab !== 'combinado'}>
        {#if visibleSensors.length === 0}
          <div class="empty-types">Seleccioná al menos un tipo de sensor.</div>
        {:else}
          <MultiSensorChart sensors={visibleSensors} {from} {to} {live} />
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

  .box-card__header { padding: 12px 16px 10px; }

  .title-row {
    display:flex; align-items:center; gap:8px;
    margin-bottom:8px; cursor:pointer; user-select:none;
  }
  .chevron { font-size:10px; color:var(--text-muted); transition:transform .2s; display:inline-block; }
  .chevron.open { transform:rotate(90deg); }
  .name {
    font-family:'DM Mono',monospace; font-size:12px; font-weight:600;
    letter-spacing:.08em; color:var(--text-primary);
    text-transform:uppercase; margin:0; flex:1;
  }
  .count { font-family:'DM Mono',monospace; font-size:9px; color:var(--text-faint); letter-spacing:.06em; }

  .pills { display:flex; flex-wrap:wrap; gap:4px; }
  .pill {
    font-family:'DM Mono',monospace; font-size:8.5px; letter-spacing:.06em;
    padding:2px 7px; border-radius:2px; color:var(--c);
    background:color-mix(in srgb, var(--c) 12%, transparent);
    border:1px solid color-mix(in srgb, var(--c) 28%, transparent);
    transition: opacity .15s;
  }
  .pill--off { opacity: .35; }

  .box-card__body { border-top:1px solid var(--border-subtle); padding:0 16px 16px; }

  .tabs { display:flex; margin:0 -16px 12px; border-bottom:1px solid var(--border-subtle); }
  .tab {
    padding:8px 18px; background:none; border:none;
    border-bottom:2px solid transparent;
    color:var(--text-muted); font-family:'DM Mono',monospace;
    font-size:10px; letter-spacing:.1em; text-transform:uppercase;
    cursor:pointer; transition:all .12s; margin-bottom:-1px;
  }
  .tab:hover { color:var(--text-secondary); }
  .tab.active { color:var(--text-primary); border-bottom-color:var(--border-strong); }

  /* Sensor rows */
  .sensor-row {
    display: flex; gap: 10px; align-items: flex-start;
    padding: 10px 0;
    border-bottom: 1px solid var(--border-subtle);
  }
  .sensor-row:last-child { border-bottom: none; }

  .sensor-row__label {
    font-family: 'DM Mono', monospace; font-size: 10px;
    color: var(--text-faint); letter-spacing: .08em;
    min-width: 28px; padding-top: 6px; flex-shrink: 0;
    text-align: right;
  }

  .sensor-row__charts {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
    gap: 10px; flex: 1;
  }

  .chart-wrap {
    background: var(--bg-inset); border: 1px solid var(--border-subtle);
    border-radius: 4px; padding: 10px 12px;
  }

  .combined {
    margin-top: 4px; background: var(--bg-inset);
    border: 1px solid var(--border-subtle); border-radius: 4px; padding: 12px 14px;
  }

  .empty-types {
    padding: 24px; text-align: center;
    font-family: 'DM Mono', monospace; font-size: 10px;
    color: var(--text-faint); letter-spacing: .06em;
  }

  .hidden { display: none; }
</style>