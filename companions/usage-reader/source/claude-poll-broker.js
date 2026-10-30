import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { randomBytes } from 'node:crypto';

export const CLAUDE_BROKER_PORT = 19374;
export const CLAUDE_POLL_INTERVAL_MS = 60_000;
export const CLAUDE_POLL_LEASE_MS = 45_000;
const MAX_BODY = 16_384;
const MAX_BACKOFF = 15 * 60_000;
const REASONS = new Set(['fresh-http-response', 'authorization-unavailable', 'authorization-rejected',
  'auth-rejected', 'rate-limited', 'http-error', 'invalid-response', 'quota-unavailable', 'timeout',
  'host-request-refused', 'cancelled', 'network-error', 'request-failed', 'waiting', 'logout',
  'lease-expired', 'publication-failed', 'storage-error']);
const finite = value => typeof value === 'number' && Number.isFinite(value);
const time = value => Number.isSafeInteger(value) && value >= 0;
const object = value => value !== null && typeof value === 'object' && !Array.isArray(value);
const ownKeys = (value, keys) => object(value) && Object.keys(value).every(key => keys.includes(key));

function atomicJson(filename, value) {
  fs.mkdirSync(path.dirname(filename), { recursive: true });
  const temporary = `${filename}.${randomBytes(6).toString('hex')}.tmp`;
  try {
    fs.writeFileSync(temporary, JSON.stringify(value), { encoding: 'utf8', mode: 0o600 });
    fs.renameSync(temporary, filename);
  } finally {
    try { fs.unlinkSync(temporary); } catch { /* No unfinished temporary file. */ }
  }
}

function windowValue(value, observedAt) {
  if (value === null || value === undefined) return null;
  if (!ownKeys(value, ['usedPercent', 'resetsAt']) || !finite(value.usedPercent) ||
      value.usedPercent < 0 || value.usedPercent > 100) throw new Error('invalid-window');
  if (value.resetsAt !== null && value.resetsAt !== undefined &&
      (!finite(value.resetsAt) || value.resetsAt <= observedAt / 1000 || value.resetsAt > 1e11)) throw new Error('invalid-reset');
  return { usedPercent: value.usedPercent, resetsAt: value.resetsAt ?? null };
}

/** Sole process-wide coordinator. It never receives an authorization handle. */
export function createClaudePollBroker({ directory = path.join(process.env.LOCALAPPDATA ||
  path.join(os.homedir(), 'AppData', 'Local'), 'UlanziToolkit', 'AIUsageMonitor'),
  port = CLAUDE_BROKER_PORT, now = Date.now } = {}) {
  const statePath = path.join(directory, 'claude-poll-state.json');
  const diagnosticPath = path.join(directory, 'claude-poll-status.json');
  const cachePath = path.join(directory, 'claude-account-usage.json');
  let state = { version: 1, nextAllowedAt: 0, failureCount: 0, lastAttemptAt: null,
    lastSuccessAt: null, generation: 0, status: 'waiting', reason: 'waiting', httpStatus: null };
  // State is loaded only after winning the listening socket, never by a second instance.
  let lease = null;
  let listening = false;
  let stopped = false;
  let server;
  const sockets = new Set();

  function loadState() {
    try {
      if (fs.statSync(statePath).size > MAX_BODY) throw new Error('invalid-state');
      const raw = JSON.parse(fs.readFileSync(statePath, 'utf8'));
      if (raw.version !== 1 || !time(raw.nextAllowedAt) || !Number.isInteger(raw.failureCount) ||
          raw.failureCount < 0 || raw.failureCount > 100 || !time(raw.generation)) throw new Error('invalid-state');
      state = { ...state, nextAllowedAt: raw.nextAllowedAt, failureCount: raw.failureCount,
        generation: raw.generation, lastAttemptAt: time(raw.lastAttemptAt) ? raw.lastAttemptAt : null,
        lastSuccessAt: time(raw.lastSuccessAt) ? raw.lastSuccessAt : null,
        status: ['waiting', 'ok', 'unavailable', 'error', 'signed-out'].includes(raw.status) ? raw.status : 'waiting',
        reason: REASONS.has(raw.reason) ? raw.reason : 'waiting',
        httpStatus: Number.isInteger(raw.httpStatus) && raw.httpStatus >= 100 && raw.httpStatus <= 599 ? raw.httpStatus : null };
    } catch (error) {
      // Unknown/corrupt state cannot erase a previous process's request cooldown.
      if (error.code !== 'ENOENT') state.nextAllowedAt = now() + CLAUDE_POLL_INTERVAL_MS;
    }
  }

  function saveState() { atomicJson(statePath, state); }
  function diagnostic() {
    atomicJson(diagnosticPath, { version: 1, mode: 'account-poll', status: state.status,
      reason: state.reason, httpStatus: state.httpStatus, lastAttemptAt: state.lastAttemptAt,
      lastSuccessAt: state.lastSuccessAt, retryAt: state.nextAllowedAt });
  }
  function remaining(clock = now()) {
    return Math.max(0, state.nextAllowedAt - clock, lease ? lease.expiresAt - clock : 0);
  }
  function writeObservation(data) {
    // The canonical cache is authoritative; its name cannot expose a lease ID.
    atomicJson(cachePath, data);
    // Keep the existing immutable protocol for history/older readers as well.
    const filename = path.join(directory, 'claude-live-v2', `${data.observedAt}-${data.status === 'ok' ? 'a' : 'z'}${randomBytes(8).toString('hex')}`);
    try {
      atomicJson(`${filename}.json`, data);
      fs.writeFileSync(`${filename}.ready`, '', { mode: 0o600 });
    } catch { /* A completed canonical write is sufficient; do not undo it. */ }
  }
  function normalizeResult(raw, clock) {
    if (!ownKeys(raw, ['status', 'reason', 'httpStatus', 'observedAt', 'fiveHour', 'weekly', 'retryAfterMs']) ||
        !['ok', 'error', 'unavailable'].includes(raw.status) ||
        (raw.httpStatus != null && (!Number.isInteger(raw.httpStatus) || raw.httpStatus < 100 || raw.httpStatus > 599)) ||
        (raw.retryAfterMs != null && (!finite(raw.retryAfterMs) || raw.retryAfterMs < 0 || raw.retryAfterMs > Number.MAX_SAFE_INTEGER - clock))) {
      throw new Error('invalid-result');
    }
    const normalized = { status: raw.status, reason: REASONS.has(raw.reason) ? raw.reason : 'request-failed',
      httpStatus: raw.httpStatus ?? null, retryAfterMs: Math.ceil(raw.retryAfterMs || 0) };
    if (raw.status === 'ok') {
      if (raw.httpStatus !== 200 || !time(raw.observedAt) || raw.observedAt < lease.startedAt || raw.observedAt > clock) throw new Error('invalid-observation');
      normalized.observedAt = raw.observedAt;
      normalized.fiveHour = windowValue(raw.fiveHour, raw.observedAt);
      normalized.weekly = windowValue(raw.weekly, raw.observedAt);
      if (!normalized.fiveHour && !normalized.weekly) throw new Error('empty-observation');
      normalized.reason = 'fresh-http-response';
    }
    return normalized;
  }

  // All mutations and durable writes below are synchronous within one Node event
  // loop dispatch. There is no await between lease checking and changing state.
  function dispatch(route, body) {
    const clock = now();
    if (route === '/claim') {
      if (!ownKeys(body, [])) return [400, { granted: false, retryAfterMs: CLAUDE_POLL_INTERVAL_MS }];
      if (lease && lease.expiresAt <= clock) lease = null;
      const retryAfterMs = remaining(clock);
      if (retryAfterMs > 0) return [200, { granted: false, retryAfterMs }];
      state.lastAttemptAt = clock;
      state.nextAllowedAt = clock + CLAUDE_POLL_INTERVAL_MS;
      try { saveState(); diagnostic(); }
      catch { return [503, { granted: false, retryAfterMs: CLAUDE_POLL_INTERVAL_MS }]; }
      lease = { id: randomBytes(24).toString('hex'), startedAt: clock,
        expiresAt: clock + CLAUDE_POLL_LEASE_MS, generation: state.generation };
      return [200, { granted: true, leaseId: lease.id, retryAfterMs: 0 }];
    }
    if (route === '/complete') {
      if (!ownKeys(body, ['leaseId', 'result'])) return [400, { accepted: false, retryAfterMs: Math.max(1000, remaining(clock)) }];
      if (!lease || body.leaseId !== lease.id || lease.generation !== state.generation || lease.expiresAt <= clock) {
        return [200, { accepted: false, retryAfterMs: Math.max(1000, remaining(clock)) }];
      }
      let result;
      try { result = normalizeResult(body.result, clock); }
      catch { return [400, { accepted: false, retryAfterMs: Math.max(1000, remaining(clock)) }]; }
      lease = null;
      let publicationFailed = false;
      if (result.status === 'ok') {
        try {
          writeObservation({ version: 2, provider: 'claude', source: 'claude-account-poller', accountBound: true,
            event: 'quota', reason: 'measurement', observedAt: result.observedAt, status: 'ok',
            fiveHour: result.fiveHour, weekly: result.weekly });
          state.lastSuccessAt = result.observedAt;
          state.failureCount = 0;
          state.status = 'ok'; state.reason = 'fresh-http-response'; state.httpStatus = 200;
        } catch {
          publicationFailed = true;
          result.status = 'error'; result.reason = 'publication-failed'; result.httpStatus = null;
        }
      }
      if (result.status !== 'ok') {
        state.failureCount = Math.min(100, state.failureCount + 1);
        const backoff = Math.min(MAX_BACKOFF, CLAUDE_POLL_INTERVAL_MS * 2 ** Math.min(10, state.failureCount - 1));
        state.nextAllowedAt = Math.max(state.nextAllowedAt, clock + backoff, clock + result.retryAfterMs);
        state.status = result.status;
        state.reason = result.httpStatus === 401 || result.httpStatus === 403 ? 'auth-rejected' : result.reason;
        state.httpStatus = result.httpStatus;
      }
      try { saveState(); diagnostic(); }
      catch { return [503, { accepted: false, retryAfterMs: Math.max(CLAUDE_POLL_INTERVAL_MS, remaining(clock)) }]; }
      return [200, { accepted: !publicationFailed, retryAfterMs: remaining(clock) }];
    }
    if (route === '/invalidate') {
      if (!ownKeys(body, ['reason']) || body.reason !== 'logout') return [400, { accepted: false, retryAfterMs: CLAUDE_POLL_INTERVAL_MS }];
      state.generation += 1;
      lease = null;
      state.nextAllowedAt = Math.max(state.nextAllowedAt, clock + CLAUDE_POLL_INTERVAL_MS);
      state.status = 'signed-out'; state.reason = 'logout'; state.httpStatus = null;
      let publicationFailed = false;
      try {
        writeObservation({ version: 2, provider: 'claude', source: 'claude-account-poller', accountBound: true,
          event: 'auth-invalidated', reason: 'logout', observedAt: clock, status: 'unavailable', fiveHour: null, weekly: null });
      } catch { publicationFailed = true; }
      // Persist the logout barrier even if its cache file cannot be replaced.
      try { saveState(); diagnostic(); }
      catch { return [503, { accepted: false, retryAfterMs: CLAUDE_POLL_INTERVAL_MS }]; }
      if (publicationFailed) return [503, { accepted: false, retryAfterMs: CLAUDE_POLL_INTERVAL_MS }];
      return [200, { accepted: true, retryAfterMs: remaining(clock) }];
    }
    return [404, { error: 'unknown-route' }];
  }

  function handle(request, response) {
    let done = false;
    const send = (status, value) => {
      if (done) return;
      done = true;
      response.writeHead(status, { 'content-type': 'application/json', 'cache-control': 'no-store', 'x-content-type-options': 'nosniff' });
      response.end(JSON.stringify(value));
    };
    const remote = request.socket.remoteAddress;
    if ((remote !== '127.0.0.1' && remote !== '::ffff:127.0.0.1') || request.headers.origin !== undefined) {
      send(403, { error: 'request-refused' }); request.resume(); return;
    }
    if (request.method !== 'POST' || !['/claim', '/complete', '/invalidate'].includes(request.url)) {
      send(404, { error: 'unknown-route' }); request.resume(); return;
    }
    if (!/^application\/json(?:\s*;.*)?$/i.test(request.headers['content-type'] || '')) {
      send(415, { error: 'json-required' }); request.resume(); return;
    }
    if (Number(request.headers['content-length']) > MAX_BODY) {
      send(413, { error: 'request-too-large' }); request.resume(); return;
    }
    let chunks = [], length = 0;
    request.on('data', chunk => {
      if (done) return;
      length += chunk.length;
      if (length > MAX_BODY) { chunks = []; send(413, { error: 'request-too-large' }); return; }
      chunks.push(chunk);
    });
    request.on('end', () => {
      if (done) return;
      let body;
      try { body = JSON.parse(Buffer.concat(chunks).toString('utf8')); }
      catch { send(400, { error: 'invalid-json' }); return; }
      try { const [status, result] = dispatch(request.url, body); send(status, result); }
      catch { send(503, { error: 'broker-unavailable' }); }
    });
    request.on('error', () => send(400, { error: 'request-failed' }));
  }

  return {
    async start() {
      if (listening) return { listening: true, port: server.address().port };
      if (stopped) return { listening: false, reason: 'stopped' };
      server = http.createServer({ requestTimeout: 5000, headersTimeout: 5000 }, handle);
      server.on('connection', socket => { sockets.add(socket); socket.on('close', () => sockets.delete(socket)); });
      server.on('clientError', (_error, socket) => { socket.end('HTTP/1.1 400 Bad Request\r\nConnection: close\r\n\r\n'); });
      return new Promise(resolve => {
        server.once('error', error => resolve({ listening: false,
          reason: error.code === 'EADDRINUSE' ? 'address-in-use' : 'listen-failed' }));
        server.listen(port, '127.0.0.1', () => {
          listening = true;
          loadState();
          try { diagnostic(); }
          catch { /* Claims fail closed if durable coordination cannot be saved. */ }
          resolve({ listening: true, port: server.address().port });
        });
      });
    },
    async stop() {
      stopped = true;
      lease = null;
      if (!listening) return;
      listening = false;
      for (const socket of sockets) socket.destroy();
      await new Promise(resolve => server.close(resolve));
    }
  };
}
