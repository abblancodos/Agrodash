<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { fetchTemperature } from '$lib/api';

  const API_BASE = import.meta.env.VITE_API_BASE ?? '';

  let time = $state('');
  let temperature = $state<number | null>(null);
  let tempError = $state(false);
  let tempLoading = $state(true);

  // ── Theme ──────────────────────────────────────────────────────────────────
  let isDark = $state(false);

  function toggleTheme() {
    isDark = !isDark;
    document.documentElement.classList.toggle('light', !isDark);
    try { localStorage.setItem('agrodash-theme', isDark ? 'dark' : 'light'); } catch {}
  }

  onMount(() => {
    // Restore saved preference
    try {
      const saved = localStorage.getItem('agrodash-theme');
      const preferDark = saved === 'dark';
      isDark = preferDark;
      document.documentElement.classList.toggle('light', !preferDark);
    } catch {
      document.documentElement.classList.add('light');
    }
  });

  // ── Clock ──────────────────────────────────────────────────────────────────
  function tick() {
    const now = new Date();
    const pad = (n: number) => String(n).padStart(2, '0');
    time = `${pad(now.getHours())}:${pad(now.getMinutes())}:${pad(now.getSeconds())}`;
  }

  // ── Temperature ────────────────────────────────────────────────────────────
  async function loadTemp() {
    try {
      temperature = await fetchTemperature();
      tempError = false;
    } catch {
      tempError = true; temperature = null;
    } finally {
      tempLoading = false;
    }
  }

  let clockInterval: ReturnType<typeof setInterval>;
  let tempInterval: ReturnType<typeof setInterval>;

  onMount(() => {
    tick();
    clockInterval = setInterval(tick, 1000);
    loadTemp();
    tempInterval = setInterval(loadTemp, 30_000);
  });

  onDestroy(() => {
    clearInterval(clockInterval);
    clearInterval(tempInterval);
  });
</script>

<header class="topbar">
  <div class="topbar__brand">
    <span class="topbar__logo-mark"></span>
    <span class="topbar__name">AGRODASH</span>
  </div>

  <div class="topbar__right">
    <!-- Temperature -->
    <div class="readout" class:readout--error={tempError}>
      <span class="readout__label">TEMP</span>
      <span class="readout__value">
        {#if tempLoading}
          <span class="readout__skeleton"></span>
        {:else if tempError}
          <span class="readout__err">—</span>
        {:else}
          {temperature?.toFixed(1)}<span class="readout__unit">°C</span>
        {/if}
      </span>
    </div>

    <div class="topbar__divider"></div>

    <!-- Clock -->
    <div class="readout">
      <span class="readout__label">LOCAL</span>
      <span class="readout__value readout__value--mono">{time || '——:——:——'}</span>
    </div>

    <div class="topbar__divider"></div>

    <!-- Theme toggle -->
    <button
      class="theme-toggle"
      onclick={toggleTheme}
      title={isDark ? 'Cambiar a modo claro' : 'Cambiar a modo oscuro'}
      aria-label="Toggle theme"
    >
      {#if isDark}
        <!-- Sun icon -->
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
          <circle cx="12" cy="12" r="5"/>
          <line x1="12" y1="1" x2="12" y2="3"/>
          <line x1="12" y1="21" x2="12" y2="23"/>
          <line x1="4.22" y1="4.22" x2="5.64" y2="5.64"/>
          <line x1="18.36" y1="18.36" x2="19.78" y2="19.78"/>
          <line x1="1" y1="12" x2="3" y2="12"/>
          <line x1="21" y1="12" x2="23" y2="12"/>
          <line x1="4.22" y1="19.78" x2="5.64" y2="18.36"/>
          <line x1="18.36" y1="5.64" x2="19.78" y2="4.22"/>
        </svg>
      {:else}
        <!-- Moon icon -->
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
          <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"/>
        </svg>
      {/if}
    </button>
  </div>
</header>

<style>
  .topbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 48px;
    padding: 0 20px;
    background: var(--tb-bg);
    border-bottom: 1px solid var(--tb-border);
    background-image: repeating-linear-gradient(
      90deg, transparent, transparent 2px,
      rgba(0,0,0,.012) 2px, rgba(0,0,0,.012) 4px
    );
    font-family: 'DM Mono', monospace;
    user-select: none;
    transition: background 0.2s, border-color 0.2s;
  }

  .topbar__brand {
    display: flex; align-items: center; gap: 9px;
  }
  .topbar__logo-mark {
    display: block; width: 10px; height: 10px;
    background: var(--tb-accent);
    clip-path: polygon(50% 0%, 100% 75%, 0% 75%);
  }
  .topbar__name {
    font-size: 11px; font-weight: 600; letter-spacing: 0.2em;
    color: var(--tb-text);
  }

  .topbar__right {
    display: flex; align-items: center; gap: 16px;
  }

  .topbar__divider {
    width: 1px; height: 20px; background: var(--tb-border);
  }

  /* Readouts */
  .readout {
    display: flex; flex-direction: column; align-items: flex-end; gap: 1px; min-width: 72px;
  }
  .readout__label {
    font-size: 8.5px; letter-spacing: 0.14em;
    color: var(--tb-muted); line-height: 1;
  }
  .readout__value {
    font-size: 15px; font-weight: 500; color: var(--tb-text);
    line-height: 1; display: flex; align-items: baseline; gap: 2px;
  }
  .readout__value--mono { font-variant-numeric: tabular-nums; letter-spacing: 0.04em; }
  .readout__unit { font-size: 10px; color: var(--tb-secondary); font-weight: 400; }
  .readout--error .readout__label { color: var(--error-color); opacity: 0.7; }
  .readout__err  { color: var(--tb-muted); }

  .readout__skeleton {
    display: inline-block; width: 40px; height: 13px; border-radius: 2px;
    background: linear-gradient(90deg, var(--tb-border) 25%, color-mix(in srgb, var(--tb-border) 60%, transparent) 50%, var(--tb-border) 75%);
    background-size: 200% 100%;
    animation: shimmer 1.4s infinite;
  }
  @keyframes shimmer { 0%{background-position:200% center}100%{background-position:-200% center} }

  /* Theme toggle */
  .theme-toggle {
    display: flex; align-items: center; justify-content: center;
    width: 28px; height: 28px;
    background: none;
    border: 1px solid var(--tb-border);
    border-radius: 4px;
    color: var(--tb-secondary);
    cursor: pointer;
    transition: all 0.15s;
    padding: 0;
  }
  .theme-toggle:hover {
    background: color-mix(in srgb, var(--tb-border) 40%, transparent);
    color: var(--tb-text);
  }
</style>