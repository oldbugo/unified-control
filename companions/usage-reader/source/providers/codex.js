import { spawn } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

const MAX_OUTPUT_BYTES = 1024 * 1024;

function usageWindow(value) {
  if (!value || !Number.isFinite(value.usedPercent) || value.usedPercent < 0 ||
      value.usedPercent > 100 || !Number.isFinite(value.resetsAt) || value.resetsAt <= 0) return null;
  return { usedPercent: value.usedPercent, resetsAt: value.resetsAt };
}

/** Return only quota fields; never retain account IDs, credits, or authentication. */
export function normalizeCodexUsage(result, observedAt = Date.now()) {
  const multiBucket = result?.rateLimitsByLimitId;
  const bucket = multiBucket && typeof multiBucket === 'object'
    ? multiBucket.codex
    : result?.rateLimits;
  const windows = [bucket?.primary, bucket?.secondary];
  const fiveHour = usageWindow(windows.find(value => value?.windowDurationMins === 300));
  const weekly = usageWindow(windows.find(value => value?.windowDurationMins === 10080));
  return {
    provider: 'codex', status: fiveHour || weekly ? 'ok' : 'unavailable', observedAt,
    fiveHour, weekly,
    ...(!fiveHour && !weekly ? { message: 'Codex did not report five-hour or weekly limits. Sign in to Codex with ChatGPT.' } : {})
  };
}

/** Locate the desktop-bundled executable without inspecting sign-in files. */
export function findCodexExecutable() {
  if (process.platform === 'win32' && process.env.LOCALAPPDATA) {
    const bin = path.join(process.env.LOCALAPPDATA, 'OpenAI', 'Codex', 'bin');
    try {
      const candidates = fs.readdirSync(bin, { withFileTypes: true })
        .filter(entry => entry.isDirectory())
        .map(entry => path.join(bin, entry.name, 'codex.exe'))
        .filter(file => fs.existsSync(file))
        .map(file => ({ file, modified: fs.statSync(file).mtimeMs }))
        .sort((left, right) => right.modified - left.modified);
      if (candidates.length) return candidates[0].file;
    } catch { /* CLI installations may instead expose codex on PATH. */ }
  }
  return process.platform === 'win32' ? 'codex.exe' : 'codex';
}

export function normalizeCodexTokenHistory(result, observedAt = Date.now()) {
  const byDate = new Map();
  if (Array.isArray(result?.dailyUsageBuckets)) for (const bucket of result.dailyUsageBuckets) {
    const date = bucket?.startDate;
    if (typeof date !== 'string' || !/^\d{4}-\d{2}-\d{2}$/.test(date) ||
        !Number.isSafeInteger(bucket.tokens) || bucket.tokens < 0) continue;
    const parsed = new Date(`${date}T00:00:00Z`);
    if (!Number.isFinite(parsed.getTime()) || parsed.toISOString().slice(0, 10) !== date) continue;
    byDate.set(date, { date, tokens: bucket.tokens });
  }
  const daily = [...byDate.values()].sort((a, b) => a.date.localeCompare(b.date)).slice(-30);
  return { status: daily.length ? 'ok' : 'unavailable', observedAt, daily };
}

/** A dedicated, read-only server process. No thread/start or turn/start requests. */
async function readAccountData(method, normalize, options) {
  const executable = options.executable || findCodexExecutable();
  const timeoutMs = Number.isFinite(options.timeoutMs)
    ? Math.max(10, Math.min(60000, options.timeoutMs)) : 15000;
  const spawnProcess = options.spawnProcess || spawn;
  return new Promise(resolve => {
    let child;
    let timer;
    let settled = false;
    let buffer = '';
    let bytes = 0;
    let phase = 'initialize';
    const onAbort = () => finish(empty('unavailable', 'The usage reader was stopped.'));
    const empty = (status, message) => method === 'account/usage/read'
      ? { status, observedAt: Date.now(), daily: [], message }
      : { provider: 'codex', status, observedAt: Date.now(), fiveHour: null, weekly: null, message };
    const finish = value => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      options.signal?.removeEventListener('abort', onAbort);
      // Only this reader's dedicated child is terminated; no existing Codex session is touched.
      if (child) {
        child.stdin?.end();
        child.kill();
      }
      resolve(value);
    };
    const send = message => {
      if (settled) return;
      try { child.stdin.write(`${JSON.stringify(message)}\n`); }
      catch { finish(empty('error', 'Could not communicate with the Codex usage reader.')); }
    };
    if (options.signal?.aborted) { finish(empty('unavailable', 'The usage reader was stopped.')); return; }
    options.signal?.addEventListener('abort', onAbort, { once: true });
    try {
      child = spawnProcess(executable, ['app-server'], {
        windowsHide: true, shell: false, stdio: ['pipe', 'pipe', 'ignore']
      });
    } catch {
      finish(empty('unavailable', 'Codex could not be started. Install Codex or choose its executable.'));
      return;
    }
    timer = setTimeout(() => finish(empty('error', 'Codex usage refresh timed out.')), timeoutMs);
    child.on('error', () => finish(empty('unavailable', 'Codex could not be started. Install Codex or choose its executable.')));
    child.stdin.on('error', () => finish(empty('error', 'The Codex usage connection closed.')));
    child.on('close', () => finish(empty('error', 'Codex closed before returning usage.')));
    child.stdout.setEncoding('utf8');
    child.stdout.on('data', chunk => {
      if (settled) return;
      bytes += Buffer.byteLength(chunk);
      if (bytes > MAX_OUTPUT_BYTES) {
        finish(empty('error', 'Codex returned an unexpectedly large usage response.'));
        return;
      }
      buffer += chunk;
      let newline;
      while (!settled && (newline = buffer.indexOf('\n')) !== -1) {
        const line = buffer.slice(0, newline).trim();
        buffer = buffer.slice(newline + 1);
        if (!line) continue;
        let message;
        try { message = JSON.parse(line); }
        catch {
          finish(empty('error', 'Codex returned an invalid usage response.'));
          return;
        }
        if (phase === 'initialize' && message.id === 1) {
          if (message.error || !message.result) {
            finish(empty('error', 'Codex could not initialize the usage reader.'));
            return;
          }
          phase = 'usage';
          send({ method: 'initialized', params: {} });
          send({ id: 2, method });
        } else if (phase === 'usage' && message.id === 2) {
          if (message.error) {
            finish(empty('unavailable', 'Codex usage is unavailable. Check your ChatGPT sign-in in Codex.'));
          } else {
            finish(normalize(message.result));
          }
        }
      }
    });
    send({ id: 1, method: 'initialize', params: {
      clientInfo: { name: 'ulanzi_ai_usage_monitor', title: 'Ulanzi AI Usage Monitor', version: '0.2.0' }
    } });
  });
}

export async function readCodexUsage(options = {}) {
  return readAccountData('account/rateLimits/read', normalizeCodexUsage, options);
}

export async function readCodexTokenHistory(options = {}) {
  return readAccountData('account/usage/read', normalizeCodexTokenHistory, options);
}
