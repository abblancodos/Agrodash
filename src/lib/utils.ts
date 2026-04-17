// src/lib/utils.ts

/**
 * Convierte un timestamp ISO (UTC) a texto relativo legible.
 * Ej: "hace 30s", "hace 4 min", "hace 2 h", "hace 3 días", "hace 2 meses"
 */
export function relTime(isoUtc: string | null | undefined): string {
  if (!isoUtc) return 'sin datos';
  const secsAgo = Math.floor((Date.now() - new Date(isoUtc).getTime()) / 1000);
  if (secsAgo <  0)         return 'ahora';
  if (secsAgo <  60)        return `hace ${secsAgo}s`;
  if (secsAgo <  3_600)     return `hace ${Math.round(secsAgo / 60)} min`;
  if (secsAgo <  86_400)    return `hace ${Math.round(secsAgo / 3_600)} h`;
  if (secsAgo <  2_592_000) return `hace ${Math.round(secsAgo / 86_400)} días`;
  return `hace ${Math.round(secsAgo / 2_592_000)} meses`;
}

/**
 * Clase CSS semántica según la antigüedad del último dato.
 *   fresh   < 5 min   → verde
 *   recent  < 30 min  → gris (normal)
 *   stale   < 24 h    → ámbar
 *   dead    >= 24 h   → rojo
 */
export function relTimeClass(isoUtc: string | null | undefined): string {
  if (!isoUtc) return 'dead';
  const secsAgo = Math.floor((Date.now() - new Date(isoUtc).getTime()) / 1000);
  if (secsAgo < 300)    return 'fresh';
  if (secsAgo < 1_800)  return 'recent';
  if (secsAgo < 86_400) return 'stale';
  return 'dead';
}

/**
 * Clase CSS según anomaly_score (desviaciones estándar).
 *   normal  < 1.5σ
 *   warn    1.5–3σ
 *   alert   > 3σ
 */
export function anomalyClass(score: number | null | undefined): 'normal' | 'warn' | 'alert' {
  if (score === null || score === undefined) return 'normal';
  if (score >= 3.0) return 'alert';
  if (score >= 1.5) return 'warn';
  return 'normal';
}

/**
 * Formatea un valor numérico para display según el tipo de sensor.
 * Evita que se muestren 8 decimales innecesarios.
 */
export function formatValue(value: number | null, sensorType: string): string {
  if (value === null || value === undefined) return '—';
  const t = sensorType.toLowerCase();
  if (t.includes('temperatura') || t === 't' || t.includes('temperature')) {
    return value.toFixed(2);
  }
  if (t === 'ec' || t === 'e') return value.toFixed(4);
  if (t === 'p' || t.includes('presion') || t.includes('pressure')) {
    return value.toFixed(2);
  }
  if (t.includes('humedad') || t === 'vwc' || t.includes('humidity')) {
    return value.toFixed(2);
  }
  return value.toPrecision(4);
}