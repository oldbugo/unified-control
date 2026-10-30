// JSON lines over inherited private pipes only. The sole loopback listener, when
// Claude is explicitly active, is the existing shared Toolkit broker protocol.
import { createUsageReader, VIEWS } from './engine.mjs';
import { createHistoryStore } from './source/history.js';
import path from 'node:path';
const reader = createUsageReader({ historyStore: createHistoryStore({
  ...(process.env.UC_USAGE_HOME ? { filePath: path.join(process.env.UC_USAGE_HOME, 'quota-history.json') } : {})
}) });
const MAX_LINE = 4096;
let buffered = '', queued = 0, chain = Promise.resolve(), stopped = false;
const reply = value => process.stdout.write(JSON.stringify(value) + '\n');
async function dispatch(line) {
  let message;
  try {
    message = JSON.parse(line);
    if (!Number.isSafeInteger(message?.id) || message.id < 1) throw new Error('invalid-request');
    let result;
    switch (message.method) {
      case 'set_active': result = await reader.setActive(message.providers); break;
      case 'snapshot': result = reader.snapshot(message.provider); break;
      case 'render': result = await reader.render(message.provider, message.view, message.width, message.height); break;
      case 'views': result = VIEWS; break;
      case 'shutdown': await reader.stop(); stopped = true; result = { stopped: true }; break;
      default: throw new Error('unsupported-method');
    }
    reply({ id: message.id, result });
    if (stopped) { process.stdin.destroy(); process.stdout.end(); }
  } catch { reply({ id: Number.isSafeInteger(message?.id) ? message.id : 0, error: 'Usage request failed.' }); }
}
process.stdin.setEncoding('utf8');
process.stdin.on('data', chunk => {
  if (stopped) return;
  buffered += chunk;
  if (buffered.length > MAX_LINE && !buffered.includes('\n')) { void reader.stop().finally(() => process.exit(2)); return; }
  let newline;
  while ((newline = buffered.indexOf('\n')) !== -1) {
    const line = buffered.slice(0, newline); buffered = buffered.slice(newline + 1);
    if (line.length > MAX_LINE || ++queued > 32) { void reader.stop().finally(() => process.exit(2)); return; }
    chain = chain.then(() => stopped ? undefined : dispatch(line)).finally(() => queued--);
  }
});
process.stdin.on('end', () => { void chain.finally(() => reader.stop()); });
process.stdin.on('error', () => { void reader.stop(); });
process.stdout.on('error', () => { void reader.stop().finally(() => process.exit(2)); });
for (const signal of ['SIGINT', 'SIGTERM']) process.on(signal, () => { void reader.stop().finally(() => process.exit(0)); });
