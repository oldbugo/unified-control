// Rust pipe-contract fixture. Never calls createUsageReader or any provider/broker.
import readline from 'node:readline';
import { rasterize } from '../engine.mjs';
let active = [];
const snapshot = provider => ({ provider, status: 'unavailable', observedAt: null, provenanceVerified: false,
  fiveHour: null, weekly: null, tokenHistoryStatus: 'unavailable', tokenHistoryObservedAt: null, tokenHistory: [], message: 'Fixture has no account data.' });
const lines = readline.createInterface({ input: process.stdin });
lines.on('line', line => {
  const m = JSON.parse(line); let result;
  if (m.method === 'set_active') { active = m.providers; result = { active }; }
  else if (m.method === 'snapshot') result = snapshot(m.provider);
  else if (m.method === 'render') result = rasterize(snapshot(m.provider), m);
  else if (m.method === 'shutdown') { result = { stopped: true }; lines.close(); }
  process.stdout.write(JSON.stringify({ id: m.id, result }) + '\n');
});
