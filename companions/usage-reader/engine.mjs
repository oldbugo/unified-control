import { Resvg } from '@resvg/resvg-js';
import fs from 'node:fs';
import path from 'node:path';
import { readClaudeUsage } from './source/providers/claude.js';
import { readCodexUsage, readCodexTokenHistory } from './source/providers/codex.js';
import { createClaudePollBroker } from './source/claude-poll-broker.js';
import { createHistoryStore } from './source/history.js';
import { renderUsageSvg, usageView, VIEW_OPTIONS } from './source/render.js';

export const PROVIDERS = Object.freeze(['claude', 'codex']);
export const VIEWS = VIEW_OPTIONS;
const FRESH_MS = 300_000;
const finite = n => typeof n === 'number' && Number.isFinite(n);
const fresh = (t, now, age = FRESH_MS) => Number.isSafeInteger(t) && t > 0 && t <= now + 1000 && now - t <= age;
// Read only the standard Windows UI fonts, rather than rescan the full system
// font collection for every key. No font is copied out of Windows.
const fontFiles = process.platform === 'win32' ? ['segoeui.ttf', 'segoeuib.ttf']
  .map(name => path.join(process.env.WINDIR || 'C:/Windows', 'Fonts', name)).filter(name => fs.existsSync(name)) : [];
const unknown = (provider, status = 'unavailable', observedAt = null) => ({
  provider, status, observedAt, provenanceVerified: false, fiveHour: null, weekly: null,
  tokenHistoryStatus: 'unavailable', tokenHistoryObservedAt: null, tokenHistory: [],
  message: status === 'stale' ? 'No successful account reading within five minutes.' :
    status === 'inactive' ? 'This provider is not active.' : 'Current account usage is unavailable.'
});
function windowValue(raw, now) {
  if (!raw || !finite(raw.usedPercent) || raw.usedPercent < 0 || raw.usedPercent > 100) return null;
  if (raw.resetsAt != null && (!finite(raw.resetsAt) || raw.resetsAt * 1000 <= now || raw.resetsAt > 1e11)) return null;
  return { usedPercent: raw.usedPercent, resetsAt: raw.resetsAt == null ? null : Math.floor(raw.resetsAt) };
}
// Both the provider publication boundary and every render recheck provenance/time.
// No command accepts a snapshot supplied by the UI or restores a quota cache.
export function guardSnapshot(provider, raw, now = Date.now()) {
  if (!PROVIDERS.includes(provider)) throw new Error('invalid-provider');
  const at = Number.isSafeInteger(raw?.observedAt) && raw.observedAt > 0 && raw.observedAt <= now + 1000 ? raw.observedAt : null;
  if (raw?.provider !== provider || raw.provenanceVerified !== true || raw.status !== 'ok') {
    return unknown(provider, raw?.reason === 'stale-observation' || raw?.status === 'stale' ? 'stale' : raw?.status === 'error' ? 'error' : 'unavailable', at);
  }
  if (!fresh(at, now)) return unknown(provider, at && now - at > FRESH_MS ? 'stale' : 'unavailable', at);
  const fiveHour = windowValue(raw.fiveHour, now), weekly = windowValue(raw.weekly, now);
  if (!fiveHour && !weekly) return unknown(provider, 'unavailable', at);
  const daily = provider === 'codex' && raw.tokenHistoryStatus === 'ok' && fresh(raw.tokenHistoryObservedAt, now, 2 * 3600_000)
    ? (Array.isArray(raw.tokenHistory) ? raw.tokenHistory : []).filter(d => {
      if (!/^\d{4}-\d{2}-\d{2}$/.test(d?.date ?? '') || !Number.isSafeInteger(d.tokens) || d.tokens < 0) return false;
      const parsed = Date.parse(`${d.date}T00:00:00Z`);
      return finite(parsed) && new Date(parsed).toISOString().slice(0, 10) === d.date;
    }).slice(-30).map(({ date, tokens }) => ({ date, tokens })) : [];
  return { provider, status: 'ok', observedAt: at, provenanceVerified: true, fiveHour, weekly,
    tokenHistoryStatus: daily.length ? 'ok' : 'unavailable', tokenHistoryObservedAt: daily.length ? raw.tokenHistoryObservedAt : null,
    tokenHistory: daily, message: 'Verified account observation.' };
}
export function rasterize(snapshot, { provider, view, width, height, history = [], now = Date.now() }) {
  if (!VIEWS.some(v => v.id === view && (!v.provider || v.provider === provider))) throw new Error('invalid-view');
  if (![[196, 196], [458, 196]].some(([w, h]) => w === width && h === height)) throw new Error('invalid-size');
  const guarded = guardSnapshot(provider, snapshot, now);
  // Toolkit's public renderer receives only the sanitized, freshly gated fields.
  const renderSnapshot = { ...guarded, status: guarded.status === 'stale' ? 'unavailable' : guarded.status,
    ...(guarded.status === 'stale' ? { reason: 'stale-observation' } : {}) };
  const svg = renderUsageSvg(renderSnapshot, { provider, view, now, history, format: width === 458 ? 'wide' : 'square' })
    .replace(/width="\d+" height="144"/, `width="${width}" height="${height}"`);
  const png = new Resvg(svg, { font: { loadSystemFonts: process.platform !== 'win32', fontFiles, defaultFontFamily: 'Segoe UI' } }).render().asPng();
  return { png: Array.from(png), snapshot: guarded };
}

const verifiedRead = reader => async options => ({ ...await reader(options), provenanceVerified: true });
// Dependencies can be replaced only in module tests, never by a pipe command.
export function createUsageReader({ now = Date.now,
  readers = { claude: verifiedRead(readClaudeUsage), codex: verifiedRead(readCodexUsage) },
  tokenReader = readCodexTokenHistory, brokerFactory = createClaudePollBroker,
  historyStore = createHistoryStore({ now }), setTimer = setInterval, clearTimer = clearInterval,
  raster = rasterize } = {}) {
  const active = new Map(), snapshots = new Map(), pending = new Map(), attempts = new Map(), controllers = new Map();
  let generation = 0, stopped = false, timer = null, broker = null, token = null, tokenAttempted = -Infinity, tokenPending = null, tokenController = null;
  const empty = provider => unknown(provider, active.has(provider) ? 'unavailable' : 'inactive');
  function snapshot(provider) {
    if (!active.has(provider)) return empty(provider);
    return guardSnapshot(provider, { ...(snapshots.get(provider) || empty(provider)),
      ...(provider === 'codex' && token ? { tokenHistory: token.daily, tokenHistoryObservedAt: token.observedAt, tokenHistoryStatus: token.status } : {}) }, now());
  }
  async function refresh(provider, forceLocalCheck = false) {
    if (stopped || !active.has(provider)) return;
    if (pending.has(provider)) return pending.get(provider);
    const interval = provider === 'claude' && forceLocalCheck ? 1000 : 60_000;
    if (attempts.has(provider) && now() - attempts.get(provider) < interval) return;
    attempts.set(provider, now());
    const gen = generation, controller = new AbortController(); controllers.set(provider, controller);
    const task = Promise.resolve().then(() => readers[provider]({ signal: controller.signal, timeoutMs: 12_000 })).then(raw => {
      if (stopped || generation !== gen || !active.has(provider)) return;
      const clean = guardSnapshot(provider, raw, now()); snapshots.set(provider, clean);
      if (clean.status === 'ok') historyStore.record(clean);
      else { historyStore.clear(provider); if (provider === 'codex') token = null; }
    }, () => { if (generation === gen && active.has(provider)) { snapshots.set(provider, unknown(provider, 'error')); historyStore.clear(provider); if (provider === 'codex') token = null; } })
      .finally(() => { if (pending.get(provider) === task) { pending.delete(provider); controllers.delete(provider); } });
    pending.set(provider, task); return task;
  }
  async function refreshTokens() {
    if (tokenPending) return tokenPending;
    if (stopped || !active.get('codex')?.tokenHistory || now() - tokenAttempted < 3600_000) return;
    if (snapshot('codex').status !== 'ok') return;
    tokenAttempted = now(); const gen = generation;
    const controller = new AbortController(); tokenController = controller;
    const signal = controller.signal;
    const task = Promise.resolve().then(() => tokenReader({ timeoutMs: 12_000, signal })).then(raw => {
      if (stopped || gen !== generation || !active.get('codex')?.tokenHistory || snapshot('codex').status !== 'ok') return;
      token = { status: raw?.status === 'ok' ? 'ok' : 'unavailable', observedAt: raw?.observedAt, daily: raw?.daily };
    }, () => { if (gen === generation) token = null; }).finally(() => { if (tokenPending === task) { tokenPending = null; tokenController = null; } });
    tokenPending = task; return task;
  }
  async function tick() {
    if (stopped) return;
    await Promise.all([...active.keys()].map(p => refresh(p, p === 'claude')));
    await refreshTokens();
  }
  async function setActive(configs) {
    if (stopped || !Array.isArray(configs) || configs.length > 2 || configs.some(c => !PROVIDERS.includes(c?.provider) ||
      typeof c.tokenHistory !== 'boolean' || c.provider === 'claude' && c.tokenHistory) || new Set(configs.map(c => c.provider)).size !== configs.length) throw new Error('invalid-active');
    const same = JSON.stringify([...active.values()]) === JSON.stringify(configs);
    if (same) return { active: [...active.values()] };
    generation++; for (const controller of controllers.values()) controller.abort(); controllers.clear(); pending.clear();
    tokenController?.abort(); tokenController = null; tokenPending = null;
    active.clear(); snapshots.clear(); attempts.clear(); token = null; tokenAttempted = -Infinity;
    for (const config of configs) active.set(config.provider, { provider: config.provider, tokenHistory: config.tokenHistory });
    if (timer) { clearTimer(timer); timer = null; }
    if (active.has('claude') && !broker) { broker = brokerFactory(); await broker.start(); }
    if (!active.has('claude') && broker) { await broker.stop(); broker = null; }
    if (active.size) { timer = setTimer(() => { void tick(); }, 1000); timer?.unref?.(); void tick(); }
    return { active: [...active.values()] };
  }
  return {
    setActive, snapshot, tick,
    async render(provider, view, width, height) {
      if (!PROVIDERS.includes(provider)) throw new Error('invalid-provider');
      // Local bridge files include the logout barrier; check before every new render
      // with one shared, at-most-once-per-second file read across all Claude keys.
      if (provider === 'claude' && active.has(provider)) await refresh(provider, true);
      const current = snapshot(provider);
      return raster(current, { provider, view, width, height, now: now(), history: current.status === 'ok' ? historyStore.get(provider) : [] });
    },
    async stop() {
      stopped = true; generation++; if (timer) clearTimer(timer); timer = null;
      for (const controller of controllers.values()) controller.abort(); controllers.clear();
      tokenController?.abort(); tokenController = null;
      active.clear(); snapshots.clear(); token = null;
      if (broker) { await broker.stop(); broker = null; }
    }
  };
}
