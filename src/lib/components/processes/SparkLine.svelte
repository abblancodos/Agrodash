<!-- src/lib/components/processes/SparkLine.svelte -->
<!-- SVG sparkline liviana — sin Chart.js, sin canvas, sin overhead -->
<script lang="ts">
  let { data = [], color = '#4a90d9', height = 24 }: {
    data:   number[];
    color?: string;
    height?: number;
  } = $props();

  const W = 200; // viewBox width, escala con el contenedor

  const points = $derived.by(() => {
    const vals = data.filter(v => isFinite(v));
    if (vals.length < 2) return '';
    const mn = Math.min(...vals);
    const mx = Math.max(...vals);
    const range = mx - mn || 1;
    const pad = 2;
    return vals.map((v, i) => {
      const x = (i / (vals.length - 1)) * W;
      const y = height - pad - ((v - mn) / range) * (height - pad * 2);
      return `${x.toFixed(1)},${y.toFixed(1)}`;
    }).join(' ');
  });

  // Área de relleno: añadir puntos de cierre abajo
  const area = $derived.by(() => {
    if (!points) return '';
    const pts = points.split(' ');
    const firstX = pts[0]?.split(',')[0] ?? '0';
    const lastX  = pts[pts.length - 1]?.split(',')[0] ?? String(W);
    return `${points} ${lastX},${height} ${firstX},${height}`;
  });
</script>

<svg
  viewBox="0 0 {W} {height}"
  preserveAspectRatio="none"
  width="100%"
  height={height}
  aria-hidden="true"
>
  {#if area}
    <polygon points={area} fill="{color}18" />
    <polyline points={points} fill="none" stroke={color} stroke-width="1.4" stroke-linejoin="round" stroke-linecap="round" />
  {/if}
</svg>

<style>
  svg { display:block; overflow:visible; }
</style>
