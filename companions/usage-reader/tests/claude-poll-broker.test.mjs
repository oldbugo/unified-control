import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import http from 'node:http';
import { createClaudePollBroker } from '../source/claude-poll-broker.js';

const START = Date.UTC(2026, 9, 24, 3);
function request(port, route, body = {}, headers = {}) {
  return new Promise((resolve, reject) => {
    const data = typeof body === 'string' ? body : JSON.stringify(body);
    const req = http.request({ host: '127.0.0.1', port, path: route, method: 'POST',
      headers: { 'content-type': 'application/json', 'content-length': Buffer.byteLength(data), ...headers } }, response => {
      let text = '';
      response.setEncoding('utf8');
      response.on('data', chunk => { text += chunk; });
      response.on('end', () => resolve({ status: response.statusCode, body: JSON.parse(text), headers: response.headers }));
    });
    req.on('error', reject);
    req.end(data);
  });
}
async function setup(t) {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'ulanzi-poll-broker-'));
  let clock = START;
  const broker = createClaudePollBroker({ directory, port: 0, now: () => clock });
  const started = await broker.start();
  assert.equal(started.listening, true);
  t.after(async () => { await broker.stop(); fs.rmSync(directory, { recursive: true, force: true }); });
  return { directory, broker, port: started.port, now: () => clock, advance: ms => { clock += ms; },
    read: filename => JSON.parse(fs.readFileSync(path.join(directory, filename), 'utf8')) };
}
const quota = observedAt => ({ status: 'ok', reason: 'fresh-http-response', httpStatus: 200, observedAt,
  fiveHour: { usedPercent: 23.5, resetsAt: observedAt / 1000 + 3600 },
  weekly: { usedPercent: 42, resetsAt: observedAt / 1000 + 86400 }, retryAfterMs: 60000 });
const failure = (httpStatus = 500, retryAfterMs = 0) => ({ status: 'error', reason: httpStatus === 429 ? 'rate-limited' : 'http-error',
  httpStatus, observedAt: null, fiveHour: null, weekly: null, retryAfterMs });

test('concurrent Desktop sessions receive only one lease and starts stay at least sixty seconds apart', async t => {
  const s = await setup(t);
  const responses = await Promise.all(Array.from({ length: 12 }, () => request(s.port, '/claim')));
  const granted = responses.filter(response => response.body.granted);
  assert.equal(granted.length, 1);
  assert.match(granted[0].body.leaseId, /^[a-f0-9]{48}$/);
  assert.ok(responses.filter(response => !response.body.granted).every(response => response.body.retryAfterMs === 60000));
  s.advance(45001);
  assert.equal((await request(s.port, '/claim')).body.granted, false);
  s.advance(14999);
  assert.equal((await request(s.port, '/claim')).body.granted, true);
});

test('wrong or expired leases cannot publish or complete another session request', async t => {
  const s = await setup(t);
  const { leaseId } = (await request(s.port, '/claim')).body;
  assert.equal((await request(s.port, '/complete', { leaseId: 'wrong', result: quota(START) })).body.accepted, false);
  s.advance(45000);
  assert.equal((await request(s.port, '/complete', { leaseId, result: quota(START) })).body.accepted, false);
  assert.equal(fs.existsSync(path.join(s.directory, 'claude-account-usage.json')), false);
});

test('a genuine successful result publishes canonical and ready records with its request-start timestamp', async t => {
  const s = await setup(t);
  const { leaseId } = (await request(s.port, '/claim')).body;
  s.advance(2000);
  const result = await request(s.port, '/complete', { leaseId, result: quota(START) });
  assert.equal(result.body.accepted, true);
  const cache = s.read('claude-account-usage.json');
  assert.deepEqual(cache, { version: 2, provider: 'claude', source: 'claude-account-poller', accountBound: true,
    event: 'quota', reason: 'measurement', observedAt: START, status: 'ok',
    fiveHour: quota(START).fiveHour, weekly: quota(START).weekly });
  const names = fs.readdirSync(path.join(s.directory, 'claude-live-v2'));
  assert.equal(names.filter(name => name.endsWith('.ready')).length, 1);
  assert.equal(names.some(name => name.endsWith('.tmp')), false);
  assert.ok(names.every(name => name.startsWith(String(START))));
  const diagnostic = s.read('claude-poll-status.json');
  assert.equal(diagnostic.mode, 'account-poll');
  assert.equal(diagnostic.lastSuccessAt, START);
  assert.equal(diagnostic.httpStatus, 200);
  assert.equal(diagnostic.status, 'ok');
  assert.doesNotMatch(JSON.stringify(diagnostic), /usedPercent|leaseId|23\.5/);
  assert.doesNotMatch(fs.readFileSync(path.join(s.directory, 'claude-poll-state.json'), 'utf8'), new RegExp(leaseId));
});

test('logout invalidates even without a lease and blocks an in-flight pre-logout result', async t => {
  const s = await setup(t);
  const { leaseId } = (await request(s.port, '/claim')).body;
  s.advance(500);
  assert.equal((await request(s.port, '/invalidate', { reason: 'logout' })).body.accepted, true);
  const signedOut = s.read('claude-account-usage.json');
  assert.equal(signedOut.event, 'auth-invalidated');
  assert.equal(signedOut.reason, 'logout');
  assert.equal(signedOut.observedAt, START + 500);
  assert.equal((await request(s.port, '/complete', { leaseId, result: quota(START) })).body.accepted, false);
  assert.deepEqual(s.read('claude-account-usage.json'), signedOut);
  assert.equal(s.read('claude-poll-status.json').status, 'signed-out');
  s.advance(1000);
  assert.equal((await request(s.port, '/invalidate', { reason: 'logout' })).body.accepted, true);
  assert.equal(s.read('claude-account-usage.json').observedAt, START + 1500);
});

test('rate limit cooldown is shared, honors Retry-After beyond the backoff cap, and survives restart', async t => {
  const s = await setup(t);
  const { leaseId } = (await request(s.port, '/claim')).body;
  const completed = await request(s.port, '/complete', { leaseId, result: failure(429, 3600000) });
  assert.equal(completed.body.retryAfterMs, 3600000);
  assert.equal(s.read('claude-poll-status.json').httpStatus, 429);
  assert.equal((await request(s.port, '/claim')).body.granted, false);
  await s.broker.stop();
  const replacement = createClaudePollBroker({ directory: s.directory, port: 0, now: s.now });
  const restarted = await replacement.start();
  t.after(() => replacement.stop());
  s.advance(60000);
  const follower = await request(restarted.port, '/claim');
  assert.equal(follower.body.granted, false);
  assert.equal(follower.body.retryAfterMs, 3540000);
  assert.equal(s.read('claude-poll-status.json').httpStatus, 429);
});

test('transient failures preserve the old cache timestamp and exponential failure backoff', async t => {
  const s = await setup(t);
  let claim = (await request(s.port, '/claim')).body;
  await request(s.port, '/complete', { leaseId: claim.leaseId, result: quota(START) });
  const original = s.read('claude-account-usage.json');
  s.advance(60000);
  claim = (await request(s.port, '/claim')).body;
  assert.equal((await request(s.port, '/complete', { leaseId: claim.leaseId, result: failure() })).body.retryAfterMs, 60000);
  assert.deepEqual(s.read('claude-account-usage.json'), original);
  s.advance(60000);
  claim = (await request(s.port, '/claim')).body;
  assert.equal((await request(s.port, '/complete', { leaseId: claim.leaseId, result: failure() })).body.retryAfterMs, 120000);
  assert.equal(s.read('claude-poll-status.json').lastSuccessAt, START);
  assert.equal(fs.readdirSync(path.join(s.directory, 'claude-live-v2')).filter(name => name.endsWith('.ready')).length, 1);
});

test('HTTP authorization rejection is visible without pretending Claude globally logged out', async t => {
  const s = await setup(t);
  const { leaseId } = (await request(s.port, '/claim')).body;
  await request(s.port, '/complete', { leaseId, result: failure(401) });
  const diagnostic = s.read('claude-poll-status.json');
  assert.equal(diagnostic.reason, 'auth-rejected');
  assert.equal(diagnostic.httpStatus, 401);
  assert.notEqual(diagnostic.status, 'signed-out');
  assert.equal(fs.existsSync(path.join(s.directory, 'claude-account-usage.json')), false);
});

test('malformed or restamped success data and credential-shaped extra fields cannot publish', async t => {
  const s = await setup(t);
  const { leaseId } = (await request(s.port, '/claim')).body;
  for (const result of [
    { ...quota(START), observedAt: START - 1 }, { ...quota(START), observedAt: START + 1 },
    { ...quota(START), httpStatus: 500 }, { ...quota(START), fiveHour: { usedPercent: 101, resetsAt: null } },
    { ...quota(START), auth: 'must-not-be-stored' }
  ]) assert.equal((await request(s.port, '/complete', { leaseId, result })).body.accepted, false);
  assert.equal(fs.existsSync(path.join(s.directory, 'claude-account-usage.json')), false);
  assert.doesNotMatch(fs.readFileSync(path.join(s.directory, 'claude-poll-status.json'), 'utf8'), /must-not-be-stored/);
});

test('foreign Origin, wrong content type, unknown routes and oversized payloads are refused', async t => {
  const s = await setup(t);
  assert.equal((await request(s.port, '/claim', {}, { origin: 'https://foreign.example' })).status, 403);
  assert.equal((await request(s.port, '/claim', {}, { origin: 'null' })).status, 403);
  assert.equal((await request(s.port, '/claim', {}, { 'content-type': 'text/plain' })).status, 415);
  assert.equal((await request(s.port, '/unknown')).status, 404);
  assert.equal((await request(s.port, '/claim', 'x'.repeat(17000))).status, 413);
  const response = await request(s.port, '/claim');
  assert.equal(response.body.granted, true);
  assert.equal(response.headers['access-control-allow-origin'], undefined);
});

test('a second service cannot bind or overwrite the existing broker health file', async t => {
  const s = await setup(t);
  await request(s.port, '/claim');
  const original = fs.readFileSync(path.join(s.directory, 'claude-poll-status.json'), 'utf8');
  const second = createClaudePollBroker({ directory: s.directory, port: s.port, now: () => START + 5000 });
  assert.deepEqual(await second.start(), { listening: false, reason: 'address-in-use' });
  await second.stop();
  assert.equal(fs.readFileSync(path.join(s.directory, 'claude-poll-status.json'), 'utf8'), original);
});

test('failed durable claim coordination never grants an unpersisted lease', async t => {
  const s = await setup(t);
  fs.mkdirSync(path.join(s.directory, 'claude-poll-state.json'));
  const response = await request(s.port, '/claim');
  assert.equal(response.status, 503);
  assert.equal(response.body.granted, false);
  assert.equal(response.body.leaseId, undefined);
});

test('cache publication failure is not reported as an accepted successful update', async t => {
  const s = await setup(t);
  const { leaseId } = (await request(s.port, '/claim')).body;
  fs.mkdirSync(path.join(s.directory, 'claude-account-usage.json'));
  const response = await request(s.port, '/complete', { leaseId, result: quota(START) });
  assert.equal(response.body.accepted, false);
  assert.equal(s.read('claude-poll-status.json').reason, 'publication-failed');
  assert.equal(s.read('claude-poll-status.json').lastSuccessAt, null);
  assert.equal(fs.existsSync(path.join(s.directory, 'claude-live-v2')), false);
  const invalidation = await request(s.port, '/invalidate', { reason: 'logout' });
  assert.equal(invalidation.body.accepted, false);
  assert.equal(s.read('claude-poll-status.json').status, 'signed-out', 'The logout barrier survives a cache failure');
});
