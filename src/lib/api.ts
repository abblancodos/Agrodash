// src/lib/api.ts
// ─────────────────────────────────────────────────────────────────────────────
// Single source of truth for all backend calls.
// ─────────────────────────────────────────────────────────────────────────────

export const API_BASE = import.meta.env.VITE_API_BASE ?? 'http://localhost:3000';

// ── Types ─────────────────────────────────────────────────────────────────────

export interface Sensor {
  id: string;
  sensor_number: number;
  type: string;
}

export interface Box {
  id: string;
  name: string;
  sensors: Sensor[];
}

export interface Reading {
  bucket: string;
  value: number;
}

export interface LiveReading {
  sensor_id: string;
  value: number;
  created_at: string;
}

export interface TemperatureResponse {
  temperature_c: number;
}

export interface TimeRange {
  first: Date;
  last: Date;
}

// ── Normalise sensor type label ───────────────────────────────────────────────
const TYPE_LABELS: Record<string, string> = {
  temperatura: 'Temperatura',
  temperatura1: 'Temperatura 1', temperatura2: 'Temperatura 2',
  temperatura3: 'Temperatura 3', temperatura4: 'Temperatura 4',
  humedad: 'Humedad',
  humedad1: 'Humedad 1', humedad2: 'Humedad 2', humedad3: 'Humedad 3',
  humedad4: 'Humedad 4', humedad5: 'Humedad 5', humedad6: 'Humedad 6',
  humedadaire: 'Humedad Aire',
  humedadsuelo: 'Humedad Suelo',
  calibrada: 'Calibrada',
  polinomial: 'Polinomial',
  ec: 'EC', e: 'EC',
  p: 'Presión',
  vwc: 'VWC', vr: 'VR',
  t: 'Temperatura',
  radiacionpar: 'Radiación PAR',
  irradiancia: 'Irradiancia',
  'wind speed': 'Velocidad Viento', 'wind speed (m/s)': 'Velocidad Viento',
  'wind direction': 'Dirección Viento', 'wind direction (°)': 'Dirección Viento',
  'air temperature': 'Temperatura Aire', 'air temperature (°c)': 'Temperatura Aire',
  'solar radiation': 'Radiación Solar',
  'atmospheric pressure': 'Presión Atm', 'atmospheric pressure (°)': 'Presión Atm',
  'vapor pressure': 'Presión Vapor', 'vapor pressure (kpa)': 'Presión Vapor',
  vpd: 'VPD', 'vpd (kpa)': 'VPD',
  precipitation: 'Precipitación',
  'gust speed': 'Ráfaga', 'gust speed (m/s)': 'Ráfaga',
  'battery percent': 'Batería %', 'battery voltage': 'Batería V',
  'lightning activity': 'Act. Rayos', 'lightning distance': 'Dist. Rayos',
  'rh sensor temp': 'Temp. Sensor HR', 'rh sensor temp (°c)': 'Temp. Sensor HR',
};

export function normaliseSensorLabel(raw: string): string {
  return TYPE_LABELS[raw.toLowerCase()] ?? raw;
}

// ── Color palette per sensor type family ─────────────────────────────────────
const TYPE_COLORS: Record<string, string> = {
  temperatura: '#e07b54', temperatura1: '#e07b54', temperatura2: '#c45e3a',
  temperatura3: '#a84525', temperatura4: '#e8956e',
  t: '#e07b54', 'air temperature': '#e07b54', 'air temperature (°c)': '#e07b54',
  humedad: '#4a90d9', humedad1: '#4a90d9', humedad2: '#357abd',
  humedad3: '#2163a6', humedad4: '#6aaee3', humedad5: '#89c0ec',
  humedad6: '#aad3f5', humedadaire: '#5ba3e0', humedadsuelo: '#2e6da4',
  calibrada: '#7c6fcd', polinomial: '#9b8ae0',
  ec: '#3da85a', e: '#3da85a', vwc: '#2d8a47', vr: '#5bbf70',
  p: '#e8a838', 'atmospheric pressure': '#e8a838',
  radiacionpar: '#f0c040', irradiancia: '#f5d060', 'solar radiation': '#f5d060',
  'wind speed': '#78c4b8', 'gust speed': '#56b0a4',
  precipitation: '#6194d4',
  vpd: '#d47cb0',
  default: '#8a9bb0',
};

export function sensorColor(raw: string): string {
  return TYPE_COLORS[raw.toLowerCase()] ?? TYPE_COLORS.default;
}

// ── API functions ─────────────────────────────────────────────────────────────

export async function fetchBoxes(): Promise<Box[]> {
  const res = await fetch(`${API_BASE}/api/v1/boxes`);
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}

export async function fetchReadings(
  sensorId: string,
  _sensorType: string,
  from: Date,
  to: Date,
  points = 300,
): Promise<Reading[]> {
  const fmt = (d: Date) => d.toISOString().slice(0, 19) + 'Z';
  const params = new URLSearchParams({
    sensor_id: sensorId,
    from: fmt(from),
    to:   fmt(to),
    points: String(points),
  });
  const res = await fetch(`${API_BASE}/api/v1/readings?${params}`);
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}

export async function fetchLatestReadings(_boxId: string): Promise<LiveReading[]> {
  return [];
}

export async function fetchTemperature(): Promise<number> {
  const res = await fetch(`${API_BASE}/api/v1/environment/temperature`);
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  const data: TemperatureResponse = await res.json();
  return data.temperature_c;
}

export async function fetchTimeRange(): Promise<TimeRange> {
  const res = await fetch(`${API_BASE}/api/v1/readings/time-range`);
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  const data = await res.json();
  return { first: new Date(data.first + 'Z'), last: new Date(data.last + 'Z') };
}

export interface LastReading {
  bucket: string;
  value: number;
}

export async function fetchLastReading(sensorId: string): Promise<LastReading | null> {
  const res = await fetch(`${API_BASE}/api/v1/readings/last?sensor_id=${sensorId}`);
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}