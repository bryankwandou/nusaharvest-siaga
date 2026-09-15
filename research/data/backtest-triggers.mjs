// Backtest of rainfall triggers on real reanalysis data (Open-Meteo historical API, ERA5/ERA5-Land).
// Purpose: research evidence only. Compares the V1 absolute trigger (<40 mm in 30 days)
// with an anomaly trigger (30-day rainfall below the 10th percentile for the same calendar window).
import { writeFileSync } from 'node:fs';

const SITES = [
  { id: 'klaten',   name: 'Klaten, Jawa Tengah (V1 demo site, mostly irrigated rice)', lat: -7.7078, lon: 110.6101 },
  { id: 'grobogan', name: 'Grobogan, Jawa Tengah (rainfed rice and maize)',            lat: -7.0900, lon: 110.9200 },
  { id: 'kupang',   name: 'Kupang, NTT (rainfed maize, dry climate)',                   lat: -10.1700, lon: 123.6100 },
  { id: 'demak',    name: 'Demak, Jawa Tengah (flood-prone rice)',                      lat: -6.8900, lon: 110.6400 },
];
const START = '1991-01-01', END = '2025-12-31';
const BASE_FROM = 1991, BASE_TO = 2020;

async function getDaily(site) {
  const url = `https://archive-api.open-meteo.com/v1/archive?latitude=${site.lat}&longitude=${site.lon}` +
    `&start_date=${START}&end_date=${END}&daily=precipitation_sum&timezone=Asia%2FJakarta`;
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${site.id}: HTTP ${res.status}`);
  const j = await res.json();
  return j.daily.time.map((t, i) => ({ t, p: j.daily.precipitation_sum[i] ?? 0 }));
}

function rolling(arr, n) {
  const out = new Array(arr.length).fill(null);
  let s = 0;
  for (let i = 0; i < arr.length; i++) {
    s += arr[i].p;
    if (i >= n) s -= arr[i - n].p;
    if (i >= n - 1) out[i] = s;
  }
  return out;
}

function pct(sorted, q) {
  if (!sorted.length) return null;
  const idx = (sorted.length - 1) * q, lo = Math.floor(idx), hi = Math.ceil(idx);
  return sorted[lo] + (sorted[hi] - sorted[lo]) * (idx - lo);
}

const summary = [];
for (const site of SITES) {
  const d = await getDaily(site);
  const r30 = rolling(d, 30);
  const r3 = rolling(d, 3);
  // day-of-year keyed climatology (±7 day window) for 30-day totals, baseline 1991-2020
  const doy = (t) => { const x = new Date(t + 'T00:00:00Z'); return Math.floor((x - Date.UTC(x.getUTCFullYear(), 0, 0)) / 864e5); };
  const byDoy = Array.from({ length: 367 }, () => []);
  d.forEach((row, i) => {
    const y = +row.t.slice(0, 4);
    if (r30[i] == null || y < BASE_FROM || y > BASE_TO) return;
    const k = doy(row.t);
    for (let w = -7; w <= 7; w++) byDoy[((k + w - 1 + 366) % 366) + 1].push(r30[i]);
  });
  const p10 = byDoy.map(v => pct(v.sort((a, b) => a - b), 0.10));
  const all3 = r3.filter((v, i) => v != null && +d[i].t.slice(0, 4) >= BASE_FROM && +d[i].t.slice(0, 4) <= BASE_TO).sort((a, b) => a - b);
  const flood99 = pct(all3, 0.995);

  // Evaluate per year 2001-2025 (25 seasons)
  const years = {};
  d.forEach((row, i) => {
    const y = +row.t.slice(0, 4);
    if (y < 2001 || r30[i] == null) return;
    const m = +row.t.slice(5, 7);
    years[y] ??= { v1Days: 0, anomalyDaysWetSeason: 0, floodDays: 0, v1Months: new Set(), anomalyMonths: new Set(), floodDates: [] };
    if (r30[i] < 40) { years[y].v1Days++; years[y].v1Months.add(m); }
    // anomaly trigger only counts inside the main rainfed growing window (Nov-Apr)
    const inSeason = m >= 11 || m <= 4;
    if (inSeason && p10[doy(row.t)] != null && r30[i] < p10[doy(row.t)]) { years[y].anomalyDaysWetSeason++; years[y].anomalyMonths.add(m); }
    if (r3[i] != null && r3[i] > flood99) { years[y].floodDays++; if (years[y].floodDates.length < 3) years[y].floodDates.push(row.t); }
  });
  const ys = Object.keys(years).map(Number).sort();
  const v1YearsFired = ys.filter(y => years[y].v1Days > 0).length;
  const anomalyYearsFired = ys.filter(y => years[y].anomalyDaysWetSeason >= 10).length;
  const floodYearsFired = ys.filter(y => years[y].floodDays > 0).length;
  const avgV1Days = ys.reduce((s, y) => s + years[y].v1Days, 0) / ys.length;
  const row = {
    site: site.name, lat: site.lat, lon: site.lon,
    seasons: ys.length,
    v1_trigger_years_fired: v1YearsFired,
    v1_avg_days_per_year_below_40mm_30d: Math.round(avgV1Days),
    anomaly_trigger_years_fired_NovApr: anomalyYearsFired,
    anomaly_fired_years: ys.filter(y => years[y].anomalyDaysWetSeason >= 10),
    flood_3day_threshold_mm: Math.round(flood99),
    flood_trigger_years_fired: floodYearsFired,
    flood_first_dates_2024: years[2024]?.floodDates ?? [],
  };
  summary.push(row);
  const csv = ['year,v1_days_below_40mm_30d,v1_months,anomaly_days_NovApr,flood_days'].concat(
    ys.map(y => `${y},${years[y].v1Days},"${[...years[y].v1Months].join(' ')}",${years[y].anomalyDaysWetSeason},${years[y].floodDays}`)
  ).join('\n');
  writeFileSync(new URL(`./backtest-${site.id}.csv`, import.meta.url), csv);
}
writeFileSync(new URL('./backtest-summary.json', import.meta.url), JSON.stringify({ source: 'Open-Meteo Historical Weather API (ERA5 / ERA5-Land reanalysis)', generated: new Date().toISOString(), baseline: `${BASE_FROM}-${BASE_TO}`, sites: summary }, null, 2));
console.log(JSON.stringify(summary, null, 2));
