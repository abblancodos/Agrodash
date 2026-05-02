<!-- src/lib/components/processes/LogsTab.svelte -->
<script lang="ts">
  import { processStore } from '$lib/stores/process';

  let { processId }: { processId: string } = $props();

  const logs        = $derived(processStore.logs);
  let filterLevel   = $state('');
  let filterSource  = $state('');

  const filtered = $derived(
    logs.filter(l =>
      (!filterLevel  || l.level  === filterLevel) &&
      (!filterSource || l.source === filterSource)
    )
  );

  function levelColor(level: string) {
    return level === 'error' ? '#e05454' : level === 'warn' ? '#e8a838' : level === 'debug' ? '#8a9bb0' : 'var(--text-secondary)';
  }

  function srcIcon(source: string) {
    return source === 'user' ? '👤' : source === 'worker' ? '⚙' : source === 'control' ? '🤖' : '·';
  }

  function fmtTs(ts: string) {
    return new Date(ts).toLocaleString('es-CR', {
      day:'2-digit', month:'short',
      hour:'2-digit', minute:'2-digit', second:'2-digit', hour12: false,
    });
  }
</script>

<div class="logs-tab">
  <div class="toolbar">
    <select class="sel" bind:value={filterLevel}>
      <option value="">todos los niveles</option>
      <option value="error">error</option>
      <option value="warn">warn</option>
      <option value="info">info</option>
      <option value="debug">debug</option>
    </select>
    <select class="sel" bind:value={filterSource}>
      <option value="">todas las fuentes</option>
      <option value="worker">worker</option>
      <option value="user">user</option>
      <option value="system">system</option>
    </select>
    <span class="count">{filtered.length} eventos</span>
  </div>

  {#if filtered.length === 0}
    <div class="empty">sin eventos registrados</div>
  {:else}
    <div class="event-list">
      {#each filtered as log, i (log.id ?? `${i}-${log.ts}-${log.message?.slice(0,20)}`)}
        <div class="event-row">
          <span class="ev-ts">{fmtTs(log.ts)}</span>
          <span class="ev-src" title={log.source}>{srcIcon(log.source)}</span>
          <span class="ev-lvl" style="color:{levelColor(log.level)}">{log.level}</span>
          <span class="ev-msg">{log.message}</span>
          {#if log.data}
            <span class="ev-data">{JSON.stringify(log.data)}</span>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .logs-tab { display: flex; flex-direction: column; gap: calc(12px * var(--font-scale)); }

  .toolbar { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .sel { padding: 4px 8px; border: 0.5px solid var(--border-default); border-radius: 4px; background: var(--bg-surface); color: var(--text-primary); font-size: calc(12px * var(--font-scale)); outline: none; }
  .count { font-size: calc(11px * var(--font-scale)); color: var(--text-muted); font-family: 'DM Mono', monospace; margin-left: auto; }

  .empty { color: var(--text-muted); font-size: calc(12px * var(--font-scale)); text-align: center; padding: 40px 0; }

  .event-list { display: flex; flex-direction: column; border: 0.5px solid var(--border-subtle); border-radius: 8px; overflow: hidden; }
  .event-row { display: grid; grid-template-columns: 140px 22px 46px 1fr; gap: calc(8px * var(--font-scale)); align-items: baseline; padding: calc(6px * var(--font-scale)) calc(12px * var(--font-scale)); border-bottom: 0.5px solid var(--border-subtle); font-size: calc(11px * var(--font-scale)); }
  .event-row:last-child { border-bottom: none; }
  .event-row:hover { background: var(--interactive-hover); }
  .ev-ts  { color: var(--text-muted); font-family: 'DM Mono', monospace; white-space: nowrap; }
  .ev-src { text-align: center; }
  .ev-lvl { font-family: 'DM Mono', monospace; font-weight: 500; }
  .ev-msg { color: var(--text-primary); }
  .ev-data { grid-column: 4; color: var(--text-muted); font-family: 'DM Mono', monospace; font-size: calc(10px * var(--font-scale)); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>