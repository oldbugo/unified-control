export const STALE_AFTER_MS = 5 * 60_000;
export const VIEW_OPTIONS = Object.freeze([
  { id: 'rings', label: 'Both allowances: rings', metric: 'both' },
  { id: 'bars', label: 'Both allowances: bars', metric: 'both' },
  { id: 'trend', label: 'Both allowances: quota history', metric: 'both' },
  { id: 'tokens', label: 'Daily token history (Codex)', metric: 'tokens', provider: 'codex' },
  { id: 'resets', label: 'Both reset countdowns', metric: 'both' },
  { id: 'five-hour-ring', label: 'Five-hour allowance: ring', metric: 'fiveHour' },
  { id: 'weekly-ring', label: 'Weekly allowance: ring', metric: 'weekly' },
  { id: 'five-hour-number', label: 'Five-hour allowance: large number', metric: 'fiveHour' },
  { id: 'weekly-number', label: 'Weekly allowance: large number', metric: 'weekly' },
  { id: 'five-hour-reset', label: 'Five-hour reset countdown', metric: 'fiveHour' },
  { id: 'weekly-reset', label: 'Weekly reset countdown', metric: 'weekly' },
  { id: 'five-hour-time', label: 'Five-hour reset: Sydney date and time', metric: 'fiveHour' },
  { id: 'weekly-time', label: 'Weekly reset: Sydney date and time', metric: 'weekly' },
  { id: 'five-hour-trend', label: 'Five-hour allowance: quota history', metric: 'fiveHour' },
  { id: 'weekly-trend', label: 'Weekly allowance: quota history', metric: 'weekly' },
  { id: 'wide-dashboard', label: 'Wide: both allowances and resets', metric: 'both' }
].map(Object.freeze));
const finite = (n) => typeof n === 'number' && Number.isFinite(n);
const fresh = (t, now) => finite(t) && t > 0 && t <= now + 60_000 && now - t <= STALE_AFTER_MS;
const escapeXml = (value) => String(value).replace(/[<>&"']/g, (char) => ({ '<': '&lt;', '>': '&gt;', '&': '&amp;', '"': '&quot;', "'": '&apos;' }[char]));

export function formatCountdown(resetsAt, now = Date.now()) {
  if (!finite(resetsAt) || resetsAt <= 0) return 'Unknown reset';
  const minutes = Math.ceil((resetsAt * 1000 - now) / 60_000);
  if (minutes <= 0) return 'Awaiting update';
  if (minutes < 60) return `${minutes}m`;
  if (minutes < 1440) return `${Math.floor(minutes / 60)}h ${minutes % 60}m`;
  return `${Math.floor(minutes / 1440)}d ${Math.floor((minutes % 1440) / 60)}h`;
}

export function formatSydney(resetsAt) {
  if (!finite(resetsAt) || resetsAt <= 0) return null;
  const date = new Date(resetsAt * 1000);
  if (!finite(date.getTime())) return null;
  const parts = Object.fromEntries(new Intl.DateTimeFormat('en-AU', {
    timeZone: 'Australia/Sydney', weekday: 'short', day: 'numeric', month: 'short',
    hour: '2-digit', minute: '2-digit', hourCycle: 'h23', timeZoneName: 'short'
  }).formatToParts(date).map(({ type, value }) => [type, value]));
  return { date: `${parts.weekday} ${parts.day} ${parts.month}`, time: `${parts.hour}:${parts.minute}`, zone: parts.timeZoneName };
}

function unknownWindow(state = 'missing', countdown = 'No reading') {
  return { state, value: '--', remainingValue: '--', usedPercent: null, remainingPercent: null, countdown, reset: null, width: 0 };
}

export function windowView(window, now = Date.now()) {
  if (!window || !finite(window.usedPercent) || window.usedPercent < 0 || window.usedPercent > 100) return unknownWindow();
  const reset = formatSydney(window.resetsAt);
  if (reset && window.resetsAt * 1000 <= now) return unknownWindow('expired', 'Awaiting update');
  const remainingPercent = 100 - window.usedPercent;
  return { state: 'available', value: `${Math.round(window.usedPercent)}% used`, usedPercent: window.usedPercent,
    remainingValue: `${Math.round(remainingPercent)}%`, remainingPercent,
    countdown: reset ? formatCountdown(window.resetsAt, now) : 'Reset unavailable', reset, width: window.usedPercent };
}

export function usageView(snapshot, now = Date.now()) {
  const hasObservation = finite(snapshot?.observedAt) && snapshot.observedAt > 0 && snapshot.observedAt <= now + 60_000;
  const stale = snapshot?.reason === 'stale-observation' || snapshot?.status === 'ok' && hasObservation && !fresh(snapshot.observedAt, now);
  const loading = !snapshot || snapshot.status === 'loading';
  const available = snapshot?.status === 'ok' && hasObservation && !stale;
  return {
    provider: snapshot?.provider === 'claude' ? 'Claude' : 'Codex',
    state: loading ? 'loading' : stale ? 'stale' : snapshot?.status === 'ok' && !hasObservation ? 'unavailable' : snapshot?.status,
    label: loading ? 'CONNECTING' : stale ? 'STALE' : available ? 'LEFT' : 'NO DATA',
    message: snapshot?.message || (loading ? 'Waiting for the first reading.' : stale ?
      'This reading is more than five minutes old.' : available ? 'Usage readings available.' : 'Usage is not available.'),
    observedAt: hasObservation ? snapshot.observedAt : null,
    fiveHour: windowView(available ? snapshot.fiveHour : null, now), weekly: windowView(available ? snapshot.weekly : null, now)
  };
}

const palette = (provider) => provider === 'claude'
  ? { primary: '#f69570', secondary: '#ffd39b', watermark: '#e6906f' }
  : { primary: '#64e5ce', secondary: '#a3cafa', watermark: '#72cfc2' };
const text = (x, y, value, size = 20, fill = '#f6f8fc', extra = '') =>
  `<text x="${x}" y="${y}" font-size="${size}" fill="${fill}" ${extra}>${escapeXml(value)}</text>`;
function percent(x, y, value, color, size = 30) {
  if (!finite(value)) return text(x, y, '--', size + 2, '#cbd3dc', 'text-anchor="middle"');
  return `<text x="${x}" y="${y}" font-size="${size}" fill="${color}" text-anchor="middle">${Math.round(value)}<tspan font-size="${Math.round(size * 0.54)}">%</tspan></text>`;
}

// Original vector identifiers, not official trademark artwork: a radial spark
// for Claude and a code-bracket glyph for Codex, faint behind the figures.
export function providerMark(provider, opacity = 0.08) {
  const color = palette(provider).watermark;
  if (provider === 'claude') {
    const rays = Array.from({ length: 12 }, (_, i) => `<path d="M72 29L72 55" transform="rotate(${i * 30} 72 72)"/>`).join('');
    return `<g stroke="${color}" stroke-width="7" stroke-linecap="round" opacity="${opacity}">${rays}</g>`;
  }
  return `<g fill="none" stroke="${color}" stroke-width="8" stroke-linecap="round" stroke-linejoin="round" opacity="${opacity}"><path d="M57 45L31 72L57 99M87 45L113 72L87 99M80 43L64 101"/></g>`;
}

function ring(radius, width, remaining, color, key) {
  const circumference = 2 * Math.PI * radius;
  const track = `<circle data-track="${key}" cx="72" cy="72" r="${radius}" fill="none" stroke="#27313c" stroke-width="${width}"/>`;
  if (!finite(remaining)) return track;
  return track + `<circle data-gauge="${key}" data-remaining="${remaining}" cx="72" cy="72" r="${radius}" fill="none" stroke="${color}" stroke-width="${width}" stroke-dasharray="${(circumference * remaining / 100).toFixed(3)} ${circumference.toFixed(3)}" transform="rotate(-90 72 72)"/>`;
}

function statusBadge(view) {
  if (view.state === 'ok') return '';
  const label = view.state === 'loading' ? 'WAIT' : view.state === 'stale' ? 'STALE' : 'NO DATA';
  const width = label.length > 5 ? 80 : 68;
  return `<rect x="${72 - width / 2}" y="120" width="${width}" height="20" rx="6" fill="#141b23" stroke="#d9aa63" stroke-width="1"/>` + text(72, 135, label, 14, '#ffcf86', 'text-anchor="middle"');
}

function rings(view, colors) {
  return ring(63, 12, view.fiveHour.remainingPercent, colors.primary, 'fiveHour') +
    ring(47, 10, view.weekly.remainingPercent, colors.secondary, 'weekly') +
    percent(72, 65, view.fiveHour.remainingPercent, colors.primary, 28) +
    '<path d="M48 73H96" stroke="#3c4653" stroke-width="1"/>' +
    percent(72, 98, view.weekly.remainingPercent, colors.secondary, 28);
}

function bars(view, colors) {
  const row = (window, y, color, key) => text(13, y - 9, key === 'fiveHour' ? '5h' : '7d', 16, '#a6b0bf') +
    percent(94, y - 7, window.remainingPercent, color, 36) +
    `<rect x="12" y="${y + 6}" width="120" height="17" rx="5" fill="#27313c"/>` +
    (finite(window.remainingPercent) ? `<rect data-gauge="${key}" data-remaining="${window.remainingPercent}" x="12" y="${y + 6}" width="${(120 * window.remainingPercent / 100).toFixed(2)}" height="17" rx="5" fill="${color}"/>` : '');
  return row(view.fiveHour, 40, colors.primary, 'fiveHour') + row(view.weekly, 105, colors.secondary, 'weekly');
}

// Historical quota observations are not token counts. Break paths at resets,
// missing samples, polling gaps or allowance increases; never invent zeroes.
export function trendSegments(history, key, now = Date.now()) {
  const source = Array.isArray(history) ? history.filter((entry) => finite(entry?.observedAt) && entry.observedAt > 0 && entry.observedAt <= now + 60_000).sort((a, b) => a.observedAt - b.observedAt) : [];
  const segments = [];
  let segment = [];
  let previous;
  const flush = () => { if (segment.length > 1) segments.push(segment); segment = []; };
  for (const entry of source) {
    const raw = entry[key];
    if ((entry.status && entry.status !== 'ok') || !raw || !finite(raw.usedPercent) || raw.usedPercent < 0 || raw.usedPercent > 100 ||
      (raw.resetsAt != null && (!finite(raw.resetsAt) || raw.resetsAt * 1000 <= entry.observedAt))) { flush(); previous = undefined; continue; }
    const point = { observedAt: entry.observedAt, remainingPercent: 100 - raw.usedPercent, resetsAt: raw.resetsAt ?? null };
    if (previous && point.observedAt === previous.observedAt) continue;
    if (previous && (point.observedAt - previous.observedAt > STALE_AFTER_MS || point.resetsAt !== previous.resetsAt || point.remainingPercent > previous.remainingPercent + 0.001)) flush();
    segment.push(point); previous = point;
  }
  flush(); return segments;
}

function trends(view, colors, history, now) {
  const sets = ['fiveHour', 'weekly'].map((key) => view.state === 'ok' && view[key].state === 'available' ? trendSegments(history, key, now) : []);
  if (!sets.some((set) => set.length)) return text(72, 75, '--', 43, '#cbd3dc', 'text-anchor="middle"') + text(72, 101, view.state === 'ok' ? 'NO HISTORY' : 'NO LIVE DATA', 16, '#a6b0bf', 'text-anchor="middle"');
  const points = sets.flat(2);
  const earliest = Math.min(...points.map((p) => p.observedAt));
  const latest = Math.max(...points.map((p) => p.observedAt));
  return sets.map((segments, i) => {
    const top = i === 0 ? 42 : 105;
    const color = i === 0 ? colors.primary : colors.secondary;
    const key = i === 0 ? 'fiveHour' : 'weekly';
    const header = text(12, top - 17, i === 0 ? '5h' : '7d', 16, '#a6b0bf') + percent(100, top - 13, segments.length ? view[key].remainingPercent : null, color, 29);
    if (!segments.length) return header + text(72, top + 24, 'NO HISTORY', 14, '#a6b0bf', 'text-anchor="middle"');
    return header + `<path d="M12 ${top + 32}H132" stroke="#27313c" stroke-width="2"/>` + segments.map((segment) => {
      const path = segment.map((point, index) => `${index ? 'L' : 'M'}${(12 + 120 * (point.observedAt - earliest) / Math.max(1, latest - earliest)).toFixed(2)} ${(top + 32 - 32 * point.remainingPercent / 100).toFixed(2)}`).join(' ');
      return `<path data-trend="${key}" d="${path}" fill="none" stroke="${color}" stroke-width="5" stroke-linecap="round" stroke-linejoin="round"/>`;
    }).join('');
  }).join('');
}

function resets(view, colors) {
  const row = (window, y, label, color) => {
    const countdown = window.state === 'available' && window.reset ? window.countdown : '--';
    return text(14, y - 24, label, 16, '#a6b0bf') + text(72, y + 7, countdown, countdown.length > 7 ? 24 : 30, color, 'text-anchor="middle"');
  };
  return row(view.fiveHour, 44, '5h reset', colors.primary) + row(view.weekly, 111, '7d reset', colors.secondary);
}

export function tokenSeries(history) {
  if (!Array.isArray(history)) return [];
  const days = new Map();
  for (const item of history) {
    if (!/^\d{4}-\d{2}-\d{2}$/.test(item?.date || '') || !finite(item.tokens) || item.tokens < 0) continue;
    const day = Date.parse(item.date + 'T00:00:00Z');
    if (!finite(day) || new Date(day).toISOString().slice(0, 10) !== item.date) continue;
    days.set(item.date, { date: item.date, tokens: item.tokens, day });
  }
  const sorted = [...days.values()].sort((a, b) => a.day - b.day);
  const cutoff = sorted.length ? sorted.at(-1).day - 13 * 86400_000 : 0;
  return sorted.filter((point) => point.day >= cutoff);
}

const tokenNumber = (n) => n >= 1e9 ? `${(n / 1e9).toFixed(1)}B` : n >= 1e6 ? `${(n / 1e6).toFixed(1)}M` : n >= 1e3 ? `${(n / 1e3).toFixed(1)}k` : String(Math.round(n));
function tokens(snapshot, colors, now) {
  const history = tokenSeries(snapshot?.provider === 'claude' ? [] : snapshot?.tokenHistory);
  const observedAt = snapshot?.tokenHistoryObservedAt ?? snapshot?.tokenObservedAt;
  // Token history refreshes hourly, independent of the minute-by-minute quota reader.
  const valid = (!snapshot?.tokenHistoryStatus || snapshot.tokenHistoryStatus === 'ok') && finite(observedAt) && observedAt > 0 && observedAt <= now + 60_000 && now - observedAt <= 2 * 3600_000;
  if (!valid || !history.length) return text(72, 79, '--', 46, '#cbd3dc', 'text-anchor="middle"') + text(72, 107, 'NO TOKEN DATA', 14, '#a6b0bf', 'text-anchor="middle"');
  const latest = history.at(-1);
  const maximum = Math.max(1, ...history.map((point) => point.tokens));
  const span = Math.max(14 * 86400_000, latest.day - history[0].day + 86400_000);
  const start = latest.day - span + 86400_000;
  const chart = history.map((point) => {
    const height = 47 * point.tokens / maximum;
    const x = 12 + 120 * (point.day - start) / span;
    return `<rect data-token-date="${point.date}" data-tokens="${point.tokens}" x="${x.toFixed(2)}" y="${(133 - height).toFixed(2)}" width="6" height="${height.toFixed(2)}" rx="1.5" fill="${colors.primary}"/>`;
  }).join('');
  return text(72, 24, 'TOKENS / DAY', 14, '#a6b0bf', 'text-anchor="middle"') + text(72, 61, tokenNumber(latest.tokens), 34, colors.primary, 'text-anchor="middle"') + text(72, 78, latest.date.slice(5), 12, '#a6b0bf', 'text-anchor="middle"') + chart;
}

function single(view, colors, presentation, history, now) {
  const key = presentation.startsWith('weekly-') ? 'weekly' : 'fiveHour';
  const label = key === 'fiveHour' ? '5H' : '7D';
  const window = view[key];
  const color = key === 'fiveHour' ? colors.primary : colors.secondary;
  const kind = presentation.slice(presentation.lastIndexOf('-') + 1);
  if (kind === 'ring') return ring(61, 12, window.remainingPercent, color, key) +
    text(72, 45, `${label} LEFT`, 14, '#a6b0bf', 'text-anchor="middle"') + percent(72, 94, window.remainingPercent, color, 44);
  if (kind === 'number') return text(72, 28, `${label} LEFT`, 16, '#a6b0bf', 'text-anchor="middle"') + percent(72, 101, window.remainingPercent, color, 58);
  if (kind === 'reset') {
    const countdown = window.state === 'available' && window.reset ? window.countdown : '--';
    const size = countdown.length <= 4 ? 54 : countdown.length <= 6 ? 40 : 32;
    return text(72, 28, `${label} RESET`, 16, '#a6b0bf', 'text-anchor="middle"') + text(72, 91, countdown, size, color, 'text-anchor="middle"');
  }
  if (kind === 'time') return text(72, 26, `${label} RESET`, 16, '#a6b0bf', 'text-anchor="middle"') +
    text(72, 83, window.reset?.time || '--', 46, color, 'text-anchor="middle"') +
    (window.reset ? text(72, 112, window.reset.date, 18, '#d1d9e5', 'text-anchor="middle"') + text(72, 135, window.reset.zone, 14, '#a6b0bf', 'text-anchor="middle"') : '');
  const segments = view.state === 'ok' && window.state === 'available' ? trendSegments(history, key, now) : [];
  const header = text(13, 26, `${label} LEFT`, 15, '#a6b0bf') + percent(106, 50, window.remainingPercent, color, 33);
  if (!segments.length) return header + text(72, 93, '--', 32, '#cbd3dc', 'text-anchor="middle"') + text(72, 115, 'NO HISTORY', 14, '#a6b0bf', 'text-anchor="middle"');
  const points = segments.flat();
  const earliest = Math.min(...points.map((point) => point.observedAt));
  const latest = Math.max(...points.map((point) => point.observedAt));
  return header + '<path d="M12 133H132" stroke="#27313c" stroke-width="2"/>' + segments.map((segment) => {
    const path = segment.map((point, index) => `${index ? 'L' : 'M'}${(12 + 120 * (point.observedAt - earliest) / Math.max(1, latest - earliest)).toFixed(2)} ${(133 - 65 * point.remainingPercent / 100).toFixed(2)}`).join(' ');
    return `<path data-trend="${key}" d="${path}" fill="none" stroke="${color}" stroke-width="6" stroke-linecap="round" stroke-linejoin="round"/>`;
  }).join('');
}

function wideDashboard(view, colors) {
  const panel = (window, x, label, color, key) => {
    const countdown = window.state === 'available' && window.reset ? window.countdown : '--';
    return `<g transform="translate(${x} 0)">` + text(72, 23, `${label} LEFT`, 15, '#a6b0bf', 'text-anchor="middle"') +
      percent(72, 73, window.remainingPercent, color, 42) +
      `<rect x="15" y="85" width="114" height="9" rx="4" fill="#27313c"/>` +
      (finite(window.remainingPercent) ? `<rect data-gauge="${key}" data-remaining="${window.remainingPercent}" x="15" y="85" width="${(114 * window.remainingPercent / 100).toFixed(2)}" height="9" rx="4" fill="${color}"/>` : '') +
      text(72, 123, countdown, 24, '#d1d9e5', 'text-anchor="middle"') + '</g>';
  };
  return panel(view.fiveHour, 0, '5H', colors.primary, 'fiveHour') + '<path d="M144 18V126" stroke="#34404d" stroke-width="1"/>' + panel(view.weekly, 144, '7D', colors.secondary, 'weekly');
}

export function renderUsageSvg(snapshot, options = {}) {
  const { provider = snapshot?.provider || 'codex', now = Date.now(), history = [] } = options;
  const requested = options.view ?? (options.details ? 'resets' : 'rings');
  const presentation = VIEW_OPTIONS.some((option) => option.id === requested) ? requested : 'rings';
  const view = usageView(snapshot ?? { provider, status: 'loading' }, now);
  const colors = palette(provider);
  const dashboard = presentation === 'wide-dashboard';
  const wide = options.format === 'wide' || dashboard && options.format !== 'square';
  const width = wide ? 288 : 144;
  const contentWidth = dashboard ? 288 : 144;
  const offset = (width - contentWidth) / 2;
  const transform = dashboard && !wide ? 'translate(0 36) scale(0.5)' : `translate(${offset} 0)`;
  const content = dashboard ? wideDashboard(view, colors) : presentation.startsWith('five-hour-') || presentation.startsWith('weekly-') ? single(view, colors, presentation, history, now) :
    presentation === 'bars' ? bars(view, colors) : presentation === 'trend' ? trends(view, colors, history, now) :
      presentation === 'tokens' ? tokens(snapshot, colors, now) : presentation === 'resets' ? resets(view, colors) : rings(view, colors);
  const mark = dashboard ? providerMark(provider) + `<g transform="translate(144 0)">${providerMark(provider)}</g>` : providerMark(provider);
  const badge = presentation === 'tokens' ? '' : dashboard ? `<g transform="translate(72 0)">${statusBadge(view)}</g>` : statusBadge(view);
  const accessible = presentation === 'tokens' ? `${provider} actual daily token history` : `${provider} ${VIEW_OPTIONS.find((option) => option.id === presentation).label}; allowance remaining`;
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="144" viewBox="0 0 ${width} 144" role="img" aria-label="${escapeXml(accessible)}"><rect width="${width}" height="144" rx="17" fill="#10161f"/><g transform="${transform}">${mark}<g font-family="Segoe UI,Arial,sans-serif" font-weight="700" font-variant-numeric="tabular-nums">${content}${badge}</g></g></svg>`;
}
