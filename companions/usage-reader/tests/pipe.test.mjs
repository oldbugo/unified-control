import test from 'node:test';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import readline from 'node:readline';
import { once } from 'node:events';
import { fileURLToPath } from 'node:url';
test('real pipe entry answers views/unknown only and exits without activating an account', async t => {
  const child = spawn(process.execPath, [fileURLToPath(new URL('../reader.mjs', import.meta.url))],
    { windowsHide: true, shell: false, stdio: ['pipe', 'pipe', 'ignore'] });
  t.after(() => child.kill());
  const lines = readline.createInterface({ input: child.stdout });
  const iterator = lines[Symbol.asyncIterator]();
  let id = 0;
  async function request(method, args = {}) {
    child.stdin.write(JSON.stringify({ id: ++id, method, ...args }) + '\n');
    const deadline = new Promise((_, reject) => { const timer = setTimeout(() => reject(Error('fixture pipe deadline')), 3000); timer.unref(); });
    const line = await Promise.race([iterator.next(), deadline]);
    const result = JSON.parse(line.value); assert.equal(result.id, id); assert.equal(result.error, undefined); return result.result;
  }
  assert.equal((await request('views')).length, 16);
  const inactive = await request('snapshot', { provider: 'codex' }); assert.equal(inactive.status, 'inactive'); assert.equal(inactive.fiveHour, null);
  const rendered = await request('render', { provider: 'codex', view: 'rings', width: 196, height: 196 });
  assert.equal(rendered.snapshot.fiveHour, null); assert.equal(Buffer.from(rendered.png).readUInt32BE(16), 196);
  const closed = once(child, 'exit'); await request('shutdown'); child.stdin.end();
  assert.equal((await closed)[0], 0); lines.close();
});
