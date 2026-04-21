<script lang="ts">
  import { experimentStore, activeEvents } from '$lib/stores/experiment';
  import { auth } from '$lib/stores/auth';

  // Muestra banner si hay entries recientes (< 5 min) de otros usuarios
  const recentConflict = $derived(() => {
    const myId = $auth.user?.id;
    const fiveMinAgo = Date.now() - 5 * 60 * 1000;
    return $activeEvents.find(ev =>
      ev.recorded_by && ev.recorded_by !== myId &&
      new Date(ev.recorded_at).getTime() > fiveMinAgo
    ) ?? null;
  });

  function relTime(iso: string) {
    const secs = Math.floor((Date.now() - new Date(iso).getTime()) / 1000);
    if (secs < 60) return `hace ${secs}s`;
    return `hace ${Math.floor(secs / 60)} min`;
  }
</script>

{#if recentConflict()}
  {@const ev = recentConflict()!}
  <div class="conflict-banner">
    <div class="conflict-icon"></div>
    <div class="conflict-text">
      <strong>Actividad reciente</strong> — se registró una entry
      ({ev.step_key}, {relTime(ev.recorded_at)}).
      Verificá que tus datos sigan siendo válidos antes de guardar.
    </div>
  </div>
{/if}

<style>
  .conflict-banner {
    display: flex; align-items: flex-start; gap: calc(10px * var(--font-scale));
    padding: calc(10px * var(--font-scale)) calc(14px * var(--font-scale));
    background: #FAEEDA; border: 0.5px solid #BA7517;
    border-radius: 6px; margin-bottom: calc(14px * var(--font-scale));
  }
  .conflict-icon {
    width: 8px; height: 8px; border-radius: 50%;
    background: #BA7517; flex-shrink: 0; margin-top: 3px;
  }
  .conflict-text { font-size: calc(12px * var(--font-scale)); color: #633806; line-height: 1.5; }
  .conflict-text strong { color: #412402; }
</style>
