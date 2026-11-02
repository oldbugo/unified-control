// Live but unresponsive helper fixture. No accounts, files or devices.
import readline from 'node:readline';
readline.createInterface({ input: process.stdin }).on('line', line => {
  const m = JSON.parse(line);
  if (m.method === 'set_active') process.stdout.write(JSON.stringify({ id: m.id, result: { active: m.providers } }) + '\n');
});
