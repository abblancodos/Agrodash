// src/lib/api.ts
// ─────────────────────────────────────────────────────────────────────────────
// Single source of truth for all backend calls.
// Replace API_BASE with your real Axum server when ready.
// Every function mirrors an endpoint that the Axum API will expose.
// ─────────────────────────────────────────────────────────────────────────────

export const API_BASE = import.meta.env.VITE_API_BASE ?? 'http://localhost:3000';

// ── Types ─────────────────────────────────────────────────────────────────────

export interface Sensor {
  id: string;
  sensor_number: number;
  type: string;            // raw from DB — normalised display in UI
}

export interface Box {
  id: string;
  name: string;
  sensors: Sensor[];
}

export interface Reading {
  bucket: string;          // ISO timestamp (bucketed by Axum)
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

// ── Normalise sensor type label ───────────────────────────────────────────────
// The DB has inconsistent casing and aliases. Map them to clean display names.
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

// ── Mock data (replace calls with real fetch when Axum is up) ─────────────────

const MOCK_BOXES: Box[] = [
  {
    id: 'd59e83cd-de61-4e3a-83cd-66514508e3d2',
    name: 'Caja S',
    sensors: [
      { id: 'c4890e79', sensor_number: 3, type: 'calibrada' },
      { id: 'ed8f2168', sensor_number: 4, type: 'calibrada' },
      { id: '656e3cd0', sensor_number: 5, type: 'calibrada' },
      { id: '7a0a5fe9', sensor_number: 6, type: 'calibrada' },
      { id: 'c81de4ee', sensor_number: 1, type: 'temperatura' },
      { id: '5d9f6fa2', sensor_number: 1, type: 'ec' },
      { id: 'f4c6c0d2', sensor_number: 1, type: 'p' },
      { id: 'efb4cf66', sensor_number: 1, type: 'humedad' },
    ],
  },
  {
    id: '049b78cf-c4d9-4ec8-94c4-47b4ac33a45d',
    name: 'z6-15052',
    sensors: [
      { id: 'da394071', sensor_number: 1, type: 'Air Temperature' },
      { id: 'e946b1d9', sensor_number: 1, type: 'Solar Radiation' },
      { id: 'aab33e59', sensor_number: 1, type: 'Wind Speed' },
      { id: '54460e84', sensor_number: 1, type: 'Precipitation' },
      { id: '7ed3add7', sensor_number: 1, type: 'VPD' },
      { id: '8291d52b', sensor_number: 1, type: 'Atmospheric Pressure' },
      { id: '9bbee085', sensor_number: 7, type: 'Battery Percent' },
    ],
  },
  {
    id: '729f650a-47f4-467e-bfd1-f374ce2e4ca9',
    name: 'Caja A',
    sensors: [
      { id: 'dfd691be', sensor_number: 1, type: 'temperatura' },
      { id: '859aa9d8', sensor_number: 2, type: 'temperatura' },
      { id: '9cfdee03', sensor_number: 3, type: 'temperatura' },
      { id: '1bc82dd3', sensor_number: 4, type: 'temperatura' },
      { id: 'a5dc710d', sensor_number: 1, type: 'humedad' },
      { id: '70d9af9e', sensor_number: 2, type: 'humedad' },
      { id: '7f843a4e', sensor_number: 3, type: 'humedad' },
      { id: '61fd5107', sensor_number: 4, type: 'humedad' },
    ],
  },
  {
    id: 'de8dfddb-d167-4ead-afd4-bb9718982e5a',
    name: 'Caja Abioticos 1 SC',
    sensors: [
      { id: '4353ec76', sensor_number: 1, type: 'temperatura' },
      { id: 'be44c7f5', sensor_number: 1, type: 'humedadAire' },
      { id: 'c034a1fd', sensor_number: 1, type: 'humedadSuelo' },
      { id: '8c94cf98', sensor_number: 1, type: 'radiacionPar' },
      { id: '5bf1caff', sensor_number: 4, type: 'radiacionPar' },
    ],
  },
  {
    id: 'fa33b52a-6c85-4d47-a11b-556cc4df189c',
    name: 'Campbell',
    sensors: [
      { id: '8f9ea3cb', sensor_number: 1, type: 'T' },
      { id: '1da2e66f', sensor_number: 2, type: 'T' },
      { id: 'cde8cdf0', sensor_number: 1, type: 'EC' },
      { id: 'cb7dfdad', sensor_number: 4, type: 'VWC' },
      { id: 'aff797c0', sensor_number: 2, type: 'P' },
      { id: '584509df', sensor_number: 5, type: 'VR' },
    ],
  },
];

// Generate realistic mock time-series readings
function generateMockReadings(sensorType: string, from: Date, to: Date, points = 60): Reading[] {
  const ms = (to.getTime() - from.getTime()) / points;
  const type = sensorType.toLowerCase();
  let base = 25, amplitude = 3, noise = 0.5;
  if (type.includes('humedad') || type === 'vwc') { base = 65; amplitude = 10; noise = 2; }
  if (type === 'ec' || type === 'e') { base = 1.8; amplitude = 0.4; noise = 0.05; }
  if (type === 'p' || type.includes('presion') || type.includes('pressure')) { base = 101.3; amplitude = 0.5; noise = 0.1; }
  if (type === 'vpd') { base = 1.2; amplitude = 0.6; noise = 0.1; }
  if (type.includes('radiation') || type.includes('radiacion') || type.includes('irradiancia')) { base = 400; amplitude = 350; noise = 20; }
  if (type.includes('wind') || type.includes('viento')) { base = 2; amplitude = 3; noise = 0.5; }
  if (type.includes('precipitation') || type.includes('precipitacion')) { base = 0.2; amplitude = 0.8; noise = 0.1; }
  if (type === 'battery percent') { base = 85; amplitude = 5; noise = 0.2; }

  return Array.from({ length: points }, (_, i) => ({
    bucket: new Date(from.getTime() + i * ms).toISOString(),
    value: +(base + amplitude * Math.sin(i / 8) + (Math.random() - 0.5) * noise * 2).toFixed(3),
  }));
}

// ── API functions ─────────────────────────────────────────────────────────────

export async function fetchBoxes(): Promise<Box[]> {
  // TODO: replace with real call
  // const res = await fetch(`${API_BASE}/api/v1/boxes`);
  // if (!res.ok) throw new Error(`HTTP ${res.status}`);
  // return res.json();
  await new Promise(r => setTimeout(r, 400));
  return MOCK_BOXES;
}

export async function fetchReadings(
  sensorId: string,
  sensorType: string,
  from: Date,
  to: Date,
  points = 120,
): Promise<Reading[]> {
  // TODO: replace with real call
  // const params = new URLSearchParams({
  //   sensor_id: sensorId,
  //   from: from.toISOString(),
  //   to: to.toISOString(),
  //   points: String(points),
  // });
  // const res = await fetch(`${API_BASE}/api/v1/readings?${params}`);
  // if (!res.ok) throw new Error(`HTTP ${res.status}`);
  // return res.json();
  await new Promise(r => setTimeout(r, 300));
  return generateMockReadings(sensorType, from, to, points);
}

export async function fetchLatestReadings(boxId: string): Promise<LiveReading[]> {
  // TODO: replace with real call
  // const res = await fetch(`${API_BASE}/api/v1/boxes/${boxId}/readings/latest`);
  // return res.json();
  await new Promise(r => setTimeout(r, 200));
  return [];
}

export async function fetchTemperature(): Promise<number> {
  // TODO: replace with real call
  // const res = await fetch(`${API_BASE}/api/v1/environment/temperature`);
  // const data: TemperatureResponse = await res.json();
  // return data.temperature_c;
  return 24.3 + Math.random() * 2;
}

// ── Time range ────────────────────────────────────────────────────────────────
// Returns the first and last reading timestamps across all sensors.
// Axum endpoint: GET /api/v1/readings/time-range
// Response: { "first": "2025-11-28T12:50:37Z", "last": "2025-12-16T08:25:43Z" }

export interface TimeRange {
  first: Date;
  last: Date;
}

export async function fetchTimeRange(): Promise<TimeRange> {
  // TODO: replace with real call
  // const res = await fetch(`${API_BASE}/api/v1/readings/time-range`);
  // if (!res.ok) throw new Error(`HTTP ${res.status}`);
  // const data = await res.json();
  // return { first: new Date(data.first), last: new Date(data.last) };

  // Mock: matches the real data seen in the DB
  await new Promise(r => setTimeout(r, 250));
  return {
    first: new Date('2025-11-28T12:50:37'),
    last:  new Date('2025-12-16T08:25:43'),
  };
}