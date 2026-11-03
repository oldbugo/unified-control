// The worker owns Live's connection and retry loop. Rebuilding its editor must
// not interrupt that loop with a second firmware check. An editor-only digital
// twin uses a null sink, so it does not prevent checking an otherwise-off screen.
export function shouldCheckScreenOnEntry(live) {
  const status = live?.status;
  if (live?.settings?.inApp !== false || !status) return false;
  if (status.previewing || status.calibrating || status.tuning) return false;
  return status.running === false || (status.running === true && status.twinning === true);
}
