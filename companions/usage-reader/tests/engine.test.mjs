import test from 'node:test';
import assert from 'node:assert/strict';
import { createUsageReader, guardSnapshot, rasterize, VIEWS } from '../engine.mjs';
import { renderUsageSvg } from '../source/render.js';
const NOW = 1_920_000_000_000;
function reading(provider = 'codex', overrides = {}) {
  return { provider, status: 'ok', observedAt: NOW, provenanceVerified: true,
    fiveHour: { usedPercent: 17, resetsAt: NOW / 1000 + 3600 }, weekly: { usedPercent: 31, resetsAt: NOW / 1000 + 86400 }, ...overrides };
}
function setup(extra = {}) {
  let clock = NOW, quotaCalls = 0, tokenCalls = 0, started = 0, stopped = 0;
  const reader = createUsageReader({ now: () => clock,
    readers: { codex: async () => { quotaCalls++; return reading(); }, claude: async () => { quotaCalls++; return reading('claude'); } },
    tokenReader: async () => { tokenCalls++; return { status: 'ok', observedAt: clock, daily: [{ date: '2030-11-03', tokens: 4567 }] }; },
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
