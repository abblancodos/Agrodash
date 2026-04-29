<!-- src/lib/components/processes/LogsTab.svelte -->
<script lang="ts">
  import { processStore } from '$lib/stores/process';

  let { processId }: { processId: string } = $props();

  const logs    = $derived($processStore.logs);
  const liveLog = $derived($processStore.liveLog);

  let activeView = $state<'events' | 'raw'>('events');
  let loadingRaw = $state(false);
  let filterLevel = $state<string>('');

  async function fetchRaw() {
    loadingRaw = true;
    await processStore.fetchLogTail(processId);
    loadingRaw = false;
  }

  $effect(() => {
    if (activeView === 'raw' && liveLog.length === 0) fetchRaw();
  });

  function levelColor(level: string) {
    return level === 'error' ? '#e05454' : level === 'warn' ? '#e8a838' : level === 'debug' ? '#8a9bb0' : 'var(--text-secondary)';
  }

  function fmtTs(ts: string) {
    const d = new Date(ts);
    return d.toLocaleString('es-CR', { day:'2-digit', month:'short', hour:'2-digit', minute:'2-digit', second:'2-digit', hour12: false });
  }

  function sourceIcon(source: string) {
    return source === 'user' ? '👤' : source === 'worker' ? '⚙' : source === 'control' ? '🤖' : '·';
  }

  const filteredLogs = $derived(
    filterLevel ? logs.filter(l => l.level === filterLevel) : logs
  );
</script>

<div class="logs-tab">
  <div class="logs-toolbar">
    <div class="view-tabs">
      <button class="vtab" class:active={activeView === 'events'}
        onclick={() => activeView = 'events'}>eventos</button>
      <button class="vtab" class:active={activeView === 'raw'}
        onclick={() => activeView = 'raw'}>Control.log</button>
    </div>

    {#if activeView === 'events'}
      <div class="filter-row">
        <select class="filter-sel" bind:value={filterLevel}>
          <option value="">todos</option>
          <option value="error">error</option>
          <option value="warn">warn</option>
          <option value="info">info</option>
          <option value="debug">debug</option>
        </select>
      </div>
    {:else}
      <button class="btn-refresh" onclick={fetchRaw} disabled={loadingRaw}>
        {loadingRaw ? '...' : '↺ actualizar'}
      </button>
    {/if}
  </div>

  {#if activeView === 'events'}
    {#if filteredLogs.length === 0}
      <div class="empty">no hay eventos registrados</div>
    {:else}
      <div class="event-list">
        {#each filteredLogs as log (log.id)}
          <div class="event-row">
            <span class="ev-ts">{fmtTs(log.ts)}</span>
            <span class="ev-source" title={log.source}>{sourceIcon(log.source)}</span>
            <span class="ev-level" style="color:{levelColor(log.level)}">{log.level}</span>
            <span class="ev-msg">{log.message}</span>
            {#if log.data}
              <span class="ev-data">{JSON.stringify(log.data)}</span>
            {/if}
          </div>
        {/each}
      </div>
    {/if}

  {:else}
    {#if loadingRaw}
      <div class="empty">cargando...</div>
    {:else if liveLog.length === 0}
      <div class="empty">sin líneas en el log</div>
    {:else}
      <div class="raw-log">
        {#each liveLog as line}
          <div class="raw-line"
            class:line-error={line.includes('[ERROR]')}
            class:line-warn={line.includes('[WARNING]')}
            class:line-info={line.includes('[INFO]')}>
            {line}
          </div>
        {/each}
      </div>
    {/if}
  {/if}
</div>

<style>
  .logs-tab { display: flex; flex-direction: column; gap: calc(12px * var(--font-scale)); }

  .logs-toolbar { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
  .view-tabs { display: flex; gap: 4px; }
  .vtab { padding: calc(5px * var(--font-scale)) calc(12px * var(--font-scale)); border: 0.5px solid var(--border-default); border-radius: 6px; background: none; cursor: pointer; font-size: calc(12px * var(--font-scale)); color: var(--text-muted); font-family: 'DM Mono', monospace; }
  .vtab.active { background: var(--bg-elevated); color: var(--text-primary); }

  .filter-row { display: flex; gap: 6px; }
  .filter-sel { padding: 4px 8px; border: 0.5px solid var(--border-default); border-radius: 4px; background: var(--bg-surface); color: var(--text-primary); font-size: calc(12px * var(--font-scale)); outline: none; }
  .btn-refresh { padding: calc(5px * var(--font-scale)) calc(10px * var(--font-scale)); border: 0.5px solid var(--border-default); border-radius: 6px; background: none; cursor: pointer; font-size: calc(12px * var(--font-scale)); color: var(--text-secondary); font-family: 'DM Mono', monospace; }
  .btn-refresh:disabled { opacity: 0.5; }

  .empty { color: var(--text-muted); font-size: calc(12px * var(--font-scale)); text-align: center; padding: 40px 0; }

  /* Event list */
  .event-list { display: flex; flex-direction: column; border: 0.5px solid var(--border-subtle); border-radius: 8px; overflow: hidden; }
  .event-row { display: grid; grid-template-columns: 130px 20px 45px 1fr; gap: calc(8px * var(--font-scale)); align-items: baseline; padding: calc(6px * var(--font-scale)) calc(12px * var(--font-scale)); border-bottom: 0.5px solid var(--border-subtle); font-size: calc(11px * var(--font-scale)); }
  .event-row:last-child { border-bottom: none; }
  .event-row:hover { background: var(--interactive-hover); }
  .ev-ts { color: var(--text-muted); font-family: 'DM Mono', monospace; white-space: nowrap; }
  .ev-source { text-align: center; }
  .ev-level { font-family: 'DM Mono', monospace; font-weight: 500; }
  .ev-msg { color: var(--text-primary); }
  .ev-data { grid-column: 4; color: var(--text-muted); font-family: 'DM Mono', monospace; font-size: calc(10px * var(--font-scale)); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

  /* Raw log */
  .raw-log { background: #0d0d0d; border-radius: 8px; padding: calc(12px * var(--font-scale)); overflow-x: auto; max-height: 600px; overflow-y: auto; display: flex; flex-direction: column; gap: 1px; }
  .raw-line { font-family: 'DM Mono', monospace; font-size: calc(11px * var(--font-scale)); color: #c9d1d9; white-space: pre; line-height: 1.5; }
  .raw-line.line-error { color: #ff7b72; }
  .raw-line.line-warn  { color: #e3b341; }
  .raw-line.line-info  { color: #7ee787; }
</style>
