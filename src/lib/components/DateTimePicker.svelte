<script lang="ts">
  interface Props {
    value: Date; min?: Date; max?: Date; label?: string; onchange?: (d: Date) => void;
  }
  let { value = $bindable(), min, max, label = '', onchange }: Props = $props();

  let open = $state(false);
  let pickerEl: HTMLDivElement;
  let viewYear  = $state(value.getFullYear());
  let viewMonth = $state(value.getMonth());

  let selYear  = $state(value.getFullYear());
  let selMonth = $state(value.getMonth());
  let selDay   = $state(value.getDate());
  let selHour  = $state(value.getHours());
  let selMin   = $state(value.getMinutes());
  let selSec   = $state(value.getSeconds());

  // Sync internal state when value changes externally (e.g. preset buttons)
  $effect(() => {
    selYear=value.getFullYear(); selMonth=value.getMonth(); selDay=value.getDate();
    selHour=value.getHours(); selMin=value.getMinutes(); selSec=value.getSeconds();
    viewYear=selYear; viewMonth=selMonth;
  });

  // Clamp value when min/max change reactively (e.g. the other picker moves)
  $effect(() => {
    if (min && value < min) { value = new Date(min); onchange?.(value); }
    if (max && value > max) { value = new Date(max); onchange?.(value); }
  });

  const MONTHS = ['Ene','Feb','Mar','Abr','May','Jun','Jul','Ago','Sep','Oct','Nov','Dic'];
  const DAYS   = ['D','L','M','X','J','V','S'];

  const calendarDays = $derived.by(() => {
    const firstDow = new Date(viewYear, viewMonth, 1).getDay();
    const daysInMonth = new Date(viewYear, viewMonth + 1, 0).getDate();
    const cells: (number | null)[] = Array(firstDow).fill(null);
    for (let d = 1; d <= daysInMonth; d++) cells.push(d);
    while (cells.length % 7 !== 0) cells.push(null);
    return cells;
  });

  function isDisabled(day: number | null): boolean {
    if (!day) return true;
    const d = new Date(viewYear, viewMonth, day);
    if (min && d < new Date(min.getFullYear(), min.getMonth(), min.getDate())) return true;
    if (max && d > new Date(max.getFullYear(), max.getMonth(), max.getDate())) return true;
    return false;
  }
  function isSelected(day: number | null) {
    return !!day && day===selDay && viewMonth===selMonth && viewYear===selYear;
  }
  function isToday(day: number | null) {
    if (!day) return false;
    const n = new Date();
    return day===n.getDate() && viewMonth===n.getMonth() && viewYear===n.getFullYear();
  }

  function prevMonth() { if(viewMonth===0){viewMonth=11;viewYear--;}else viewMonth--; }
  function nextMonth() { if(viewMonth===11){viewMonth=0;viewYear++;}else viewMonth++; }

  function selectDay(day: number | null) {
    if (!day || isDisabled(day)) return;
    selDay=day; selMonth=viewMonth; selYear=viewYear; commit();
  }

  function clampTime() {
    const d = new Date(selYear, selMonth, selDay, selHour, selMin, selSec);
    if (min && d < min) { selHour=min.getHours(); selMin=min.getMinutes(); selSec=min.getSeconds(); }
    if (max && d > max) { selHour=max.getHours(); selMin=max.getMinutes(); selSec=max.getSeconds(); }
  }

  function commit() {
    clampTime();
    const d = new Date(selYear, selMonth, selDay, selHour, selMin, selSec);
    value = d; onchange?.(d);
  }

  function scrollField(field: 'h'|'m'|'s', delta: number) {
    if (field==='h') selHour=(selHour+delta+24)%24;
    if (field==='m') selMin=(selMin+delta+60)%60;
    if (field==='s') selSec=(selSec+delta+60)%60;
    commit();
  }

  function fmt(d: Date) {
    const pad = (n: number) => String(n).padStart(2,'0');
    return `${d.getFullYear()}-${pad(d.getMonth()+1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
  }

  function handleOutside(e: MouseEvent) {
    if (open && pickerEl && !pickerEl.contains(e.target as Node)) open = false;
  }
</script>

<svelte:window onclick={handleOutside} />

<div class="dtp" bind:this={pickerEl}>
  <button class="dtp__trigger" onclick={() => open = !open}>
    {#if label}<span class="dtp__tl">{label}</span>{/if}
    <span class="dtp__tv">{fmt(value)}</span>
    <span class="dtp__ti">{open ? '▴' : '▾'}</span>
  </button>

  {#if open}
    <div class="dtp__drop">
      <!-- Calendar -->
      <div class="cal">
        <div class="cal__nav">
          <button class="cal__nb" onclick={prevMonth}>‹</button>
          <span class="cal__title">{MONTHS[viewMonth]} {viewYear}</span>
          <button class="cal__nb" onclick={nextMonth}>›</button>
        </div>
        <div class="cal__grid">
          {#each DAYS as d}<span class="cal__dow">{d}</span>{/each}
          {#each calendarDays as day}
            <button class="cal__day"
              class:sel={isSelected(day)} class:tod={isToday(day)}
              class:dis={isDisabled(day)} class:empty={!day}
              onclick={() => selectDay(day)} disabled={isDisabled(day)}>
              {day ?? ''}
            </button>
          {/each}
        </div>
      </div>

      <div class="dtp__divider"></div>

      <!-- Clock -->
      <div class="clk">
        <div class="clk__lbl">HORA</div>
        <div class="clk__display">
          {#each (['h','m','s'] as const) as field, i}
            {#if i > 0}<span class="clk__sep">:</span>{/if}
            <div class="wheel">
              <button class="wheel__btn" onclick={() => scrollField(field, 1)}>▴</button>
              <input class="wheel__val" type="number"
                min={field==='h'?0:0} max={field==='h'?23:59}
                value={field==='h'?selHour:field==='m'?selMin:selSec}
                oninput={(e) => {
                  const v = Math.max(0, Math.min(field==='h'?23:59, +(e.target as HTMLInputElement).value||0));
                  if(field==='h') selHour=v; else if(field==='m') selMin=v; else selSec=v;
                  commit();
                }} />
              <button class="wheel__btn" onclick={() => scrollField(field, -1)}>▾</button>
            </div>
          {/each}
        </div>
        <div class="clk__preview">{fmt(value)}</div>
      </div>
    </div>
  {/if}
</div>

<style>
  .dtp { position:relative; display:inline-block; }

  .dtp__trigger {
    display:flex; align-items:center; gap:7px; padding:6px 10px;
    background:var(--interactive-bg); border:1px solid var(--border-default);
    border-radius:4px; color:var(--text-secondary); cursor:pointer;
    font-family:'DM Mono',monospace; font-size:11px; transition:border-color .15s; white-space:nowrap;
  }
  .dtp__trigger:hover { border-color:var(--interactive-focus); }
  .dtp__tl { font-size:9px; letter-spacing:.1em; color:var(--text-faint); text-transform:uppercase; }
  .dtp__tv { color:var(--text-primary); }
  .dtp__ti { color:var(--text-faint); font-size:9px; }

  .dtp__drop {
    position:absolute; top:calc(100% + 6px); left:0; z-index:200;
    display:flex; background:var(--bg-overlay);
    border:1px solid var(--border-default); border-radius:6px;
    box-shadow:0 8px 32px rgba(0,0,0,.18); overflow:hidden;
  }

  /* Calendar */
  .cal { padding:14px; width:210px; }
  .cal__nav { display:flex; align-items:center; justify-content:space-between; margin-bottom:10px; }
  .cal__title { font-family:'DM Mono',monospace; font-size:11px; letter-spacing:.08em; color:var(--text-primary); }
  .cal__nb { background:none; border:none; color:var(--text-muted); font-size:16px; cursor:pointer; padding:0 4px; line-height:1; transition:color .12s; }
  .cal__nb:hover { color:var(--text-primary); }

  .cal__grid { display:grid; grid-template-columns:repeat(7,1fr); gap:2px; }
  .cal__dow { font-family:'DM Mono',monospace; font-size:9px; color:var(--text-faint); text-align:center; padding:3px 0; letter-spacing:.06em; }
  .cal__day {
    background:none; border:1px solid transparent; border-radius:3px;
    font-family:'DM Mono',monospace; font-size:10px; color:var(--text-muted);
    cursor:pointer; padding:4px 2px; text-align:center; transition:all .1s; line-height:1;
  }
  .cal__day:hover:not(.dis):not(.empty) { background:var(--interactive-hover); border-color:var(--border-default); color:var(--text-primary); }
  .cal__day.tod { color:var(--text-secondary); font-weight:500; }
  .cal__day.sel { background:var(--accent-bg); border-color:var(--accent-border); color:var(--accent-text); }
  .cal__day.dis { color:var(--text-faint); cursor:default; opacity:.4; }
  .cal__day.empty { cursor:default; }

  .dtp__divider { width:1px; background:var(--border-subtle); margin:10px 0; }

  /* Clock */
  .clk { padding:14px 16px; display:flex; flex-direction:column; align-items:center; gap:12px; justify-content:center; }
  .clk__lbl { font-family:'DM Mono',monospace; font-size:8px; letter-spacing:.18em; color:var(--text-faint); }
  .clk__display { display:flex; align-items:center; gap:4px; }
  .clk__sep { font-family:'DM Mono',monospace; font-size:20px; color:var(--border-default); line-height:1; margin-bottom:2px; }
  .wheel { display:flex; flex-direction:column; align-items:center; gap:3px; }
  .wheel__btn { background:none; border:none; color:var(--text-faint); font-size:11px; cursor:pointer; padding:2px 6px; transition:color .1s; line-height:1; }
  .wheel__btn:hover { color:var(--text-secondary); }
  .wheel__val {
    width:38px; text-align:center;
    background:var(--bg-inset); border:1px solid var(--border-subtle); border-radius:3px;
    color:var(--text-primary); font-family:'DM Mono',monospace; font-size:20px; font-weight:500;
    padding:6px 2px; -moz-appearance:textfield;
  }
  .wheel__val::-webkit-inner-spin-button, .wheel__val::-webkit-outer-spin-button { -webkit-appearance:none; }
  .wheel__val:focus { outline:none; border-color:var(--interactive-focus); }

  .clk__preview { font-family:'DM Mono',monospace; font-size:9px; color:var(--text-faint); letter-spacing:.06em; border-top:1px solid var(--border-subtle); padding-top:10px; width:100%; text-align:center; }
</style>