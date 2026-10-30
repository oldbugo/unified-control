import test from 'node:test';
import assert from 'node:assert/strict';
import { createUsageReader, guardSnapshot, rasterize, VIEWS } from '../engine.mjs';
import { renderUsageSvg } from '../source/render.js';
const NOW = 1_920_000_000_000;
function reading(provider = 'codex', overrides = {}) {
  return { provider, status: 'ok', observedAt: NOW, provenanceVerified: true, _testIdentity: 'fixture-principal-a',
    fiveHour: { usedPercent: 17, resetsAt: NOW / 1000 + 3600 }, weekly: { usedPercent: 31, resetsAt: NOW / 1000 + 86400 }, ...overrides };
}
function setup(extra = {}) {
  let clock = NOW, quotaCalls = 0, tokenCalls = 0, started = 0, stopped = 0;
  const reader = createUsageReader({ now: () => clock,
    readers: { codex: async () => { quotaCalls++; return reading(); }, claude: async () => { quotaCalls++; return reading('claude'); } },
    tokenReader: async () => { tokenCalls++; return { status: 'ok', observedAt: clock, daily: [{ date: '2030-11-03', tokens: 4567 }], _testIdentity: 'fixture-principal-a' }; },
    identityOf: raw => raw?._testIdentity ?? null,
    brokerFactory: () => ({ start: async () => { started++; }, stop: async () => { stopped++; } }),
    setTimer: () => ({ unref() {} }), clearTimer() {}, ...extra });
  return { reader, advance: n => { clock += n; }, counts: () => ({ quotaCalls, tokenCalls, started, stopped }) };
}
test('provenance, current time, provider and reset bounds hide unsafe numerical data', () => {
  for (const raw of [reading('claude'), reading('codex', { provenanceVerified: false }), reading('codex', { observedAt: NOW + 1001 }),
    reading('codex', { observedAt: NOW - 300001 }), reading('codex', { status: 'unavailable' }),
    reading('codex', { fiveHour: { usedPercent: -1 }, weekly: { usedPercent: 101 } }),
    reading('codex', { fiveHour: { usedPercent: 5, resetsAt: NOW / 1000 }, weekly: null })]) {
    const clean = guardSnapshot('codex', raw, NOW);
    assert.notEqual(clean.status, 'ok'); assert.equal(clean.fiveHour, null); assert.equal(clean.weekly, null); assert.deepEqual(clean.tokenHistory, []);
    const svg = renderUsageSvg(clean, { now: NOW }); assert.ok(!svg.includes('data-gauge='));
  }
  assert.equal(guardSnapshot('codex', reading(), NOW).fiveHour.usedPercent, 17);
});
test('only whitelisted account fields leave provider boundary; quota and tokens remain distinct', () => {
  const clean = guardSnapshot('codex', reading('codex', { accountId: 'secret-account', credentials: 'secret-auth',
    message: 'secret-message', tokenHistoryStatus: 'ok', tokenHistoryObservedAt: NOW,
    tokenHistory: [{ date: '2030-11-03', tokens: 4567, account: 'secret-field' }, { date: '2030-02-30', tokens: 10 }] }), NOW);
  assert.equal(clean.fiveHour.usedPercent, 17); assert.deepEqual(clean.tokenHistory, [{ date: '2030-11-03', tokens: 4567 }]);
  assert.ok(!JSON.stringify(clean).includes('secret')); assert.equal(clean.tokenHistory[0].tokens, 4567);
});
test('constructor and inactive render never invoke providers, broker or token reader', async () => {
  const s = setup(); assert.equal(s.reader.snapshot('codex').status, 'inactive');
  const result = await s.reader.render('codex', 'rings', 196, 196);
  assert.equal(result.snapshot.fiveHour, null); assert.deepEqual(s.counts(), { quotaCalls: 0, tokenCalls: 0, started: 0, stopped: 0 });
  await s.reader.stop();
});
test('one shared quota schedule per provider; token process starts only for configured token face', async () => {
  const s = setup(); await s.reader.setActive([{ provider: 'codex', tokenHistory: false }]); await s.reader.tick();
  assert.equal(s.counts().quotaCalls, 1); assert.equal(s.counts().tokenCalls, 0);
  await Promise.all(Array.from({ length: 14 }, () => s.reader.render('codex', 'rings', 196, 196)));
  assert.equal(s.counts().quotaCalls, 1);
  s.advance(60000); await s.reader.tick(); assert.equal(s.counts().quotaCalls, 2);
  await s.reader.setActive([{ provider: 'codex', tokenHistory: true }]); await s.reader.tick();
  assert.equal(s.counts().tokenCalls, 1); assert.equal(s.reader.snapshot('codex').tokenHistory[0].tokens, 4567);
  await s.reader.tick(); assert.equal(s.counts().tokenCalls, 1);
  await s.reader.setActive([]); s.advance(3600001); await s.reader.tick(); assert.equal(s.counts().tokenCalls, 1);
  await s.reader.stop();
});
test('all sixteen original faces produce fixed physical-size PNGs; stale never becomes zero', () => {
  assert.equal(VIEWS.length, 16);
  for (const view of VIEWS) for (const [width, height] of [[196, 196], [458, 196]]) {
    const result = rasterize(reading(), { provider: 'codex', view: view.id, width, height, now: NOW });
    const png = Buffer.from(result.png); assert.equal(png.subarray(1, 4).toString(), 'PNG');
    assert.equal(png.readUInt32BE(16), width); assert.equal(png.readUInt32BE(20), height);
    const stale = rasterize(reading(), { provider: 'codex', view: view.id, width, height, now: NOW + 300001 });
    assert.equal(stale.snapshot.status, 'stale'); assert.equal(stale.snapshot.fiveHour, null); assert.deepEqual(stale.snapshot.tokenHistory, []);
  }
});
test('Claude file logout barrier suppresses gauges on next shared check; broker lifetime follows activation', async () => {
  let loggedOut = false;
  const s = setup({ readers: { claude: async () => loggedOut ? reading('claude', { status: 'unavailable' }) : reading('claude') } });
  await s.reader.setActive([{ provider: 'claude', tokenHistory: false }]); await s.reader.tick();
  assert.equal(s.reader.snapshot('claude').status, 'ok'); assert.equal(s.counts().started, 1);
  loggedOut = true; s.advance(1000);
  const result = await s.reader.render('claude', 'rings', 196, 196); assert.notEqual(result.snapshot.status, 'ok'); assert.equal(result.snapshot.fiveHour, null);
  await s.reader.setActive([]); assert.equal(s.counts().stopped, 1); await s.reader.stop();
});
test('failed refresh never renews timestamp; stale history cannot act as current', async () => {
  let fail = false;
  const s = setup({ readers: { codex: async () => { if (fail) throw Error('secret exception'); return reading(); } } });
  await s.reader.setActive([{ provider: 'codex', tokenHistory: false }]); await s.reader.tick();
  s.advance(300001); assert.equal(s.reader.snapshot('codex').status, 'stale');
  fail = true; await s.reader.tick(); const result = s.reader.snapshot('codex');
  assert.equal(result.status, 'error'); assert.equal(result.fiveHour, null); assert.ok(!JSON.stringify(result).includes('secret'));
  await s.reader.stop();
});
test('deactivation aborts a pending account read; late success cannot restore gauges', async () => {
  let release, signal;
  const s = setup({ readers: { codex: options => { signal = options.signal; return new Promise(resolve => { release = resolve; }); } } });
  await s.reader.setActive([{ provider: 'codex', tokenHistory: false }]); await new Promise(resolve => setImmediate(resolve));
  await s.reader.setActive([]); assert.equal(signal.aborted, true); release(reading()); await new Promise(resolve => setImmediate(resolve));
  assert.equal(s.reader.snapshot('codex').status, 'inactive'); await s.reader.stop();
});

test('Codex identity changes clear old quota history and tokens; every new quota refreshes tokens even if metadata matches', async () => {
  let identity = 'private-a', at = NOW, quota = 10, total = 111, tokenCalls = 0, history = [{ privateOldAccount: true }];
  const s = setup({ readers: { codex: async () => reading('codex', { observedAt: at, _testIdentity: identity, fiveHour: { usedPercent: quota, resetsAt: NOW / 1000 + 3600 } }) },
    tokenReader: async () => { tokenCalls++; return { status: 'ok', observedAt: at, daily: [{ date: '2030-11-03', tokens: total }], _testIdentity: identity }; },
    historyStore: { record: value => history.push(value), clear() { history = []; }, get: () => history } });
  await s.reader.setActive([{ provider: 'codex', tokenHistory: true }]); await s.reader.tick();
  assert.equal(tokenCalls, 1); assert.equal(s.reader.snapshot('codex').tokenHistory[0].tokens, 111); assert.equal(history.length, 1);
  identity = 'private-b'; quota = 20; total = 222; at += 60000; s.advance(60000); await s.reader.tick();
  assert.equal(tokenCalls, 2); assert.equal(s.reader.snapshot('codex').tokenHistory[0].tokens, 222); assert.equal(history.length, 1);
  assert.equal(history[0].fiveHour.usedPercent, 20); assert.ok(!JSON.stringify(history).includes('private-'));
  // Same exposed principal/workspace metadata can hide an ambiguous workspace switch.
  // The next genuine quota revision still discards the old token cache and reads anew.
  total = 333; at += 60000; s.advance(60000); await s.reader.tick();
  assert.equal(tokenCalls, 3); assert.equal(s.reader.snapshot('codex').tokenHistory[0].tokens, 333); assert.equal(history.length, 2);
  await s.reader.stop();
});
test('late token completion from a previous quota revision or identity cannot replace the current account', async () => {
  for (const changedIdentity of [false, true]) {
    let identity = 'private-a', at = NOW, completions = [], signals = [];
    const s = setup({ readers: { codex: async () => reading('codex', { observedAt: at, _testIdentity: identity }) },
      tokenReader: ({ signal }) => { signals.push(signal); return new Promise(resolve => { completions.push(resolve); }); } });
    await s.reader.setActive([{ provider: 'codex', tokenHistory: true }]); await new Promise(resolve => setImmediate(resolve));
    assert.equal(completions.length, 1);
    if (changedIdentity) identity = 'private-b'; at += 60000; s.advance(60000);
    const second = s.reader.tick(); await new Promise(resolve => setImmediate(resolve));
    assert.equal(signals[0].aborted, true); assert.equal(completions.length, 2); assert.deepEqual(s.reader.snapshot('codex').tokenHistory, []);
    completions[0]({ status: 'ok', observedAt: at, daily: [{ date: '2030-11-03', tokens: 111 }], _testIdentity: 'private-a' });
    await new Promise(resolve => setImmediate(resolve)); assert.deepEqual(s.reader.snapshot('codex').tokenHistory, []);
    completions[1]({ status: 'ok', observedAt: at, daily: [{ date: '2030-11-03', tokens: 222 }], _testIdentity: identity });
    await second; assert.equal(s.reader.snapshot('codex').tokenHistory[0].tokens, 222); await s.reader.stop();
  }
});
test('unverified Codex account identity suppresses quotas and token reads, while continuing config preserves minute throttling', async () => {
  const s = setup({ identityOf: () => null });
  await s.reader.setActive([{ provider: 'codex', tokenHistory: true }]); await s.reader.tick();
  assert.notEqual(s.reader.snapshot('codex').status, 'ok'); assert.equal(s.counts().tokenCalls, 0); await s.reader.stop();
  const stable = setup(); await stable.reader.setActive([{ provider: 'codex', tokenHistory: false }]); await stable.reader.tick();
  await stable.reader.setActive([{ provider: 'codex', tokenHistory: true }]); await stable.reader.tick();
  assert.equal(stable.counts().quotaCalls, 1); assert.equal(stable.counts().tokenCalls, 1); await stable.reader.stop();
});
test('token identity mismatch removes quota/token numbers without spinning another quota read', async () => {
  let quotas = 0;
  const s = setup({ readers: { codex: async () => { quotas++; return reading('codex', { _testIdentity: 'private-a' }); } },
    tokenReader: async () => ({ status: 'ok', observedAt: NOW, daily: [{ date: '2030-11-03', tokens: 111 }], _testIdentity: 'private-b' }) });
  await s.reader.setActive([{ provider: 'codex', tokenHistory: true }]); await s.reader.tick();
  assert.notEqual(s.reader.snapshot('codex').status, 'ok'); assert.deepEqual(s.reader.snapshot('codex').tokenHistory, []);
  s.advance(1000); await s.reader.tick(); assert.equal(quotas, 1); await s.reader.stop();
});
