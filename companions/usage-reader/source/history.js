import fs from 'node:fs';
import path from 'node:path';

export const HISTORY_RETENTION_MS = 7 * 24 * 60 * 60_000;
export const HISTORY_MAX_SAMPLES = 10_080;
const FRESH_AFTER_MS = 5 * 60_000;
const PROVIDERS = ['codex', 'claude'];

function cleanWindow(value, observedAt) {
  if (!value || !Number.isFinite(value.usedPercent) || value.usedPercent < 0 || value.usedPercent > 100) return null;
  const reset = value.resetsAt;
  if (reset !== null && reset !== undefined && (!Number.isFinite(reset) || reset * 1000 <= observedAt)) return null;
  return { usedPercent: value.usedPercent, resetsAt: reset ?? null };
}

function cleanSample(value) {
  if (!Number.isFinite(value?.observedAt) || value.observedAt <= 0) return null;
  const fiveHour = cleanWindow(value.fiveHour, value.observedAt);
  const weekly = cleanWindow(value.weekly, value.observedAt);
  return fiveHour || weekly ? { observedAt: value.observedAt, fiveHour, weekly } : null;
}

/** Stores quota percentages only. No account IDs, transcripts, or raw responses. */
export function createHistoryStore({ filePath, now = Date.now, maxSamples = HISTORY_MAX_SAMPLES,
  retentionMs = HISTORY_RETENTION_MS } = {}) {
  const limit = Number.isInteger(maxSamples) ? Math.max(1, Math.min(HISTORY_MAX_SAMPLES, maxSamples)) : HISTORY_MAX_SAMPLES;
  const retention = Number.isFinite(retentionMs) ? Math.max(1, Math.min(HISTORY_RETENTION_MS, retentionMs)) : HISTORY_RETENTION_MS;
  let providers = { codex: [], claude: [] };
  let persistenceFailed = false;
  let loaded = false;

  function prune(samples) {
    const clock = now();
    const byTime = new Map();
    for (const value of samples) {
      const sample = cleanSample(value);
      if (sample && sample.observedAt >= clock - retention && sample.observedAt <= clock) {
        byTime.set(sample.observedAt, sample);
      }
    }
    return [...byTime.values()].sort((a, b) => a.observedAt - b.observedAt).slice(-limit);
  }

  if (filePath) {
    try {
      if (fs.statSync(filePath).size <= 8 * 1024 * 1024) {
        const data = JSON.parse(fs.readFileSync(filePath, 'utf8'));
        if (data.version === 1) {
          loaded = true;
          for (const provider of PROVIDERS) {
            if (Array.isArray(data.providers?.[provider])) providers[provider] = prune(data.providers[provider]);
          }
        }
      }
    } catch { /* Missing or damaged history never blocks live readings. */ }
  }

  function persist() {
    if (!filePath) return;
    const temporary = `${filePath}.${process.pid}.tmp`;
    try {
      fs.mkdirSync(path.dirname(filePath), { recursive: true });
      fs.writeFileSync(temporary, JSON.stringify({ version: 1, providers }), { encoding: 'utf8', mode: 0o600 });
      fs.renameSync(temporary, filePath);
      persistenceFailed = false;
    } catch {
      persistenceFailed = true;
      try { fs.unlinkSync(temporary); } catch { /* No leftover temporary file to remove. */ }
    }
  }

  if (loaded) persist(); // Enforce retention on disk as well as in memory after restart.

  return {
    record(snapshot) {
      const provider = snapshot?.provider;
      if (!PROVIDERS.includes(provider) || snapshot.status !== 'ok') return false;
      const sample = cleanSample(snapshot);
      const age = sample ? now() - sample.observedAt : NaN;
      if (!sample || age < 0 || age > FRESH_AFTER_MS) return false;
      providers[provider] = prune(providers[provider]);
      if (providers[provider].at(-1)?.observedAt >= sample.observedAt) return false;
      providers[provider] = prune([...providers[provider], sample]);
      for (const other of PROVIDERS) providers[other] = prune(providers[other]);
      persist();
      return true;
    },
    get(provider) {
      if (!PROVIDERS.includes(provider)) return [];
      const previousLength = providers[provider].length;
      providers[provider] = prune(providers[provider]);
      if (previousLength !== providers[provider].length) persist();
      return providers[provider].map(sample => ({ ...sample,
        fiveHour: sample.fiveHour ? { ...sample.fiveHour } : null,
        weekly: sample.weekly ? { ...sample.weekly } : null }));
    },
    clear(provider) {
      if (provider !== undefined && !PROVIDERS.includes(provider)) return;
      for (const name of provider ? [provider] : PROVIDERS) providers[name] = [];
      persist();
    },
    get persistenceFailed() { return persistenceFailed; }
  };
}
