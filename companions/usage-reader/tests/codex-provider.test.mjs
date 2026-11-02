import test from 'node:test';
import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import { PassThrough, Writable } from 'node:stream';
import { normalizeCodexUsage, readCodexUsage, normalizeCodexTokenHistory, readCodexTokenHistory, codexIdentity } from '../source/providers/codex.js';

const window = (minutes, usedPercent = 23.5) => ({ windowDurationMins: minutes, usedPercent, resetsAt: 1900000000 });

test('matches durations rather than primary/secondary positions and keeps fractional usage', () => {
  const result = normalizeCodexUsage({ rateLimits: { primary: window(10080), secondary: window(300, 0) } }, 123);
  assert.deepEqual(result, { provider: 'codex', status: 'ok', observedAt: 123,
    fiveHour: { usedPercent: 0, resetsAt: 1900000000 }, weekly: { usedPercent: 23.5, resetsAt: 1900000000 } });
});

test('prefers the codex bucket and does not substitute unrelated buckets', () => {
  const result = normalizeCodexUsage({ rateLimitsByLimitId: { other: { primary: window(300) } }, rateLimits: { primary: window(300) } });
  assert.equal(result.fiveHour, null);
  assert.equal(result.status, 'unavailable');
  const selected = normalizeCodexUsage({ rateLimitsByLimitId: { codex: { primary: window(300, 45) } }, rateLimits: { primary: window(300) } });
  assert.equal(selected.fiveHour.usedPercent, 45);
  assert.equal(selected.weekly, null);
});

test('missing and invalid measurements are unknown, not zero', () => {
  for (const invalid of [null, { windowDurationMins: 300, usedPercent: null, resetsAt: 1900000000 }, window(300, NaN), window(300, -1), window(300, 101), { ...window(300), resetsAt: null }]) {
    assert.equal(normalizeCodexUsage({ rateLimits: { primary: invalid } }).fiveHour, null);
  }
});

const account = (email = 'synthetic@example.invalid', workspaceName = null) => ({ account: { type: 'chatgpt', email, planType: 'pro', workspaceName }, requiresOpenaiAuth: true });
function fakeServer(respond, accounts = [account(), account()]) {
  const child = new EventEmitter();
  child.stdout = new PassThrough();
  child.requests = [];
  child.killCount = 0;
  child.kill = () => { child.killCount++; return true; };
  child.stdin = new Writable({ write(chunk, encoding, callback) {
    const message = JSON.parse(String(chunk));
    child.requests.push(message);
    queueMicrotask(() => {
      if (message.method === 'account/read') child.reply({ id: message.id, result: accounts[message.id === 3 ? 0 : 1] });
      else respond(message, child);
    });
    callback();
  } });
  child.reply = message => child.stdout.write(`${JSON.stringify(message)}\n`);
  return child;
}

test('performs handshake, reads only account limits, hides the process and cleans it up', async () => {
  const child = fakeServer((message, process) => {
    if (message.id === 1) process.reply({ id: 1, result: { userAgent: 'test' } });
    if (message.id === 2) process.reply({ id: 2, result: { rateLimits: { primary: window(300) } } });
  });
  let launch;
  const result = await readCodexUsage({ executable: 'fake-codex', spawnProcess: (...args) => { launch = args; return child; } });
  assert.deepEqual(child.requests.map(message => message.method), ['initialize', 'initialized', 'account/read', 'account/rateLimits/read', 'account/read']);
  assert.ok(child.requests.filter(m => m.method === 'account/read').every(m => m.params.refreshToken === false));
  assert.equal(launch[0], 'fake-codex');
  assert.deepEqual(launch[1], ['app-server']);
  assert.equal(launch[2].windowsHide, true);
  assert.equal(launch[2].shell, false);
  assert.equal(result.status, 'ok');
  assert.equal(child.killCount, 1);
});

test('times out and kills its dedicated child', async () => {
  const child = fakeServer(() => {});
  const result = await readCodexUsage({ spawnProcess: () => child, timeoutMs: 15 });
  assert.equal(result.status, 'error');
  assert.match(result.message, /timed out/);
  assert.equal(child.killCount, 1);
});

test('deactivation abort kills only its dedicated account child promptly', async () => {
  const child = fakeServer(() => {}), controller = new AbortController();
  const pending = readCodexUsage({ spawnProcess: () => child, timeoutMs: 60000, signal: controller.signal });
  controller.abort(); const result = await pending;
  assert.equal(result.status, 'unavailable'); assert.equal(child.killCount, 1);
  let spawned = false;
  await readCodexUsage({ spawnProcess: () => { spawned = true; return child; }, signal: controller.signal });
  assert.equal(spawned, false);
});

test('server errors never expose raw messages or account details', async () => {
  const child = fakeServer((message, process) => {
    if (message.id === 1) process.reply({ id: 1, result: {} });
    if (message.id === 2) process.reply({ id: 2, error: { message: 'secret-account-and-token' } });
  });
  const result = await readCodexUsage({ spawnProcess: () => child });
  assert.equal(result.status, 'unavailable');
  assert.doesNotMatch(JSON.stringify(result), /secret-account-and-token/);
});

test('spawn failure returns a useful sanitized unavailable state', async () => {
  const result = await readCodexUsage({ spawnProcess: () => { throw new Error('private-path'); } });
  assert.equal(result.status, 'unavailable');
  assert.doesNotMatch(JSON.stringify(result), /private-path/);
});

test('accepts fragmented JSON lines and ignores unrelated notifications', async () => {
  const child = fakeServer((message, process) => {
    if (message.id === 1) {
      process.stdout.write('{"id":1,"res');
      process.stdout.write('ult":{}}\n');
    }
    if (message.id === 2) {
      process.reply({ method: 'thread/name/updated', params: { privateData: 'not retained' } });
      process.reply({ id: 2, result: { rateLimits: { primary: window(300) } } });
    }
  });
  const result = await readCodexUsage({ spawnProcess: () => child });
  assert.equal(result.status, 'ok');
  assert.doesNotMatch(JSON.stringify(result), /not retained/);
});

test('invalid protocol output fails safely and terminates the reader', async () => {
  const child = fakeServer((_message, process) => process.stdout.write('invalid private output\n'));
  const result = await readCodexUsage({ spawnProcess: () => child });
  assert.equal(result.status, 'error');
  assert.doesNotMatch(JSON.stringify(result), /private output/);
  assert.equal(child.killCount, 1);
});

test('daily token history preserves true zero and rejects missing or invalid totals and dates', () => {
  const result = normalizeCodexTokenHistory({ summary: { accountId: 'secret' }, dailyUsageBuckets: [
    { startDate: '2026-10-22', tokens: 0 }, { startDate: '2026-10-24', tokens: 1234 },
    { startDate: '2026-10-23', tokens: null }, { startDate: '2026-10-21', tokens: -1 },
    { startDate: '2026-02-30', tokens: 10 }, { startDate: 'private', tokens: 10 }
  ] }, 123);
  assert.deepEqual(result, { status: 'ok', observedAt: 123, daily: [
    { date: '2026-10-22', tokens: 0 }, { date: '2026-10-24', tokens: 1234 }
  ] });
  assert.doesNotMatch(JSON.stringify(result), /secret/);
  assert.equal(normalizeCodexTokenHistory({ dailyUsageBuckets: null }).status, 'unavailable');
});

test('daily history sorts, deduplicates and bounds the real reported buckets without filling missing days', () => {
  const buckets = Array.from({ length: 50 }, (_, day) => ({
    startDate: new Date(Date.UTC(2026, 8, 1 + day)).toISOString().slice(0, 10), tokens: day
  })).reverse();
  const result = normalizeCodexTokenHistory({ dailyUsageBuckets: [...buckets, buckets[0]] });
  assert.equal(result.daily.length, 30);
  assert.ok(result.daily[0].date < result.daily.at(-1).date);
  assert.deepEqual(result.daily.at(-1), { date: buckets[0].startDate, tokens: buckets[0].tokens });
});

test('daily token reader uses its own read-only account endpoint and returns sanitized buckets', async () => {
  const child = fakeServer((message, process) => {
    if (message.id === 1) process.reply({ id: 1, result: {} });
    if (message.id === 2) process.reply({ id: 2, result: { dailyUsageBuckets: [{ startDate: '2026-10-24', tokens: 100 }] } });
  });
  const result = await readCodexTokenHistory({ spawnProcess: () => child });
  assert.deepEqual(child.requests.map(message => message.method), ['initialize', 'initialized', 'account/read', 'account/usage/read', 'account/read']);
  assert.equal(result.status, 'ok');
  assert.deepEqual(result.daily, [{ date: '2026-10-24', tokens: 100 }]);
  assert.equal(child.killCount, 1);
});

const quotaResponder = (message, child) => {
  if (message.id === 1) child.reply({ id: 1, result: {} });
  if (message.id === 2) child.reply({ id: 2, result: { rateLimits: { primary: window(300) } } });
};
test('quota reads fail closed if account metadata is missing or changes before/after the transaction', async () => {
  for (const identities of [[account('one@example.invalid'), account('two@example.invalid')],
    [account(null), account(null)], [{ account: null }, account()],
    [{ account: { type: 'apiKey' } }, { account: { type: 'apiKey' } }],
    [account('one@example.invalid', 'One'), account('one@example.invalid', 'Two')]]) {
    const child = fakeServer(quotaResponder, identities);
    const result = await readCodexUsage({ spawnProcess: () => child });
    assert.notEqual(result.status, 'ok'); assert.equal(result.fiveHour, null); assert.equal(codexIdentity(result), null);
    assert.ok(!JSON.stringify(result).includes('@example.invalid'));
  }
});
test('identity is stable only in RAM side-channel and never an enumerable result or private pipe field', async () => {
  const read = async email => readCodexUsage({ spawnProcess: () => fakeServer(quotaResponder, [account(email), account(email)]) });
  const a = await read('one@example.invalid'), a2 = await read('one@example.invalid'), b = await read('two@example.invalid');
  assert.ok(codexIdentity(a)); assert.equal(codexIdentity(a), codexIdentity(a2)); assert.notEqual(codexIdentity(a), codexIdentity(b));
  assert.ok(!JSON.stringify(a).includes(codexIdentity(a))); assert.ok(!JSON.stringify(a).includes('@example.invalid'));
  assert.ok(!Object.keys(a).some(k => /identity|account|email/i.test(k)));
});
test('account-updated notification during otherwise stable metadata brackets invalidates usage', async () => {
  const child = fakeServer((message, child) => {
    if (message.id === 2) child.reply({ method: 'account/updated', params: { authMode: 'chatgpt', planType: 'pro' } });
    quotaResponder(message, child);
  });
  const result = await readCodexUsage({ spawnProcess: () => child }); assert.notEqual(result.status, 'ok'); assert.equal(result.fiveHour, null);
});
test('token transaction also rejects an account change in its final metadata check', async () => {
  const child = fakeServer((message, child) => {
    if (message.id === 1) child.reply({ id: 1, result: {} });
    if (message.id === 2) child.reply({ id: 2, result: { dailyUsageBuckets: [{ startDate: '2026-10-31', tokens: 999 }] } });
  }, [account('one@example.invalid'), account('two@example.invalid')]);
  const result = await readCodexTokenHistory({ spawnProcess: () => child }); assert.deepEqual(result.daily, []); assert.equal(codexIdentity(result), null);
});
