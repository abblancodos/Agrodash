<script lang="ts">
  import { fetchReadings, normaliseSensorLabel, sensorColor, type Box } from '$lib/api';
  import { downloading } from '$lib/stores/downloading';

  interface Props { box: Box; onclose: () => void; }
  let { box, onclose }: Props = $props();

  // ── Timezone ──────────────────────────────────────────────────────────────
  const userTz = Intl.DateTimeFormat().resolvedOptions().timeZone;
  const tzOffset = (() => {
    const off = -new Date().getTimezoneOffset();
    const h   = Math.floor(Math.abs(off) / 60);
    const m   = Math.abs(off) % 60;
    const s   = off >= 0 ? '+' : '-';
    return m ? `UTC${s}${h}:${String(m).padStart(2,'0')}` : `UTC${s}${h}`;
  })();

  function bucketToLocal(b: string) {
    return new Date(b + 'Z').toLocaleString('sv-SE', {
      timeZone: userTz, year:'numeric', month:'2-digit', day:'2-digit',
      hour:'2-digit', minute:'2-digit', second:'2-digit', hour12: false,
    }).replace('T',' ');
  }

  // ── Time range ────────────────────────────────────────────────────────────
  const PRESETS = [
    { label:'1h',    hours:1   },
    { label:'24h',   hours:24  },
    { label:'7d',    hours:168 },
    { label:'1 mes', hours:720 },
  ];
  let activePreset = $state<string|null>('24h');

  function fmt(d: Date) {
    return new Date(d.getTime() - d.getTimezoneOffset() * 60000).toISOString().slice(0,16);
  }
  let toDate   = $state(fmt(new Date()));
  let fromDate = $state(fmt(new Date(Date.now() - 24*3600000)));

  function applyPreset(p: {label:string;hours:number}) {
    activePreset = p.label;
    const now = new Date();
    toDate   = fmt(now);
    fromDate = fmt(new Date(now.getTime() - p.hours*3600000));
  }

  // ── Mode ──────────────────────────────────────────────────────────────────
  let mode = $state<'timeseries'|'stats'>('timeseries');

  // ── Resolution (timeseries only) ──────────────────────────────────────────
  const RES = [100,300,1000,3000,5000];
  const RESL= ['100','300','1 000','3 000','5 000'];
  let resStep = $state(2);
  const points = $derived(RES[resStep]);

  // ── Format (timeseries only) ──────────────────────────────────────────────
  let fmtMode = $state<'simple'|'descriptive'>('simple');

  // ── Column defs ───────────────────────────────────────────────────────────
  interface ColDef { sensorId:string; enabled:boolean; customLabel:string; }
  let cols = $state<ColDef[]>(box.sensors.map(s => ({
    sensorId:s.id, enabled:true, customLabel:'',
  })));

  function sensorOf(id:string) { return box.sensors.find(s=>s.id===id)!; }

  function autoLabel(col: ColDef) {
    const s = sensorOf(col.sensorId);
    return fmtMode === 'descriptive'
      ? `${normaliseSensorLabel(s.type)}(v/v%)_Sensor${s.sensor_number}`
      : `${normaliseSensorLabel(s.type)}_#${s.sensor_number}`;
  }
  function colLabel(col: ColDef) { return col.customLabel.trim() || autoLabel(col); }

  function moveCol(i:number, d:-1|1) {
    const ni=i+d; if(ni<0||ni>=cols.length) return;
    const next=[...cols]; [next[i],next[ni]]=[next[ni],next[i]]; cols=next;
  }

  const activeCols = $derived(cols.filter(c=>c.enabled));

  // ── Stat columns ──────────────────────────────────────────────────────────
  interface StatCol { key:string; label:string; enabled:boolean; customLabel:string; }
  let statCols = $state<StatCol[]>([
    { key:'mean',   label:'promedio',        enabled:true,  customLabel:'' },
    { key:'stddev', label:'desv. estándar',  enabled:true,  customLabel:'' },
    { key:'min',    label:'mínimo',          enabled:true,  customLabel:'' },
    { key:'max',    label:'máximo',          enabled:true,  customLabel:'' },
    { key:'count',  label:'n',               enabled:true,  customLabel:'' },
    { key:'range',  label:'rango',           enabled:false, customLabel:'' },
    { key:'cv',     label:'CV%',             enabled:false, customLabel:'' },
    { key:'p25',    label:'p25',             enabled:false, customLabel:'' },
    { key:'p75',    label:'p75',             enabled:false, customLabel:'' },
    { key:'p95',    label:'p95',             enabled:false, customLabel:'' },
  ]);
  function scLabel(sc:StatCol) { return sc.customLabel.trim()||sc.label; }
  const activeStats = $derived(statCols.filter(c=>c.enabled));

  // ── Advanced / preview mode ───────────────────────────────────────────────
  let advanced     = $state(false);
  let statsExpanded = $state(false); // stats column config starts collapsed

  // ── Estimated rows ────────────────────────────────────────────────────────
  // Density warning — show when the resulting resolution is very sparse
  const densityWarning = $derived(() => {
    if (mode === 'stats') return null;
    const from  = new Date(fromDate + ':00Z');
    const to    = new Date(toDate   + ':00Z');
    const hours = Math.max(0, (to.getTime() - from.getTime()) / 3_600_000);
    if (hours === 0) return null;
    const minPerPt = (hours * 60) / points;
    if (minPerPt < 5)   return null; // dense enough
    if (minPerPt < 30)  return { level: 'ok',   msg: `~1 dato cada ${Math.round(minPerPt)} min` };
    if (minPerPt < 120) return { level: 'warn', msg: `~1 dato cada ${Math.round(minPerPt)} min — podría ser poco denso` };
    const h = Math.round(minPerPt / 60);
    return { level: 'error', msg: `~1 dato cada ${h}h — subí la resolución o reducí el rango` };
  });

  const estRows = $derived(() => {
    const h = Math.max(0,(new Date(toDate+':00Z').getTime()-new Date(fromDate+':00Z').getTime())/3600000);
    return Math.min(points, Math.round(h*12)).toLocaleString('es-CR');
  });

  // ── Stats computation ─────────────────────────────────────────────────────
  function computeStats(vals: number[]) {
    if(!vals.length) return {} as Record<string,number>;
    const n=vals.length, sum=vals.reduce((a,b)=>a+b,0), mean=sum/n;
    const sorted=[...vals].sort((a,b)=>a-b);
    const stddev=Math.sqrt(vals.reduce((a,b)=>a+(b-mean)**2,0)/n);
    const min=sorted[0], max=sorted[n-1];
    const p=(pct:number)=>{const i=(pct/100)*(n-1),lo=Math.floor(i),hi=Math.ceil(i);return sorted[lo]+(sorted[hi]-sorted[lo])*(i-lo);};
    return {mean,stddev,min,max,count:n,range:max-min,cv:mean!==0?(stddev/Math.abs(mean))*100:0,p25:p(25),p75:p(75),p95:p(95)};
  }

  // ── Download ──────────────────────────────────────────────────────────────
  let error = $state('');

  async function download() {
    if(!activeCols.length) return;
    error='';
    const from=new Date(fromDate+':00Z'), to=new Date(toDate+':00Z');
    downloading.start(`${box.name} — preparando...`);
    try {
      if(mode==='timeseries') {
        const all: {col:ColDef;data:{bucket:string;value:number}[]}[]=[];
        for(let i=0;i<activeCols.length;i++){
          const col=activeCols[i], s=sensorOf(col.sensorId);
          downloading.setProgress(Math.round(i/activeCols.length*88),`${s.sensor_number} — ${normaliseSensorLabel(s.type)}`);
          all.push({col, data:await fetchReadings(s.id,s.type,from,to,points)});
        }
        downloading.setProgress(92,`construyendo CSV...`);
        const tsMap=new Map<string,Record<string,number|null>>();
        for(const{col,data}of all){
          for(const r of data){
            const ts=bucketToLocal(r.bucket);
            if(!tsMap.has(ts))tsMap.set(ts,{});
            tsMap.get(ts)![col.sensorId]=r.value;
          }
        }
        const header=[csvQ(`timestamp (${userTz}, ${tzOffset})`),...activeCols.map(c=>csvQ(colLabel(c)))].join(',');
        const rows=[...tsMap.entries()].sort(([a],[b])=>a.localeCompare(b))
          .map(([ts,vals])=>[ts,...activeCols.map(c=>{const v=vals[c.sensorId];return v!=null?v.toFixed(4):'';})].join(','));
        saveCSV([header,...rows].join('\n'), `${box.name.toLowerCase().replace(/\s+/g,'_')}_${fromDate.slice(0,10)}_${toDate.slice(0,10)}_${points}pts.csv`);

      } else {
        const rows:string[]=[];
        rows.push([csvQ('sensor'),...activeStats.map(sc=>csvQ(scLabel(sc)))].join(','));
        for(let i=0;i<activeCols.length;i++){
          const col=activeCols[i], s=sensorOf(col.sensorId);
          downloading.setProgress(Math.round(i/activeCols.length*90),`${normaliseSensorLabel(s.type)} #${s.sensor_number}`);
          const data=await fetchReadings(s.id,s.type,from,to,5000);
          const stats=computeStats(data.map(r=>r.value).filter(v=>!isNaN(v)));
          rows.push([csvQ(colLabel(col)),...activeStats.map(sc=>{const v=stats[sc.key];return v!=null?v.toFixed(sc.key==='count'?0:4):'';})].join(','));
        }
        rows.push('',`# ${new Date().toLocaleString('es-CR',{timeZone:userTz})}`,`# rango: ${fromDate} → ${toDate}`);
        saveCSV(rows.join('\n'), `${box.name.toLowerCase().replace(/\s+/g,'_')}_stats_${fromDate.slice(0,10)}_${toDate.slice(0,10)}.csv`);
      }
      downloading.finish(); setTimeout(onclose,500);
    } catch(e:any){ downloading.cancel(); error=e.message??'Error'; }
  }

  // Wrap a CSV field in quotes if it contains comma, quote, or newline
  function csvQ(s: string): string {
    if (/[,"
]/.test(s)) return '"' + s.replace(/"/g, '""') + '"';
    return s;
  }

  function saveCSV(csv:string, name:string) {
    downloading.setProgress(98,'guardando...');
    const blob=new Blob([csv],{type:'text/csv;charset=utf-8;'});
    const url=URL.createObjectURL(blob);
    const a=document.createElement('a'); a.href=url; a.download=name; a.click();
    URL.revokeObjectURL(url);
  }

  function cancel(){ downloading.cancel(); onclose(); }
</script>

<div class="overlay" onclick={cancel} role="presentation">
  <div class="panel" onclick={e=>e.stopPropagation()} onkeydown={e=>e.stopPropagation()}
       role="dialog" aria-modal="true" tabindex="-1">

    <!-- ── Header ────────────────────────────────────────────────────────── -->
    <div class="panel-head">
      <div class="head-left">
        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" class="head-icon">
          <path d="M8 2v9M4 7l4 5 4-5"/><line x1="2" y1="14" x2="14" y2="14"/>
        </svg>
        <span class="panel-title">Exportar CSV — {box.name}</span>
      </div>
      <button class="icon-btn" onclick={cancel} aria-label="Cerrar">
        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
          <line x1="4" y1="4" x2="12" y2="12"/><line x1="12" y1="4" x2="4" y2="12"/>
        </svg>
      </button>
    </div>

    <!-- ── Modo ──────────────────────────────────────────────────────────── -->
    <div class="section">
      <div class="section-label">tipo de exportación</div>
      <div class="mode-row">
        <button class="mode-btn" class:active={mode==='timeseries'} onclick={()=>mode='timeseries'}>
          <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" class="mode-icon">
            <polyline points="2,12 5,7 8,9 11,4 14,6"/><line x1="2" y1="14" x2="14" y2="14"/>
          </svg>
          <div>
            <div class="mode-label">serie temporal</div>
            <div class="mode-sub">una fila por timestamp</div>
          </div>
        </button>
        <button class="mode-btn" class:active={mode==='stats'} onclick={()=>mode='stats'}>
          <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" class="mode-icon">
            <rect x="2" y="8" width="3" height="6"/><rect x="6.5" y="5" width="3" height="9"/><rect x="11" y="2" width="3" height="12"/>
          </svg>
          <div>
            <div class="mode-label">resumen estadístico</div>
            <div class="mode-sub">una fila por sensor</div>
          </div>
        </button>
      </div>
    </div>

    <!-- ── Rango de tiempo ───────────────────────────────────────────────── -->
    <div class="section">
      <div class="section-label">
        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" class="s-icon">
          <circle cx="8" cy="8" r="6"/><polyline points="8,5 8,8 10.5,9.5"/>
        </svg>
        rango de tiempo
      </div>
      <div class="presets">
        {#each PRESETS as p (p.label)}
          <button class="pbtn" class:active={activePreset===p.label} onclick={()=>applyPreset(p)}>{p.label}</button>
        {/each}
      </div>
      <div class="date-row">
        <div class="date-field">
          <label for="csv-from" class="field-label">desde</label>
          <input id="csv-from" type="datetime-local" bind:value={fromDate} oninput={()=>activePreset=null} class="date-input"/>
        </div>
        <div class="date-sep">
          <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
            <line x1="2" y1="8" x2="14" y2="8"/><polyline points="10,4 14,8 10,12"/>
          </svg>
        </div>
        <div class="date-field">
          <label for="csv-to" class="field-label">hasta</label>
          <input id="csv-to" type="datetime-local" bind:value={toDate} oninput={()=>activePreset=null} class="date-input"/>
        </div>
      </div>
      <div class="tz-pill">
        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" class="tz-icon">
          <circle cx="8" cy="8" r="6"/><path d="M8 2c0 0-2 2-2 6s2 6 2 6M8 2c0 0 2 2 2 6s-2 6-2 6"/><line x1="2" y1="8" x2="14" y2="8"/>
        </svg>
        {userTz} · {tzOffset}
      </div>
    </div>

    <!-- ── Resolución (solo serie temporal) ─────────────────────────────── -->
    {#if mode === 'timeseries'}
    <div class="section">
      <div class="section-label">
        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" class="s-icon">
          <line x1="2" y1="14" x2="14" y2="14"/>
          <line x1="4" y1="14" x2="4" y2="10"/><line x1="7" y1="14" x2="7" y2="6"/>
          <line x1="10" y1="14" x2="10" y2="8"/><line x1="13" y1="14" x2="13" y2="3"/>
        </svg>
        resolución
        <span class="badge">{RESL[resStep]} pts/sensor</span>
      </div>
      <div class="res-row">
        <span class="res-tick">100</span>
        <input type="range" min="0" max="4" step="1" bind:value={resStep} class="res-slider"/>
        <span class="res-tick">5k</span>
      </div>
    </div>

    <!-- ── Formato (solo serie temporal) ────────────────────────────────── -->
    <div class="section">
      <div class="section-label">
        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" class="s-icon">
          <line x1="2" y1="4" x2="14" y2="4"/><line x1="2" y1="8" x2="10" y2="8"/><line x1="2" y1="12" x2="7" y2="12"/>
        </svg>
        formato de columnas
      </div>
      <div class="fmt-row">
        <button class="ftbtn" class:active={fmtMode==='simple'} onclick={()=>fmtMode='simple'}>
          <span class="ft-label">simple</span>
          <span class="ft-eg">humedad_#1</span>
        </button>
        <button class="ftbtn" class:active={fmtMode==='descriptive'} onclick={()=>fmtMode='descriptive'}>
          <span class="ft-label">descriptivo</span>
          <span class="ft-eg">VWC(v/v%)_Sensor1</span>
        </button>
      </div>
    </div>
    {/if}

    <!-- ── Sensores ──────────────────────────────────────────────────────── -->
    <div class="section">
      <div class="section-label">
        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" class="s-icon">
          <circle cx="5" cy="5" r="1.5"/><circle cx="11" cy="4" r="1.5"/>
          <circle cx="8" cy="9" r="1.5"/><circle cx="4" cy="12" r="1.5"/><circle cx="12" cy="11" r="1.5"/>
        </svg>
        sensores
        <span class="badge">{activeCols.length}/{cols.length}</span>
        <button class="link-btn" onclick={()=>{ const all=activeCols.length===cols.length; cols=cols.map(c=>({...c,enabled:!all})); }}>
          {activeCols.length===cols.length?'quitar todos':'todos'}
        </button>
        <button class="link-btn adv-toggle" onclick={()=>advanced=!advanced}>
          {advanced ? 'simple ↑' : 'avanzado ↓'}
        </button>
      </div>

      {#if !advanced}
        <!-- Vista básica -->
        <div class="sensor-list">
          {#each cols as col (col.sensorId)}
            {@const s=sensorOf(col.sensorId)}
            <label class="sensor-item">
              <input type="checkbox" bind:checked={col.enabled}/>
              <span class="sdot" style="background:{sensorColor(s.type)}"></span>
              <span class="sname">{normaliseSensorLabel(s.type)} <span class="snum">#{s.sensor_number}</span></span>
              <span class="slabel">{colLabel(col)}</span>
            </label>
          {/each}
        </div>

      {:else}
        <!-- Vista avanzada: spreadsheet preview -->
        <div class="sheet-wrap">
          <div class="sheet-scroll">
            <table class="sheet">
              <thead>
                <tr>
                  <th class="sh-th sh-th--ctrl"></th>
                  <th class="sh-th sh-th--sensor">sensor</th>
                  {#if mode==='timeseries'}
                    <th class="sh-th sh-th--label">nombre de columna</th>
                    <th class="sh-th sh-th--preview">preview</th>
                    <th class="sh-th sh-th--order">orden</th>
                  {:else}
                    <th class="sh-th sh-th--label">nombre de columna</th>
                    <th class="sh-th sh-th--stat-toggle">
                      <button class="stat-expand-btn" onclick={() => statsExpanded = !statsExpanded}>
                        {statsExpanded ? '▲ columnas' : '▼ columnas'}
                      </button>
                    </th>
                    {#if statsExpanded}
                      {#each statCols as sc (sc.key)}
                        <th class="sh-th sh-th--stat" class:sh-disabled={!sc.enabled}>
                          <label class="stat-hdr">
                            <input type="checkbox" bind:checked={sc.enabled}/>
                            <input class="stat-hdr-input" bind:value={sc.customLabel} placeholder={sc.label}/>
                          </label>
                        </th>
                      {/each}
                    {/if}
                  {/if}
                </tr>
              </thead>
              <tbody>
                {#each cols as col, i (col.sensorId)}
                  {@const s=sensorOf(col.sensorId)}
                  <tr class="sh-row" class:sh-row--disabled={!col.enabled}>
                    <td class="sh-td sh-td--ctrl">
                      <input type="checkbox" bind:checked={col.enabled}/>
                    </td>
                    <td class="sh-td sh-td--sensor">
                      <span class="sdot" style="background:{sensorColor(s.type)}"></span>
                      {normaliseSensorLabel(s.type)} #{s.sensor_number}
                    </td>
                    {#if mode==='timeseries'}
                      <td class="sh-td sh-td--editable">
                        <input class="sh-input" bind:value={col.customLabel} placeholder={autoLabel(col)}/>
                      </td>
                      <td class="sh-td sh-td--preview">{colLabel(col)}</td>
                      <td class="sh-td sh-td--order">
                        <button class="ord" onclick={()=>moveCol(i,-1)} disabled={i===0}>↑</button>
                        <button class="ord" onclick={()=>moveCol(i,1)} disabled={i===cols.length-1}>↓</button>
                      </td>
                    {:else}
                      <td class="sh-td sh-td--editable">
                        <input class="sh-input" bind:value={col.customLabel} placeholder={autoLabel(col)}/>
                      </td>
                      <td class="sh-td sh-td--stat-val">
                        <span class="stat-placeholder muted">{activeStats.length} stats</span>
                      </td>
                      {#if statsExpanded}
                        {#each statCols as sc (sc.key)}
                          <td class="sh-td sh-td--stat-val" class:sh-disabled={!sc.enabled}>
                            <span class="stat-placeholder">—</span>
                          </td>
                        {/each}
                      {/if}
                    {/if}
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
          <!-- Header preview strip -->
          <div class="header-preview">
            <span class="hp-label">header.csv</span>
            <code class="hp-code">
              {#if mode==='timeseries'}
                {[`timestamp (${userTz}, ${tzOffset})`,...activeCols.map(colLabel)].map(csvQ).join(',')}
              {:else}
                {['sensor',...activeStats.map(scLabel)].map(csvQ).join(',')}
              {/if}
            </code>
          </div>
        </div>
      {/if}
    </div>

    <!-- ── Footer: preview + botón ──────────────────────────────────────── -->
    <div class="panel-foot">
      <div class="preview-chips">
        <span class="chip">
          <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" class="chip-icon">
            <circle cx="5" cy="5" r="1.5"/><circle cx="11" cy="4" r="1.5"/>
            <circle cx="8" cy="9" r="1.5"/><circle cx="4" cy="12" r="1.5"/>
          </svg>
          {activeCols.length} sensor{activeCols.length!==1?'es':''}
        </span>
        {#if mode==='timeseries'}
          <span class="chip">~{estRows()} filas</span>
          <span class="chip">{RESL[resStep]} pts</span>
        {:else}
          <span class="chip">{activeCols.length} filas · {activeStats.length} cols estadísticas</span>
        {/if}
      </div>

      {#if error}
        <div class="error-msg">{error}</div>
      {/if}

      <button class="download-btn" onclick={download} disabled={activeCols.length===0}>
        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
          <path d="M8 2v9M4 7l4 5 4-5"/><line x1="2" y1="14" x2="14" y2="14"/>
        </svg>
        {mode==='timeseries'?'Descargar serie temporal':'Descargar resumen estadístico'}
      </button>
    </div>

  </div>
</div>

{#if $downloading.active}
  <div class="fullscreen-overlay" role="status" aria-live="polite">
    <div class="spinner-card">
      <div class="spinner"></div>
      <p class="spinner-label">{$downloading.label}</p>
      <div class="progress-track"><div class="progress-fill" style="width:{$downloading.progress}%"></div></div>
      <span class="progress-pct">{$downloading.progress}%</span>
      <button class="cancel-btn" onclick={cancel}>cancelar</button>
    </div>
  </div>
{/if}

<style>
  /* ── Layout ───────────────────────────────────────────────────────────── */
  .overlay {
    position:fixed; inset:0; z-index:100;
    background:rgba(0,0,0,.3); backdrop-filter:blur(2px);
    display:flex; align-items:center; justify-content:center;
  }
  .panel {
    background:var(--bg-surface);
    border:.5px solid var(--border-default);
    border-radius:14px;
    width:580px; max-width:calc(100vw - 24px);
    max-height:90vh; overflow-y:auto;
    display:flex; flex-direction:column;
    box-shadow:0 8px 40px rgba(0,0,0,.12);
  }

  /* ── Header ───────────────────────────────────────────────────────────── */
  .panel-head {
    display:flex; align-items:center; justify-content:space-between;
    padding:16px 20px 14px;
    border-bottom:.5px solid var(--border-subtle);
    position:sticky; top:0; background:var(--bg-surface); z-index:1;
  }
  .head-left { display:flex; align-items:center; gap:10px; }
  .head-icon { width:18px; height:18px; color:var(--text-muted); }
  .panel-title { font-size:calc(14px * var(--font-scale)); font-weight:500; color:var(--text-primary); font-family:'DM Mono',monospace; }
  .icon-btn { width:28px; height:28px; border:none; background:transparent; color:var(--text-muted); cursor:pointer; border-radius:5px; display:flex; align-items:center; justify-content:center; }
  .icon-btn svg { width:14px; height:14px; }
  .icon-btn:hover { background:var(--interactive-hover); }

  /* ── Sections ─────────────────────────────────────────────────────────── */
  .section { padding:16px 20px; border-bottom:.5px solid var(--border-subtle); display:flex; flex-direction:column; gap:12px; }
  .section-label {
    display:flex; align-items:center; gap:7px;
    font-size:calc(11px * var(--font-scale)); font-family:'DM Mono',monospace;
    letter-spacing:.06em; text-transform:uppercase; color:var(--text-muted);
  }
  .s-icon { width:13px; height:13px; flex-shrink:0; }
  .badge { font-size:calc(11px * var(--font-scale)); background:var(--bg-elevated); color:var(--text-primary); padding:1px 7px; border-radius:4px; font-weight:500; }
  .link-btn { font-size:calc(11px * var(--font-scale)); font-family:'DM Mono',monospace; color:var(--text-muted); border:none; background:transparent; cursor:pointer; padding:0; text-decoration:underline; }
  .adv-toggle { margin-left:auto; }

  /* ── Mode selector ────────────────────────────────────────────────────── */
  .mode-row { display:flex; gap:10px; }
  .mode-btn {
    flex:1; display:flex; align-items:center; gap:12px;
    padding:12px 14px; border:.5px solid var(--border-default);
    border-radius:9px; background:transparent; cursor:pointer; transition:all .12s; text-align:left;
  }
  .mode-btn:hover { background:var(--interactive-hover); }
  .mode-btn.active { border-color:var(--text-primary); background:var(--bg-elevated); }
  .mode-icon { width:20px; height:20px; flex-shrink:0; color:var(--text-muted); }
  .mode-btn.active .mode-icon { color:var(--text-primary); }
  .mode-label { font-size:calc(13px * var(--font-scale)); font-weight:500; color:var(--text-primary); font-family:'DM Mono',monospace; }
  .mode-sub   { font-size:calc(10px * var(--font-scale)); color:var(--text-muted); margin-top:1px; }

  /* ── Presets ──────────────────────────────────────────────────────────── */
  .presets { display:flex; gap:6px; }
  .pbtn {
    padding:6px 14px; border:.5px solid var(--border-default); border-radius:6px;
    background:transparent; color:var(--text-secondary); font-family:'DM Mono',monospace;
    font-size:calc(13px * var(--font-scale)); cursor:pointer; transition:all .1s;
  }
  .pbtn:hover { background:var(--interactive-hover); }
  .pbtn.active { background:var(--text-primary); color:var(--bg-surface); border-color:transparent; }

  /* ── Dates ────────────────────────────────────────────────────────────── */
  .date-row { display:flex; align-items:flex-end; gap:8px; flex-wrap:wrap; }
  .date-field { display:flex; flex-direction:column; gap:4px; flex:1; min-width:155px; }
  .field-label { font-size:calc(11px * var(--font-scale)); color:var(--text-muted); font-family:'DM Mono',monospace; }
  .date-input {
    padding:7px 10px; border:.5px solid var(--border-default); border-radius:6px;
    background:var(--bg-elevated); color:var(--text-primary);
    font-family:'DM Mono',monospace; font-size:calc(13px * var(--font-scale)); outline:none; width:100%;
  }
  .date-input:focus { border-color:var(--text-primary); }
  .date-sep { padding-bottom:9px; color:var(--text-muted); }
  .date-sep svg { width:14px; height:14px; }
  .tz-pill {
    display:inline-flex; align-items:center; gap:5px; align-self:flex-start;
    padding:3px 9px; background:var(--bg-elevated); border-radius:20px;
    font-size:calc(11px * var(--font-scale)); color:var(--text-muted); font-family:'DM Mono',monospace;
  }
  .tz-icon { width:11px; height:11px; }

  /* ── Resolution ───────────────────────────────────────────────────────── */
  .res-row { display:flex; align-items:center; gap:12px; }
  .res-tick { font-size:calc(11px * var(--font-scale)); color:var(--text-muted); font-family:'DM Mono',monospace; min-width:26px; }
  .res-slider { flex:1; }

  /* ── Format ───────────────────────────────────────────────────────────── */
  .fmt-row { display:flex; gap:8px; }
  .ftbtn {
    flex:1; display:flex; flex-direction:column; gap:3px;
    padding:10px 12px; border:.5px solid var(--border-default); border-radius:7px;
    background:transparent; cursor:pointer; text-align:left; transition:all .1s;
  }
  .ftbtn:hover { background:var(--interactive-hover); }
  .ftbtn.active { border-color:var(--text-primary); background:var(--bg-elevated); }
  .ft-label { font-size:calc(12px * var(--font-scale)); font-weight:500; color:var(--text-primary); font-family:'DM Mono',monospace; }
  .ft-eg    { font-size:calc(10px * var(--font-scale)); color:var(--text-muted); font-family:'DM Mono',monospace; }

  /* ── Sensor list (basic) ──────────────────────────────────────────────── */
  .sensor-list { display:flex; flex-direction:column; gap:1px; max-height:180px; overflow-y:auto; }
  .sensor-item {
    display:flex; align-items:center; gap:9px; padding:6px 8px;
    border-radius:5px; cursor:pointer; font-size:calc(13px * var(--font-scale)); transition:background .1s;
  }
  .sensor-item:hover { background:var(--interactive-hover); }
  .sensor-item input[type="checkbox"] { width:13px; height:13px; cursor:pointer; }
  .sdot { width:7px; height:7px; border-radius:50%; flex-shrink:0; }
  .sname { flex:1; color:var(--text-primary); }
  .snum  { color:var(--text-muted); font-size:calc(12px * var(--font-scale)); }
  .slabel { font-size:calc(10px * var(--font-scale)); color:var(--text-muted); font-family:'DM Mono',monospace; }

  /* ── Spreadsheet (advanced) ───────────────────────────────────────────── */
  .sheet-wrap { display:flex; flex-direction:column; gap:8px; }
  .sheet-scroll { overflow-x:auto; border:.5px solid var(--border-default); border-radius:8px; }
  .sheet { border-collapse:collapse; width:100%; font-size:calc(12px * var(--font-scale)); }

  .sh-th {
    padding:7px 10px; background:var(--bg-elevated); border-bottom:.5px solid var(--border-default);
    font-size:calc(10px * var(--font-scale)); font-family:'DM Mono',monospace; letter-spacing:.05em;
    color:var(--text-muted); text-transform:uppercase; white-space:nowrap;
    border-right:.5px solid var(--border-subtle); text-align:left; font-weight:500;
  }
  .sh-th:last-child { border-right:none; }
  .sh-th--ctrl  { width:28px; }
  .sh-th--sensor{ min-width:140px; }
  .sh-th--label { min-width:160px; }
  .sh-th--preview { min-width:120px; color:var(--text-muted); }
  .sh-th--order { width:52px; }
  .sh-th--stat  { min-width:80px; }
  .sh-disabled  { opacity:.35; }

  .stat-hdr { display:flex; align-items:center; gap:4px; cursor:pointer; }
  .stat-hdr input[type="checkbox"] { width:11px; height:11px; flex-shrink:0; }
  .stat-hdr-input {
    border:none; background:transparent; font-size:calc(10px * var(--font-scale));
    font-family:'DM Mono',monospace; color:var(--text-muted); outline:none; width:100%;
    cursor:pointer;
  }
  .stat-hdr-input:focus { color:var(--text-primary); }

  .sh-row { border-bottom:.5px solid var(--border-subtle); transition:background .08s; }
  .sh-row:hover { background:var(--interactive-hover); }
  .sh-row:last-child { border-bottom:none; }
  .sh-row--disabled { opacity:.45; }

  .sh-td {
    padding:6px 10px; border-right:.5px solid var(--border-subtle);
    color:var(--text-primary); vertical-align:middle;
  }
  .sh-td:last-child { border-right:none; }
  .sh-td--ctrl   { text-align:center; }
  .sh-td--sensor { display:flex; align-items:center; gap:7px; white-space:nowrap; font-family:'DM Mono',monospace; font-size:calc(11px * var(--font-scale)); color:var(--text-secondary); }
  .sh-td--preview { font-family:'DM Mono',monospace; font-size:calc(11px * var(--font-scale)); color:var(--text-muted); }
  .sh-td--order  { white-space:nowrap; }
  .sh-td--stat-val { text-align:center; }
  .sh-td--editable { padding:3px 6px; }
  .stat-placeholder { color:var(--border-default); font-size:calc(10px * var(--font-scale)); }

  .sh-input {
    width:100%; padding:4px 7px;
    border:.5px solid transparent; border-radius:4px;
    background:transparent; color:var(--text-primary);
    font-size:calc(12px * var(--font-scale)); font-family:'DM Mono',monospace; outline:none;
  }
  .sh-input:hover { border-color:var(--border-default); background:var(--bg-elevated); }
  .sh-input:focus { border-color:var(--text-primary); background:var(--bg-elevated); }

  .ord {
    width:18px; height:18px; border:none; background:none; cursor:pointer;
    font-size:11px; color:var(--text-muted); padding:0; border-radius:3px;
  }
  .ord:hover:not(:disabled) { background:var(--interactive-hover); color:var(--text-primary); }
  .ord:disabled { opacity:.2; }

  .header-preview {
    display:flex; align-items:flex-start; gap:8px;
    padding:8px 10px; background:var(--bg-inset); border-radius:6px;
    font-size:calc(11px * var(--font-scale));
  }
  .hp-label { color:var(--text-muted); font-family:'DM Mono',monospace; flex-shrink:0; }
  .hp-code  { color:var(--text-secondary); font-family:'DM Mono',monospace; word-break:break-all; white-space:pre-wrap; }

  /* ── Footer ───────────────────────────────────────────────────────────── */
  .panel-foot {
    padding:16px 20px; display:flex; flex-direction:column; gap:10px;
    position:sticky; bottom:0; background:var(--bg-surface);
    border-top:.5px solid var(--border-subtle);
  }
  .preview-chips { display:flex; align-items:center; gap:6px; flex-wrap:wrap; }
  .chip {
    display:inline-flex; align-items:center; gap:4px;
    padding:3px 9px; background:var(--bg-elevated); border:.5px solid var(--border-subtle);
    border-radius:20px; font-size:calc(11px * var(--font-scale));
    color:var(--text-muted); font-family:'DM Mono',monospace;
  }
  .chip-icon { width:10px; height:10px; }
  .error-msg { padding:8px 12px; background:var(--error-bg); border:.5px solid #F09595; border-radius:5px; font-size:calc(13px * var(--font-scale)); color:#A32D2D; }
  .download-btn {
    display:flex; align-items:center; justify-content:center; gap:8px;
    padding:11px; background:var(--text-primary); color:var(--bg-surface);
    border:none; border-radius:8px; font-family:'DM Mono',monospace;
    font-size:calc(14px * var(--font-scale)); font-weight:500; letter-spacing:.04em; cursor:pointer; transition:opacity .15s;
  }
  .download-btn svg { width:14px; height:14px; }
  .download-btn:hover:not(:disabled) { opacity:.85; }
  .download-btn:disabled { opacity:.4; cursor:not-allowed; }

  /* ── Fullscreen overlay ───────────────────────────────────────────────── */
  .fullscreen-overlay {
    position:fixed; inset:0; z-index:200; background:rgba(0,0,0,.55);
    backdrop-filter:blur(5px); display:flex; align-items:center; justify-content:center;
  }
  .spinner-card {
    display:flex; flex-direction:column; align-items:center; gap:14px;
    padding:36px 44px; background:var(--bg-surface);
    border:.5px solid var(--border-default); border-radius:14px; min-width:280px;
  }
  .spinner { width:36px; height:36px; border:3px solid var(--border-subtle); border-top-color:var(--text-primary); border-radius:50%; animation:spin .7s linear infinite; }
  @keyframes spin { to { transform:rotate(360deg); } }
  .spinner-label { font-size:calc(13px * var(--font-scale)); font-family:'DM Mono',monospace; color:var(--text-secondary); text-align:center; max-width:240px; line-height:1.6; margin:0; }
  .progress-track { width:100%; height:3px; background:var(--border-subtle); border-radius:2px; overflow:hidden; }
  .progress-fill  { height:100%; background:var(--text-primary); transition:width .25s ease; }
  .progress-pct   { font-size:calc(12px * var(--font-scale)); font-family:'DM Mono',monospace; color:var(--text-muted); }
  .cancel-btn { padding:5px 18px; border:.5px solid var(--border-default); border-radius:5px; background:transparent; color:var(--text-muted); font-family:'DM Mono',monospace; font-size:calc(13px * var(--font-scale)); cursor:pointer; }
  .cancel-btn:hover { background:var(--interactive-hover); }

  .density-warn {
    display: flex; align-items: center; gap: 6px;
    padding: 7px 10px; border-radius: 6px;
    font-size: calc(12px * var(--font-scale)); font-family: 'DM Mono', monospace;
  }
  .density-warn--ok    { background: var(--bg-elevated); color: var(--text-muted); }
  .density-warn--warn  { background: #FEF3C7; color: #92400E; border: 0.5px solid #D97706; }
  .density-warn--error { background: var(--error-bg); color: #A32D2D; border: 0.5px solid #F09595; }
  .dw-icon { width: 13px; height: 13px; flex-shrink: 0; }

  .sh-th--stat-toggle { min-width: 90px; }
  .stat-expand-btn {
    font-size: calc(10px * var(--font-scale)); font-family: 'DM Mono', monospace;
    color: var(--text-muted); background: none; border: 0.5px solid var(--border-default);
    border-radius: 4px; padding: 2px 7px; cursor: pointer; white-space: nowrap;
  }
  .stat-expand-btn:hover { background: var(--interactive-hover); color: var(--text-primary); }
  .muted { color: var(--border-default); font-size: calc(10px * var(--font-scale)); }

  @media (max-width:640px) {
    .panel { width:calc(100vw - 16px); max-height:88vh; }
    .mode-row { flex-direction:column; }
    .fmt-row  { flex-direction:column; }
    .date-row { flex-direction:column; }
    .date-sep { display:none; }
  }
</style>