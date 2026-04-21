<script lang="ts">
  import { onMount } from 'svelte';

  const API = import.meta.env.VITE_API_BASE ?? '';

  let serverTime = $state<string | null>(null);
  let loading    = $state(true);

  onMount(async () => {
    try {
      const res = await fetch(`${API}/api/v1/time`);
      if (res.ok) {
        const data = await res.json();
        serverTime = data.cr;
      }
    } catch { /* silencioso */ }
    finally { loading = false; }
  });
</script>

<div class="ts-warn">
  <div class="ts-dot"></div>
  <span class="ts-text">
    {#if loading}
      obteniendo hora del servidor...
    {:else if serverTime}
      hora del servidor: <strong>{serverTime} CR</strong> — se usará como timestamp
    {:else}
      no se pudo obtener la hora del servidor
    {/if}
  </span>
</div>

<style>
  .ts-warn {
    display: flex; align-items: center; gap: calc(8px * var(--font-scale));
    padding: calc(8px * var(--font-scale)) calc(12px * var(--font-scale));
    background: #FAEEDA; border: 0.5px solid #BA7517;
    border-radius: 6px; margin-bottom: calc(12px * var(--font-scale));
  }
  .ts-dot { width: 8px; height: 8px; border-radius: 50%; background: #BA7517; flex-shrink: 0; }
  .ts-text { font-size: calc(12px * var(--font-scale)); color: #633806; }
  .ts-text strong { color: #412402; }
</style>
