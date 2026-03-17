<script lang="ts">
  import { onMount } from 'svelte';
  import { fetchBoxes, fetchTimeRange, type Box } from '$lib/api';
  import BoxCard from '$lib/components/BoxCard.svelte';
  import BoxSelector from '$lib/components/BoxSelector.svelte';
  import TypeSelector from '$lib/components/TypeSelector.svelte';
  import DateTimePicker from '$lib/components/DateTimePicker.svelte';

  let boxes = $state<Box[]>([]);
  let loading = $state(true);
  let error = $state('');

  let minDate = $state<Date | undefined>(undefined);
  let maxDate = $state<Date | undefined>(undefined);
  let fromDate = $state(new Date(Date.now() - 24 * 60 * 60 * 1000));
  let toDate   = $state(new Date());

  let rangeLoading = $state(false);
  let rangeError   = $state(false);

  // Box selection — starts empty, filled after boxes load
  let selectedBoxIds = $state<Set<string>>(new Set());

  // Type selection — all types active by default
  let activeTypes = $state<Set<string>>(new Set());

  async function loadTimeRange() {
    rangeLoading = true; rangeError = false;
    try {
      const { first, last } = await fetchTimeRange();
      minDate = first; maxDate = last;
      toDate = last;
      fromDate = new Date(last.getTime() - 24 * 60 * 60 * 1000);
    } catch { rangeError = true; }
    finally { rangeLoading = false; }
  }

  const PRESETS = [
    { label: '1h',  hours: 1 },
    { label: '6h',  hours: 6 },
    { label: '24h', hours: 24 },
    { label: '7d',  hours: 168 },
    { label: '30d', hours: 720 },
  ];
  let activePreset = $state('24h');

  function applyPreset(p: { label: string; hours: number }) {
    activePreset = p.label;
    toDate   = maxDate ?? new Date();
    fromDate = new Date(toDate.getTime() - p.hours * 3_600_000);
  }

  let live = $state(false);
  let search = $state('');

  const filteredBoxes = $derived(
    boxes.filter(b =>
      selectedBoxIds.has(b.id) &&
      (!search.trim() || b.name.toLowerCase().includes(search.toLowerCase()))
    )
  );

  onMount(async () => {
    await Promise.all([
      fetchBoxes().then(b => {
        boxes = b;
        selectedBoxIds = new Set(b.length ? [b[0].id] : []);
        // Collect all unique types across all boxes
        activeTypes = new Set(b.flatMap(box => box.sensors.map(s => s.type.toLowerCase())));
      }).catch(e => error = e.message),
      loadTimeRange(),
    ]);
    loading = false;
  });
</script>

<svelte:head><title>AgroDash</title></svelte:head>

<main class="dashboard">
  <div class="controls">
    <div class="controls__search">
      <span class="controls__icon">⌕</span>
      <input class="controls__input" type="text"
        placeholder="Filtrar cajas..." bind:value={search} />
    </div>

    <div class="controls__presets">
      {#each PRESETS as p}
        <button class="preset-btn" class:active={activePreset === p.label}
          onclick={() => applyPreset(p)}>{p.label}</button>
      {/each}
    </div>

    <div class="controls__range">
      <DateTimePicker bind:value={fromDate} min={minDate} max={toDate}
        label="DESDE" onchange={() => activePreset = ''} />
      <span class="sep">→</span>
      <div class="to-wrap">
        <DateTimePicker bind:value={toDate} min={fromDate} max={maxDate}
          label="HASTA" onchange={() => activePreset = ''} />
        <button class="reload-btn"
          class:loading={rangeLoading} class:err={rangeError}
          onclick={loadTimeRange} title="Actualizar última lectura"
          disabled={rangeLoading}>
          {#if rangeLoading}<span class="spinner"></span>
          {:else if rangeError}!
          {:else}↺{/if}
        </button>
      </div>
    </div>

    <button class="live-btn" class:active={live} onclick={() => live = !live}>
      <span class="live-btn__dot"></span>
      {live ? 'LIVE ON' : 'LIVE OFF'}
    </button>

    {#if !loading && boxes.length}
      <TypeSelector
        {boxes}
        selected={activeTypes}
        onchange={(s) => activeTypes = s}
      />
    {/if}
  </div>

  {#if !loading}
    <div class="summary">
      <span>{filteredBoxes.length} cajas</span>
      <span class="dot">·</span>
      <span>{filteredBoxes.reduce((a, b) => a + b.sensors.length, 0)} sensores</span>
      {#if minDate}<span class="dot">·</span><span>datos desde {minDate.toLocaleDateString('es-CR')}</span>{/if}
      {#if live}<span class="dot">·</span><span class="live-label">actualizando cada 15s</span>{/if}
    </div>
  {/if}

  {#if !loading && boxes.length}
    <div class="box-selector-bar">
      <BoxSelector
        {boxes}
        selected={selectedBoxIds}
        onchange={(s) => selectedBoxIds = s}
      />
    </div>
  {/if}

  <div class="box-grid">
    {#if loading}
      {#each Array(4) as _}<div class="skeleton"></div>{/each}
    {:else if error}
      <div class="error-msg">Error: {error}</div>
    {:else if filteredBoxes.length === 0 && boxes.length > 0}
      <div class="no-selection">Seleccioná una o más cajas para ver los datos.</div>
    {:else}
      {#each filteredBoxes as box (box.id)}<BoxCard {box} from={fromDate} to={toDate} {live} {activeTypes} />{/each}
    {/if}
  </div>
</main>

<style>
  .dashboard { max-width: 1600px; margin: 0 auto; padding: 20px 24px 48px; }

  .controls { display:flex; align-items:center; gap:10px; flex-wrap:wrap; margin-bottom:12px; }

  .controls__search { position:relative; flex:1; min-width:160px; max-width:240px; }
  .controls__icon { position:absolute; left:9px; top:50%; transform:translateY(-50%); color:var(--text-muted); font-size:14px; pointer-events:none; }
  .controls__input { width:100%; box-sizing:border-box; padding:6px 10px 6px 28px; background:var(--bg-surface); border:1px solid var(--border-default); border-radius:4px; color:var(--text-primary); font-family:'DM Mono',monospace; font-size:11px; outline:none; transition:border-color .15s; }
  .controls__input:focus { border-color:var(--interactive-focus); }
  .controls__input::placeholder { color:var(--text-faint); }

  .controls__presets { display:flex; gap:3px; }
  .preset-btn { padding:5px 9px; background:var(--interactive-bg); border:1px solid var(--border-default); border-radius:3px; color:var(--text-muted); font-family:'DM Mono',monospace; font-size:10px; letter-spacing:.06em; cursor:pointer; transition:all .12s; }
  .preset-btn:hover { background:var(--interactive-hover); color:var(--text-secondary); }
  .preset-btn.active { background:var(--accent-bg); border-color:var(--accent-border); color:var(--accent-text); }

  .controls__range { display:flex; align-items:center; gap:6px; flex-wrap:wrap; }
  .sep { color:var(--text-faint); font-size:11px; }
  .to-wrap { display:flex; align-items:center; gap:4px; }

  .reload-btn { width:26px; height:26px; padding:0; background:var(--interactive-bg); border:1px solid var(--border-default); border-radius:3px; color:var(--text-muted); font-size:13px; cursor:pointer; display:flex; align-items:center; justify-content:center; transition:all .12s; }
  .reload-btn:hover:not(:disabled) { background:var(--interactive-hover); color:var(--text-secondary); }
  .reload-btn.err { border-color:var(--error-border); color:var(--error-color); }
  .reload-btn.loading { opacity:.5; cursor:default; }
  .spinner { width:10px; height:10px; border:1.5px solid var(--border-strong); border-top-color:var(--text-secondary); border-radius:50%; animation:spin .7s linear infinite; display:inline-block; }
  @keyframes spin { to{transform:rotate(360deg)} }

  .live-btn { display:flex; align-items:center; gap:6px; padding:5px 11px; background:var(--interactive-bg); border:1px solid var(--border-default); border-radius:3px; color:var(--text-muted); font-family:'DM Mono',monospace; font-size:10px; letter-spacing:.1em; cursor:pointer; transition:all .15s; }
  .live-btn.active { background:var(--live-bg); border-color:var(--live-border); color:var(--live-color); }
  .live-btn__dot { width:6px; height:6px; border-radius:50%; background:currentColor; }
  .live-btn.active .live-btn__dot { animation:pulse 1.5s infinite; }
  @keyframes pulse { 0%,100%{opacity:1;transform:scale(1)}50%{opacity:.4;transform:scale(.8)} }

  .summary { display:flex; align-items:center; gap:8px; margin-bottom:16px; font-size:10px; color:var(--text-muted); letter-spacing:.06em; }
  .dot { color:var(--border-default); }
  .live-label { color:var(--live-color); }

  .box-grid { display:flex; flex-direction:column; gap:12px; }

  .skeleton { height:160px; border-radius:6px; border:1px solid var(--border-subtle); background:linear-gradient(90deg,var(--skeleton-from) 25%,var(--skeleton-to) 50%,var(--skeleton-from) 75%); background-size:200% 100%; animation:shimmer 1.5s infinite; }
  @keyframes shimmer { 0%{background-position:200% center}100%{background-position:-200% center} }

  .error-msg { font-size:11px; color:var(--error-color); padding:24px; text-align:center; border:1px solid var(--error-border); border-radius:6px; background:var(--error-bg); }
  .box-selector-bar { margin-bottom: 12px; }
  .no-selection { font-size:11px; color:var(--text-faint); padding:48px; text-align:center; font-family:'DM Mono',monospace; letter-spacing:.06em; }
</style>