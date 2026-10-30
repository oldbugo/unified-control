import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, writeFile, rm, readdir } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { readClaudeUsage } from '../source/providers/claude.js';
const now = 1792800000000;
const record = overrides => ({ version: 2, event: 'quota', reason: 'measurement', provider: 'claude', source: 'claude-session-mod', accountBound: true,
  observedAt: now - 1000, status: 'ok', fiveHour: { usedPercent: 20, resetsAt: now / 1000 + 4000 },
  weekly: { usedPercent: 40, resetsAt: now / 1000 + 86400 }, ...overrides });
async function fixture(t, data) {
  const dir = await mkdtemp(path.join(os.tmpdir(), 'claude-provider-'));
  t.after(() => rm(dir, { recursive: true, force: true }));
  const filename = path.join(dir, 'quota.json');
  await writeFile(filename, typeof data === 'string' ? data : JSON.stringify(data));
  return filename;
}
function unknown(data) { assert.notEqual(data.status, 'ok'); assert.equal(data.fiveHour, null); assert.equal(data.weekly, null); }
test('yesterday history cannot become live gauge; recent unverified history also refused', async t => {
  for (const time of [now - 86400000, now - 1000]) {
    const filename = await fixture(t, { version: 2, samples: [{ t: time, org: 'test', u: { fh: 2, sd: 20 } }] });
    unknown(await readClaudeUsage({ desktopHistoryPath: filename, now }));
    unknown(await readClaudeUsage({ cachePath: filename, now }));
  }
});
test('default reader ignores recent historical Desktop source', async t => {
  const dir = path.dirname(await fixture(t, {}));
  const profile = path.join(dir, 'Claude-Data', 'profile');
  await mkdir(profile, { recursive: true });
  await writeFile(path.join(profile, 'plan-usage-history.json'), JSON.stringify({ version: 2, samples: [{ t: now, org: 'test', u: { fh: 33 } }] }));
  const data = await readClaudeUsage({ env: { LOCALAPPDATA: dir }, now }); unknown(data);
  assert.match(data.message, /historical cache is not a current account reading/);
});
test('fresh companion preserves percentages, reset seconds and true observation time', async t => {
  const cachePath = await fixture(t, record()); const data = await readClaudeUsage({ cachePath, now });
  assert.equal(data.status, 'ok'); assert.deepEqual(data.fiveHour, record().fiveHour);
  assert.deepEqual(data.weekly, record().weekly); assert.equal(data.observedAt, now - 1000);
});

test('idle observation retains its real age and is explicitly stale rather than disconnected', async t => {
  const cachePath = await fixture(t, record({ observedAt: now - 480000 }));
  const data = await readClaudeUsage({ cachePath, now });
  unknown(data);
  assert.equal(data.reason, 'stale-observation');
  assert.equal(data.observedAt, now - 480000);
});

async function accountFixture(t, health, data) {
  const dir = path.dirname(await fixture(t, {}));
  const base = path.join(dir, 'UlanziToolkit', 'AIUsageMonitor');
  await mkdir(base, { recursive: true });
  await writeFile(path.join(base, 'claude-poll-status.json'), JSON.stringify({ version: 1, mode: 'account-poll', ...health }));
  if (data) await writeFile(path.join(base, 'claude-account-usage.json'), JSON.stringify(data));
  return { dir, base, options: { env: { LOCALAPPDATA: dir }, now } };
}

test('configured independent reader cannot be masked by fresh session events', async t => {
  const f = await accountFixture(t, { status: 'waiting', httpStatus: null });
  const bridge = path.join(f.base, 'claude-live-v2'); await mkdir(bridge);
  await writeFile(path.join(bridge, `${now - 1000}-a.json`), JSON.stringify(record()));
  await writeFile(path.join(bridge, `${now - 1000}-a.ready`), '');
  const data = await readClaudeUsage(f.options);
  unknown(data); assert.match(data.message, /independent account reader/);
});

test('temporary polling failure preserves only a genuinely fresh original account observation', async t => {
  const sample = record({ source: 'claude-account-poller' });
  const f = await accountFixture(t, { status: 'error', httpStatus: 503 }, sample);
  const data = await readClaudeUsage(f.options);
  assert.equal(data.status, 'ok'); assert.equal(data.observedAt, sample.observedAt);
  assert.match(data.message, /retrying/);
  const old = await readClaudeUsage({ ...f.options, now: now + 300000 });
  unknown(old); assert.equal(old.reason, 'stale-observation');
});

test('account authentication rejection hides even a fresh account cache', async t => {
  const f = await accountFixture(t, { status: 'unavailable', httpStatus: 401 }, record({ source: 'claude-account-poller' }));
  const data = await readClaudeUsage(f.options);
  unknown(data); assert.match(data.message, /authenticate/);
});

test('logout while the broker is offline blocks a previously successful account poll', async t => {
  const f = await accountFixture(t, { status: 'ok', httpStatus: 200 }, record({ source: 'claude-account-poller' }));
  const bridge = path.join(f.base, 'claude-live-v2'); await mkdir(bridge);
  await writeFile(path.join(bridge, `${now}-z.json`), JSON.stringify(record({ observedAt: now, event: 'auth-invalidated', reason: 'logout', status: 'unavailable', fiveHour: null, weekly: null })));
  await writeFile(path.join(bridge, `${now}-z.ready`), '');
  const data = await readClaudeUsage(f.options);
  unknown(data); assert.match(data.message, /signed out/);
});
test('provenance, account binding, timestamp and five-minute freshness required', async t => {
  for (const payload of [record({ source: 'history' }), record({ accountBound: false }), record({ observedAt: undefined }),
    record({ observedAt: now + 2000 }), record({ observedAt: now - 300001 }), record({ provider: 'codex' }),
    record({ status: 'unavailable' }), null]) {
    const cachePath = await fixture(t, payload); unknown(await readClaudeUsage({ cachePath, now, maxAgeMs: 86400000 }));
  }
});
test('invalid/null percentages unknown; true zero preserved; missing weekly unknown', async t => {
  for (const usedPercent of [null, '30', 101, -1]) {
    const cachePath = await fixture(t, record({ fiveHour: { usedPercent, resetsAt: now / 1000 + 60 } }));
    unknown(await readClaudeUsage({ cachePath, now }));
  }
  const cachePath = await fixture(t, record({ fiveHour: { usedPercent: 0, resetsAt: null }, weekly: null }));
  const data = await readClaudeUsage({ cachePath, now }); assert.equal(data.fiveHour.usedPercent, 0); assert.equal(data.weekly, null);
});
test('passed resets invalidate independently without invented zero', async t => {
  const cachePath = await fixture(t, record({ fiveHour: { usedPercent: 88, resetsAt: now / 1000 } }));
  const data = await readClaudeUsage({ cachePath, now }); assert.equal(data.fiveHour, null); assert.deepEqual(data.weekly, record().weekly);
});
test('ready protocol ignores partial unpublished files; latest invalidation never falls back', async t => {
  const dir = path.dirname(await fixture(t, {}));
  await writeFile(path.join(dir, `${now - 2000}-a.json`), JSON.stringify(record({ observedAt: now - 2000 })));
  await writeFile(path.join(dir, `${now - 2000}-a.ready`), '');
  await writeFile(path.join(dir, `${now - 1000}-b.json`), '{partial');
  assert.equal((await readClaudeUsage({ bridgeDirectory: dir, now })).status, 'ok');
  await writeFile(path.join(dir, `${now}-c.json`), JSON.stringify(record({ observedAt: now, status: 'unavailable' })));
  await writeFile(path.join(dir, `${now}-c.ready`), '');
  unknown(await readClaudeUsage({ bridgeDirectory: dir, now }));
});
test('default locates bridge output; cleanup only removes own old records', async t => {
  const dir = path.dirname(await fixture(t, {}));
  const bridge = path.join(dir, 'UlanziToolkit', 'AIUsageMonitor', 'claude-live-v2'); await mkdir(bridge, { recursive: true });
  for (const suffix of ['json', 'ready']) await writeFile(path.join(bridge, `${now - 172800000}-old.${suffix}`), '{}');
  await writeFile(path.join(bridge, 'unrelated.txt'), 'keep');
  await writeFile(path.join(bridge, `${now}-a.json`), JSON.stringify(record({ observedAt: now }))); await writeFile(path.join(bridge, `${now}-a.ready`), '');
  const data = await readClaudeUsage({ env: { LOCALAPPDATA: dir }, now }); assert.equal(data.status, 'ok'); assert.equal(data.observedAt, now);
  const names = await readdir(bridge); assert.ok(names.includes('unrelated.txt')); assert.ok(!names.some(name => name.includes('-old.')));
});
test('any same-millisecond invalidation suppresses success regardless of random filename order', async t => {
  const dir = path.dirname(await fixture(t, {}));
  for (const [nonce, payload] of [['z', record({ observedAt: now })], ['a', record({ observedAt: now, status: 'unavailable' })]]) {
    await writeFile(path.join(dir, `${now}-${nonce}.json`), JSON.stringify(payload));
    await writeFile(path.join(dir, `${now}-${nonce}.ready`), '');
  }
  unknown(await readClaudeUsage({ bridgeDirectory: dir, now }));
});
test('publication timestamp mismatch and malformed same-time record suppress live gauge', async t => {
  const dir = path.dirname(await fixture(t, {}));
  await writeFile(path.join(dir, `${now}-z.json`), JSON.stringify(record()));
  await writeFile(path.join(dir, `${now}-z.ready`), '');
  unknown(await readClaudeUsage({ bridgeDirectory: dir, now }));
  await writeFile(path.join(dir, `${now}-z.json`), JSON.stringify(record({ observedAt: now })));
  await writeFile(path.join(dir, `${now}-a.json`), '{partial');
  await writeFile(path.join(dir, `${now}-a.ready`), '');
  unknown(await readClaudeUsage({ bridgeDirectory: dir, now }));
});
test('malformed/missing bridge exposes no paths or raw content', async t => {
  const cachePath = await fixture(t, '{private-invalid-content'); const data = await readClaudeUsage({ cachePath, now }); unknown(data);
  assert.ok(!JSON.stringify(data).includes(cachePath)); assert.ok(!JSON.stringify(data).includes('private-invalid-content'));
  unknown(await readClaudeUsage({ env: {}, now }));
});
test('discovers virtualized Desktop output, compares locations, honors newest invalidation', async t => {
  const dir = path.dirname(await fixture(t, {}));
  const suffix = path.join('UlanziToolkit', 'AIUsageMonitor', 'claude-live-v2');
  const regular = path.join(dir, suffix);
  const packaged = path.join(dir, 'Packages', 'Claude_testpackage', 'LocalCache', 'Local', suffix);
  for (const [folder, time] of [[regular, now - 86400000], [packaged, now]]) {
    await mkdir(folder, { recursive: true });
    await writeFile(path.join(folder, `${time}-a.json`), JSON.stringify(record({ observedAt: time })));
    await writeFile(path.join(folder, `${time}-a.ready`), '');
  }
  let data = await readClaudeUsage({ env: { LOCALAPPDATA: dir }, now });
  assert.equal(data.status, 'ok'); assert.equal(data.observedAt, now);
  assert.ok(!JSON.stringify(data).includes('testpackage'));
  await writeFile(path.join(regular, `${now + 1}-z.json`), JSON.stringify(record({ observedAt: now + 1, status: 'unavailable' })));
  await writeFile(path.join(regular, `${now + 1}-z.ready`), '');
  unknown(await readClaudeUsage({ env: { LOCALAPPDATA: dir }, now: now + 1 }));
});
test('v2 event/reason allowlist rejects legacy and malformed event claims', async t => {
  for (const payload of [record({ version: 1 }), record({ event: undefined }), record({ reason: 'unknown' }),
    record({ event: 'auth-invalidated', reason: 'other', status: 'unavailable' })]) {
    const cachePath = await fixture(t, payload); unknown(await readClaudeUsage({ cachePath, now }));
  }
});
test('newer legacy directory cannot interfere; absence of v2 never falls back to legacy', async t => {
  const dir = path.dirname(await fixture(t, {}));
  const base = path.join(dir, 'UlanziToolkit', 'AIUsageMonitor');
  const old = path.join(base, 'claude-live'), live = path.join(base, 'claude-live-v2');
  await mkdir(old, { recursive: true });
  await writeFile(path.join(old, `${now}-z.json`), JSON.stringify(record({ version: 1, observedAt: now, status: 'unavailable' })));
  await writeFile(path.join(old, `${now}-z.ready`), '');
  unknown(await readClaudeUsage({ env: { LOCALAPPDATA: dir }, now }));
  await mkdir(live, { recursive: true });
  await writeFile(path.join(live, `${now - 1000}-a.json`), JSON.stringify(record()));
  await writeFile(path.join(live, `${now - 1000}-a.ready`), '');
  assert.equal((await readClaudeUsage({ env: { LOCALAPPDATA: dir }, now })).status, 'ok');
  await writeFile(path.join(live, `${now + 1}-z.json`), JSON.stringify(record({ event: 'auth-invalidated', reason: 'logout',
    status: 'unavailable', observedAt: now + 1, fiveHour: null, weekly: null })));
  await writeFile(path.join(live, `${now + 1}-z.ready`), '');
  const signedOut = await readClaudeUsage({ env: { LOCALAPPDATA: dir }, now: now + 1 });
  unknown(signedOut); assert.match(signedOut.message, /signed out/);
});
