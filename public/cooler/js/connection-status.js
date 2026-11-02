// One UI view of the manual screen check and the current Live connection.
// Pure functions: status refreshes never open USB or restart the screen.
const FAILURES = new Set(['notFound', 'failedUsb', 'busy', 'noReply', 'error']);

export function deviceJobActive(jobs = [], deviceBusy = false) {
  return deviceBusy || jobs.some(j => ['send', 'setup', 'apply', 'recovery'].includes(j.kind) && ['running', 'waiting'].includes(j.status));
}

export function liveUsesDevice(live = {}) {
  const st = live.status || {};
  return !st.noDevice && !st.twinning && !!(live.settings?.inApp || st.previewing || st.calibrating || st.tuning);
}

export function liveHeld(live = {}) {
  const st = live.status || {};
  return !!(st.paused || st.deviceHold || st.systemHold || st.idle || st.layerOff);
}

export function screenStatus(screen, live = {}, { checking = false, deviceBusy = false, jobs = [] } = {}) {
  if (checking) return { ...screen, state: 'checking', source: 'check' };
  if (deviceJobActive(jobs, deviceBusy)) return { ...screen, state: 'updating', source: 'job' };
  const st = live.status || {};
  if (!liveUsesDevice(live) || liveHeld(live)) return screen;
  const c = st.connection;
  if (st.connected || c?.state === 'connected') {
    return { state: 'ready', info: screen.info, error: null, source: 'live', retryInSeconds: 0 };
  }
  if (!c || ['off', 'paused'].includes(c.state)) return screen;
  // A serial driver still needs setup before Live can use WinUSB.
  if (screen.state === 'setup' && !['failedUsb', 'notFound'].includes(c.state)) return screen;
  if (!FAILURES.has(c.state) && c.state !== 'connecting') return screen;
  return {
    state: c.state, info: null, error: typeof c.message === 'string' ? c.message : null,
    retryInSeconds: Number.isFinite(c.retryInSeconds) ? Math.max(0, Math.ceil(c.retryInSeconds)) : 0,
    source: 'live',
  };
}

export function retryMessage(status) {
  if (status.source !== 'live' || !FAILURES.has(status.state)) return '';
  return status.retryInSeconds > 0 ? ` Retrying in ${status.retryInSeconds} s.` : ' Retrying automatically.';
}

export function rememberedScreen(screen, live, options = {}) {
  const current = screenStatus(screen, live, options);
  return current.source === 'live' && current.state === 'ready'
    ? { state: 'ready', info: current.info, error: null } : screen;
}

export function connectionMessage(status) {
  const text = {
    connecting: 'Connecting to the screen…',
    failedUsb: 'Windows cannot recognise the screen’s USB connection. Reconnect needs Windows approval.',
    notFound: 'Screen not found. Check the cooler’s USB cable and power.',
    noReply: 'The screen is not responding to commands. Embedded may keep playing.',
    busy: status.error || 'Another program is using the screen.',
    error: status.error || 'Could not connect to the screen.',
  }[status.state] || '';
  return text + retryMessage(status);
}

export function liveMessage(live = {}, status) {
  const st = live.status || {};
  if (!live.settings?.inApp && !st.previewing && !st.calibrating && !st.tuning) return '';
  if (status?.source === 'check') return 'Checking the screen…';
  if (st.deviceHold || status?.source === 'job') return 'Paused: the screen is busy.';
  if (st.systemHold) return 'Paused: the PC sleeps or is locked.';
  if (st.paused) return 'Paused from the tray.';
  if (st.layerOff) return 'Live layer is off. The screen shows Embedded.';
  if (st.idle) return 'No live items. The screen shows Embedded.';
  if (st.twinning || st.noDevice) return 'Preview only: nothing is sent to the screen.';
  if (st.connected || st.connection?.state === 'connected') {
    const fps = Number(st.measuredFps) || 0;
    return fps > 0 ? `Running: ${fps.toFixed(1)} of ${st.fps >= 45 || !st.fps ? 60 : 30} frames per second sent.` : 'Running.';
  }
  return connectionMessage(status || { state: 'connecting' }) || 'Connecting to the screen…';
}

// After a failed check, watch only enumeration (no open or commands). A real
// device check is needed just once when it comes back. Never probe healthy Live.
export function shouldWatchPresence(screen, live, options = {}) {
  const st = live?.status || {};
  // Theme idle/layer flags can remain when no pipeline exists. They describe
  // dormant Live content, not a reason to ignore a returning Embedded screen.
  const held = st.deviceHold || st.systemHold || (live?.settings?.inApp && liveHeld(live));
  return !options.checking && !deviceJobActive(options.jobs, options.deviceBusy)
    && !liveUsesDevice(live) && !held
    && ['notFound', 'failedUsb'].includes(screen.state);
}

export function presenceArrival(previousKey, presence) {
  const key = presence.present ? presence.transport : 'none';
  return { key, arrived: !!presence.present && key !== previousKey };
}
