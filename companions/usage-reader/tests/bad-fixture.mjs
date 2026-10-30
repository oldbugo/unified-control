// Malformed-output fixture, no account/device activity.
process.stdin.on('data', () => process.stdout.write('not-json\n'));
