import { readFile, readdir, stat, unlink } from 'node:fs/promises';
import path from 'node:path';

const MAX_AGE = 300_000;
const RECORD = /^\d{13}-[a-z0-9]+\.(json|ready)$/;
const finite = value => typeof value === 'number' && Number.isFinite(value);
function result(status, observedAt, message, fiveHour = null, weekly = null) {
  return { provider: 'claude', status, observedAt, fiveHour, weekly, message };
}
function unavailable(now, detail = '') {
  return result('unavailable', now, `Live Claude usage unavailable; historical cache is not a current account reading. ${detail}`.trim());
}
async function jsonFile(filename) {
  const info = await stat(filename);
  if (!info.isFile() || info.size > 16_384) throw new Error('invalid-file');
  return JSON.parse(await readFile(filename, 'utf8'));
}
function quota(raw, now) {
  if (raw == null) return null;
  if (!finite(raw.usedPercent) || raw.usedPercent < 0 || raw.usedPercent > 100) throw new Error('invalid-percent');
  const reset = raw.resetsAt;
  if (reset != null && (!finite(reset) || reset <= 0 || reset > 1e11)) throw new Error('invalid-reset');
  if (reset != null && reset * 1000 <= now) return null;
  return { usedPercent: raw.usedPercent, resetsAt: reset ?? null };
}
function parse(data, now, maxAge) {
  if (!data || data.version !== 2 || data.provider !== 'claude' || !['claude-session-mod', 'claude-account-poller'].includes(data.source) ||
      data.accountBound !== true || !finite(data.observedAt) || data.observedAt <= 1e12 || data.observedAt > now + 1000) {
    return unavailable(now, 'Bridge provenance is missing or unsupported.');
  }
  const quotaEvent = data.event === 'quota' && data.reason === 'measurement' && data.status === 'ok';
  const authEvent = data.event === 'auth-invalidated' && data.reason === 'logout' && data.status === 'unavailable';
  if (!quotaEvent && !authEvent) return unavailable(now, 'Bridge event or reason is not supported.');
  if (authEvent) return result('unavailable', data.observedAt, 'Live Claude usage unavailable; Claude signed out. Reload the bridge after signing in again.');
  if (now - data.observedAt > maxAge) return { ...result('unavailable', data.observedAt,
    'Claude reading is stale; no new account measurement has arrived.'), reason: 'stale-observation' };
  if (data.status !== 'ok') return result('unavailable', data.observedAt, 'Live Claude usage unavailable; waiting for a new subscription quota observation.');
  const fiveHour = quota(data.fiveHour, now), weekly = quota(data.weekly, now);
  if (!fiveHour && !weekly) return result('unavailable', data.observedAt, 'Live Claude usage unavailable; no current quota windows were reported.');
  return result('ok', data.observedAt, data.source === 'claude-account-poller'
    ? 'Claude account usage refreshed independently of chat responses.'
    : 'Claude session quota observation from the official Mods API.', fiveHour, weekly);
}
async function cleanup(directory, names, now) {
  // Only our narrowly named records: retain at most 200 observation pairs and
  // nothing older than one day. No unrelated files or subdirectories are touched.
  const own = names.filter(name => RECORD.test(name));
  const retained = new Set([...new Set(own.map(name => name.replace(/\.(json|ready)$/, '')))].sort().reverse().slice(0, 200));
  const old = own.filter(name => Number(name.slice(0, 13)) < now - 86_400_000 || !retained.has(name.replace(/\.(json|ready)$/, '')));
  await Promise.all(old.map(name => unlink(path.join(directory, name)).catch(() => {})));
}
async function directories(options, env) {
  if (options.bridgeDirectory) return [options.bridgeDirectory];
  if (!env.LOCALAPPDATA) return [];
  // Separate protocol namespace: old active sessions still emit ambiguous v1
  // invalidations. Never fall back to their directory or old quota readings.
  const suffix = path.join('UlanziToolkit', 'AIUsageMonitor', 'claude-live-v2');
  const found = [path.join(env.LOCALAPPDATA, suffix)];
  // Windows Store Claude's writes are redirected despite logical LOCALAPPDATA.
  const packages = path.join(env.LOCALAPPDATA, 'Packages');
  try {
    for (const entry of await readdir(packages, { withFileTypes: true })) {
      if (entry.isDirectory() && /^Claude_[a-z0-9]+$/i.test(entry.name)) {
        found.push(path.join(packages, entry.name, 'LocalCache', 'Local', suffix));
      }
    }
  } catch (error) { if (error.code !== 'ENOENT') throw error; }
  return found;
}
async function logoutAfter(options, env, minimum, now) {
  for (const directory of await directories(options, env)) {
    let names;
    try { names = await readdir(directory); } catch (error) { if (error.code === 'ENOENT') continue; throw error; }
    await cleanup(directory, names, now);
    for (const name of names.filter(value => RECORD.test(value) && value.endsWith('.ready') && Number(value.slice(0, 13)) >= minimum)) {
      let data;
      try { data = await jsonFile(path.join(directory, name.replace(/\.ready$/, '.json'))); } catch { continue; }
      if (data?.version === 2 && data.provider === 'claude' && data.accountBound === true &&
          ['claude-session-mod', 'claude-account-poller'].includes(data.source) && data.event === 'auth-invalidated' &&
          data.reason === 'logout' && data.status === 'unavailable' && data.observedAt === Number(name.slice(0, 13)) &&
          data.observedAt <= now + 1000) return data.observedAt;
    }
  }
  return null;
}
async function accountReading(options, env, now, maxAge) {
  if (!env.LOCALAPPDATA || options.bridgeDirectory || options.cachePath) return null;
  const base = path.join(env.LOCALAPPDATA, 'UlanziToolkit', 'AIUsageMonitor');
  let health;
  try { health = await jsonFile(path.join(base, 'claude-poll-status.json')); }
  catch (error) { if (error.code === 'ENOENT') return null; throw error; }
  // Once configured, the independent reader is authoritative. Session events
  // must not conceal its failure by stamping an older snapshot as current.
  if (health?.mode !== 'account-poll' || health.version !== 1) return unavailable(now, 'Account reader status is unsupported.');
  if (health.reason === 'logout' || health.status === 'signed-out') return unavailable(now, 'Claude signed out; waiting for an authenticated account reader.');
  if ([401, 403].includes(health.httpStatus)) return unavailable(now, 'The Claude account reader could not authenticate.');
  let data;
  try { data = await jsonFile(path.join(base, 'claude-account-usage.json')); }
  catch (error) { if (error.code === 'ENOENT') return unavailable(now, 'Waiting for the independent account reader to connect.'); throw error; }
  if (data?.source !== 'claude-account-poller') return unavailable(now, 'Account observation provenance is unsupported.');
  if (finite(data.observedAt) && await logoutAfter(options, env, data.observedAt, now) !== null) {
    return unavailable(now, 'Claude signed out after this account observation.');
  }
  const answer = parse(data, now, maxAge);
  if (answer.status === 'ok' && health.status !== 'ok') answer.message = 'Last verified account reading; the next refresh is retrying.';
  return answer;
}
/** Reads sanitized companion observations only; never Desktop history/auth/transcripts.
 * .ready markers publish immutable JSON after its write completes.
 */
export async function readClaudeUsage(options = {}) {
  const now = typeof options.now === 'function' ? options.now() : options.now ?? Date.now();
  const maxAge = Math.min(options.maxAgeMs ?? MAX_AGE, MAX_AGE);
  const env = options.env ?? process.env;
  try {
    if (options.desktopHistoryPath) return unavailable(now, 'Desktop history is not a live source.');
    if (options.cachePath) return parse(await jsonFile(options.cachePath), now, maxAge);
    const account = await accountReading(options, env, now, maxAge);
    if (account) return account;
    const snapshots = [], locations = [];
    for (const directory of await directories(options, env)) {
      let names;
      try { names = await readdir(directory); } catch (error) { if (error.code === 'ENOENT') continue; throw error; }
      locations.push({ directory, names });
      for (const name of names) if (RECORD.test(name) && name.endsWith('.ready')) snapshots.push({ directory, name });
    }
    const ready = snapshots.sort((a, b) => b.name.localeCompare(a.name));
    if (!ready.length) {
      for (const item of locations) await cleanup(item.directory, item.names, now);
      return unavailable(now, 'The companion bridge has no observation yet.');
    }
    // Latest invalidation/malformed record must never fall back to an old success.
    const latestTime = Number(ready[0].name.slice(0, 13));
    const simultaneous = ready.filter(item => Number(item.name.slice(0, 13)) === latestTime);
    let answer = null;
    for (const { directory, name } of simultaneous) {
      const data = await jsonFile(path.join(directory, name.replace(/\.ready$/, '.json')));
      if (data?.observedAt !== latestTime) return unavailable(now, 'Bridge record timestamp does not match its publication.');
      const candidate = parse(data, now, maxAge);
      if (candidate.status !== 'ok') { answer = candidate; break; }
      answer ??= candidate;
    }
    for (const item of locations) await cleanup(item.directory, item.names, now);
    return answer;
  } catch (error) {
    if (error.code === 'ENOENT') return unavailable(now, 'The companion bridge is not connected.');
    return result('error', now, 'Live Claude usage unavailable; bridge data is unreadable or invalid.');
  }
}
