// Unexpected-exit fixture: no accounts, files, model work or device access.
import readline from 'node:readline';
readline.createInterface({ input: process.stdin }).on('line', line => {
  const m = JSON.parse(line);
  process.stdout.write(JSON.stringify({ id: m.id, result: { active: m.providers } }) + '\n', () => setTimeout(() => process.exit(0), 30));
});
