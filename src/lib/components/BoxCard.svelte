<script lang="ts">
  import { type Box, normaliseSensorLabel, sensorColor } from '$lib/api';
  import SensorChart from './SensorChart.svelte';
  import MultiSensorChart from './MultiSensorChart.svelte';

  interface Props { box: Box; from: Date; to: Date; live?: boolean; }
  let { box, from, to, live = false }: Props = $props();

  type Tab = 'individual' | 'combinado';
  let activeTab = $state<Tab>('individual');
  let expanded = $state(true);

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
        <span class="pill" style="--c:{sensorColor(type)}">{normaliseSensorLabel(type)}</span>
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

      {#if activeTab === 'individual'}
        <div class="grid">
          {#each box.sensors as sensor (sensor.id)}
            <div class="chart-wrap">
              <SensorChart sensorId={sensor.id} sensorType={sensor.type}
                label="{normaliseSensorLabel(sensor.type)} #{sensor.sensor_number}"
                {from} {to} {live} />
            </div>
          {/each}
        </div>
      {:else}
        <div class="combined">
          <MultiSensorChart sensors={box.sensors} {from} {to} {live} />
        </div>
      {/if}
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
  .chevron {
    font-size:10px; color:var(--text-muted);
    transition:transform .2s; display:inline-block;
  }
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
  }

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

  .grid {
    display:grid; grid-template-columns:repeat(auto-fill,minmax(220px,1fr));
    gap:14px; margin-top:4px;
  }
  .chart-wrap {
    background:var(--bg-inset); border:1px solid var(--border-subtle);
    border-radius:4px; padding:10px 12px;
  }
  .combined {
    margin-top:4px; background:var(--bg-inset);
    border:1px solid var(--border-subtle); border-radius:4px; padding:12px 14px;
  }
</style>