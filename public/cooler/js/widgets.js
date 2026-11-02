// 3.0 theme editor: the item types (docs/v3/theme-format.md, v1.6) - the palette, default sizes, names and the
// property panels per type. themes.js owns the editor state; the panels here get what they need through `api`.

import * as More from './morewidgets.js';

export const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));
export const r4 = v => Math.round(v * 10000) / 10000;
/** Width fraction of a box that is square on screen when its height fraction is h (the canvas is 2400x1080). */
const sq = h => r4(h * 1080 / 2400);

// ---------------------------------------------------------------------- sensors
// [id, group, name]
export const METRICS = [
  ['cpu.load', 'CPU', 'CPU load'], ['cpu.core', 'CPU', 'CPU core'], ['cpu.temp', 'CPU', 'CPU temperature'], ['cpu.clock', 'CPU', 'CPU speed'], ['cpu.power', 'CPU', 'CPU power'], ['cpu.fan', 'CPU', 'CPU fan'],
  ['gpu.load', 'GPU', 'GPU load'], ['gpu.temp', 'GPU', 'GPU temperature'], ['gpu.clock', 'GPU', 'GPU speed'], ['gpu.vram', 'GPU', 'GPU memory'],
  ['gpu.power', 'GPU', 'GPU power'], ['gpu.fan', 'GPU', 'GPU fan'],
  ['board.temp', 'Board', 'Board temperature'], ['ram.used', 'Memory', 'RAM'],
  ['net.down', 'Network', 'Download'], ['net.up', 'Network', 'Upload'],
  ['disk.used', 'Disk', 'Disk used'], ['disk.activity', 'Disk', 'Disk activity'], ['disk.read', 'Disk', 'Disk read speed'], ['disk.write', 'Disk', 'Disk write speed'],
];
export const metricName = id => (METRICS.find(m => m[0] === id) || [])[2] || 'Sensor';
const isTemp = m => m === 'cpu.temp' || m === 'gpu.temp' || m === 'board.temp';

const ICON_SVG = {
  number: '<text x="24" y="23" text-anchor="middle" font-size="17" font-weight="700" fill="currentColor">42</text>',
  bar: '<rect x="5" y="12" width="38" height="8" rx="3" fill="none" stroke="currentColor" stroke-width="1.6"/><rect x="5" y="12" width="24" height="8" rx="3" fill="currentColor"/>',
  ring: '<circle cx="24" cy="16" r="10" fill="none" stroke="currentColor" stroke-width="3" opacity=".3"/><path d="M24 6a10 10 0 1 1-9.5 13" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round"/>',
  gauge: '<path d="M13 26a12 12 0 1 1 22 0" fill="none" stroke="currentColor" stroke-width="3" opacity=".3" stroke-linecap="round"/><path d="M13 26a12 12 0 0 1 4-14" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round"/>',
  graph: '<path d="M4 27 11 17 18 21 26 9 34 15 44 6V28H4z" fill="currentColor" opacity=".35"/><path d="M4 27 11 17 18 21 26 9 34 15 44 6" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round"/>',
  sparkline: '<path d="M4 22 10 15 16 20 24 8 31 17 38 12 44 14" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round" stroke-linecap="round"/>',
};
const styleIcon = id => `<svg viewBox="0 0 48 32" width="44" height="30" aria-hidden="true">${ICON_SVG[id]}</svg>`;
const SENSOR_STYLES = [['number', 'Number'], ['bar', 'Bar'], ['ring', 'Ring'], ['gauge', 'Gauge'], ['graph', 'Graph'], ['sparkline', 'Line']];

/** Default box [w, h] (fractions of the canvas) for a type and its props: rings square, graphs wide, numbers compact. */
export function defaultSize(type, p = {}) {
  const more = More.defaultSize(type, p);
  if (more) return more;
  switch (type) {
    case 'sensor': return ({ number: [0.2, 0.22], bar: [0.3, 0.14], ring: [sq(0.34), 0.34], gauge: [sq(0.38), 0.38], graph: [0.34, 0.26], sparkline: [0.24, 0.12] })[p.style || 'number'] || [0.2, 0.22];
    case 'clock': return ({ digital: [0.3, 0.34], analog: [sq(0.4), 0.4], minimal: [0.4, 0.3] })[p.style || 'digital'] || [0.3, 0.34];
    case 'date': return ({ line: [0.34, 0.1], calendar: [0.22, 0.46], big: [0.16, 0.3] })[p.style || 'line'] || [0.34, 0.1];
    case 'weather': return p.style === 'forecast' ? [0.42, 0.3] : [0.22, 0.3];
    case 'visualizer': return ({ 'bars-h': [0.4, 0.3], ring: [sq(0.4), 0.4], 'wave-fade': [0.4, 0.2] })[p.style || 'bars-h'] || [0.4, 0.3];
    case 'slideshow': return [0.3, 0.4];
    case 'info': return ({ uptime: [0.2, 0.1], cpuName: [0.3, 0.08], gpuName: [0.3, 0.08], countdown: [0.3, 0.2], topProcesses: [0.25, 0.3] })[p.kind || 'uptime'] || [0.2, 0.1];
    case 'music': return [0.34, 0.22];
    case 'stats': return [0.26, 0.5];
    case 'text': return [0.34, 0.16];
    case 'image': return [0.2, 0.3];
    default: return [0.2, 0.2];
  }
}

// ---------------------------------------------------------------------- the palette
// [key, type, name, icon, props()]; the keys are what a drag carries. "sensor" opens the metric picker when clicked.
const sensorProps = (metric, style) => () => ({ metric, style });
export const ENTRIES = {
  clock: { type: 'clock', name: 'Clock', icon: '◔', props: () => ({ format: '24h', seconds: false, date: true, color: '#ffffff', style: 'digital' }) },
  date: { type: 'date', name: 'Date', icon: '▦', props: () => ({ style: 'line', color: '#ffffff' }) },
  sensor: { type: 'sensor', name: 'Sensor', icon: '◉', props: sensorProps('cpu.load', 'number'), pick: true },
  cpu: { type: 'sensor', name: 'CPU', icon: '▥', props: sensorProps('cpu.load', 'ring') },
  gpu: { type: 'sensor', name: 'GPU', icon: '▥', props: sensorProps('gpu.load', 'ring') },
  ram: { type: 'sensor', name: 'RAM', icon: '▥', props: sensorProps('ram.used', 'ring') },
  net: { type: 'sensor', name: 'Network', icon: '⇅', props: sensorProps('net.down', 'graph') },
  disk: { type: 'sensor', name: 'Disk', icon: '▤', props: sensorProps('disk.used', 'bar') },
  stats: { type: 'stats', name: 'Stats panel', icon: '☰', props: () => ({ items: ['cpu', 'ram', 'gpuLoad', 'gpuTemp', 'gpuClock'] }) },
  music: { type: 'music', name: 'Music', icon: '♪', props: () => ({ cover: true, progress: true }) },
  visualizer: { type: 'visualizer', name: 'Visualizer', icon: '▂', props: () => ({ style: 'bars-h', bands: 32, color: '#5ac8ff', sensitivity: 1 }) },
  slideshow: { type: 'slideshow', name: 'Slideshow', icon: '▧', props: () => ({ files: [], seconds: 8, fade: 1, fit: 'cover' }) },
  image: { type: 'image', name: 'Picture', icon: '▣', props: () => ({ file: '', opacity: 1, fit: 'contain' }) },
  weather: { type: 'weather', name: 'Weather', icon: '☁', props: weatherProps },
  uptime: { type: 'info', name: 'Uptime', icon: '⏱', props: () => ({ kind: 'uptime', color: '#ffffff' }) },
  countdown: { type: 'info', name: 'Countdown', icon: '⏳', props: () => ({ kind: 'countdown', target: nextNewYear(), color: '#ffffff' }) },
  cpuName: { type: 'info', name: 'CPU/GPU name', icon: 'ⓘ', props: () => ({ kind: 'cpuName', color: '#ffffff' }) },
  procs: { type: 'info', name: 'Top processes', icon: '≣', props: () => ({ kind: 'topProcesses', by: 'cpu', count: 4, color: '#ffffff' }) },
  text: { type: 'text', name: 'Text', icon: 'T', props: () => ({ text: 'Your text', size: 0.08, color: '#ffffff', opacity: 1, align: 'left', weight: 'regular' }) },
};
Object.assign(ENTRIES, More.ENTRIES);
let appRef = null;
/** The weather unit follows the default chosen in Settings. */
function weatherProps() {
  return { style: 'now', unit: (appRef && appRef.weather && appRef.weather.unit) || 'C', span: 'days', count: 4, color: '#ffffff' };
}
export const setApp = a => { appRef = a; };
export const defaultTempUnit = () => (appRef && appRef.weather && appRef.weather.unit) || 'C';
function nextNewYear() { return `${new Date().getFullYear() + 1}-01-01T00:00`; }

export const PALETTE = [
  ['Time', ['clock', 'date']],
  ['Sensors', ['sensor', 'cpu', 'gpu', 'ram', 'net', 'disk', 'stats']],
  ['Media', ['music', 'visualizer', 'slideshow', 'image']],
  ['Info', ['weather', 'uptime', 'countdown', 'cpuName', 'procs']],
  ['Decoration', ['text']],
];
for (const [group, keys] of PALETTE) if (More.PALETTE_ADD[group]) keys.push(...More.PALETTE_ADD[group]);
const TYPE_ICON = Object.fromEntries(Object.values(ENTRIES).map(e => [e.type, e.icon]));
export const typeIcon = t => TYPE_ICON[t] || '?';
const INFO_KINDS = [['uptime', 'Uptime'], ['cpuName', 'CPU name'], ['gpuName', 'GPU name'], ['countdown', 'Countdown'], ['topProcesses', 'Top processes']];
const TYPE_NAME = { ...More.TYPE_NAME, clock: 'Clock', date: 'Date', sensor: 'Sensor', stats: 'Stats panel', music: 'Music', visualizer: 'Visualizer', slideshow: 'Slideshow', image: 'Picture', weather: 'Weather', text: 'Text', info: 'Info' };
export const typeTitle = w => (w.type === 'info' ? (INFO_KINDS.find(k => k[0] === (w.props || {}).kind) || [])[1] : null) || TYPE_NAME[w.type] || w.type;
/** The base text/pictures can be baked into the stored base. */
export const canBeOnBase = type => type === 'text' || type === 'image';

export function nameOf(w) {
  const p = w.props || {};
  if (p.name) return p.name;
  if (More.isMore(w.type)) return More.nameOf(w);
  if (w.type === 'text') { const t = (p.text || '').replace(/\s+/g, ' ').trim(); return t ? `Text: “${t.slice(0, 22)}${t.length > 22 ? '…' : ''}”` : 'Text'; }
  if (w.type === 'image') return p.file ? `Picture: ${p.file.split('/').pop()}` : 'Picture (none chosen)';
  if (w.type === 'sensor') return typeof p.label === 'string' && p.label ? p.label : metricName(p.metric);
  if (w.type === 'slideshow') return `Slideshow (${(p.files || []).length})`;
  return typeTitle(w);
}

// ---------------------------------------------------------------------- property panels
const DEFAULT_STEPS = () => [{ at: 70, color: '#f0b43c' }, { at: 85, color: '#ff6b6b' }];

/**
 * Append the property controls of `w` (its type) to `host`. `api`: el, h (field, select2, range, numInput, swRow), set(key, v),
 * edited(), repaint(), moved(w), S, backend, toast, oid, openSettings(section).
 */
export function paintTypeProps(w, host, api) {
  const { el, h } = api;
  const p = w.props;
  const { field, select2, range, numInput, swRow } = h;
  const colour = (key, text = 'Colour', dflt = '#ffffff', resettable = false) => {
    const input = el('input', { type: 'color', id: 'pCol-' + key, value: p[key] || dflt, oninput: e => { api.set(key, e.target.value); if (reset) reset.disabled = false; } });
    input.value = p[key] || dflt;
    let reset = null;
    if (resettable) {
      reset = el('button', { type: 'button', class: 'icon-btn', title: 'Automatic', 'aria-label': 'Automatic colour', disabled: !p[key], text: '↺', onclick: () => { api.set(key, null); input.value = dflt; reset.disabled = true; } });
    }
    return field(text, resettable ? el('div', { class: 'inline-ctl' }, input, reset) : input, 'pCol-' + key);
  };
  /** Changing a style resizes the box to the new style's default, unless the user sized it by hand. */
  const setStyle = (key, v) => {
    const before = defaultSize(w.type, p);
    const mine = Math.abs(w.w - before[0]) < 0.003 && Math.abs(w.h - before[1]) < 0.003;
    p[key] = v;
    if (mine) {
      const [nw, nh] = defaultSize(w.type, p);
      w.w = r4(clamp(nw, 0.02, 1)); w.h = r4(clamp(nh, 0.02, 1));
      w.x = r4(clamp(w.x, 0, 1 - w.w)); w.y = r4(clamp(w.y, 0, 1 - w.h));
      api.moved(w);
    }
    api.edited(); api.repaint();
  };
  const num = (id, key, { min, max, step = 1, placeholder = '' }) => {
    const i = numInput(id, p[key], { min, max, step, onchange: v => api.set(key, v == null ? null : clamp(v, min, max)) });
    if (placeholder) i.placeholder = placeholder;
    return i;
  };

  switch (w.type) {
    case 'sensor': return sensorPanel();
    case 'clock': {
      const st = p.style || 'digital';
      host.append(
        field('Style', select2('pStyle', st, [['digital', 'Digital'], ['analog', 'Analogue'], ['minimal', 'Minimal']], v => setStyle('style', v)), 'pStyle'),
        ...(st === 'analog' ? [] : [field('Format', select2('pFmt', p.format || '24h', [['24h', '24-hour'], ['12h', '12-hour']], v => api.set('format', v)), 'pFmt')]),
        swRow('Seconds', p.seconds, v => api.set('seconds', v)),
        ...(st === 'digital' ? [swRow('Date', p.date !== false, v => api.set('date', v))] : []),
        colour('color'));
      return;
    }
    case 'date':
      host.append(field('Style', select2('pStyle', p.style || 'line', [['line', 'Line'], ['calendar', 'Calendar'], ['big', 'Big day']], v => setStyle('style', v)), 'pStyle'), colour('color'));
      return;
    case 'weather': {
      const place = api.S.app && api.S.app.weather && api.S.app.weather.place;
      if (!place) host.append(el('p', { class: 'small' }, el('button', { type: 'button', class: 'linkbtn', text: 'Set your city', onclick: () => api.openSettings('setWeather') })));
      else host.append(el('p', { class: 'muted small', text: place.name }));
      const st = p.style || 'now';
      host.append(
        field('Style', select2('pStyle', st, [['now', 'Now'], ['forecast', 'Forecast']], v => setStyle('style', v)), 'pStyle'),
        field('Unit', select2('pUnit', p.unit || 'C', [['C', '°C'], ['F', '°F']], v => api.set('unit', v)), 'pUnit'));
      if (st === 'forecast') {
        const hours = p.span === 'hours';
        host.append(
          field('Show', select2('pSpan', p.span || 'days', [['days', 'Days'], ['hours', 'Hours']], v => { p.span = v; p.count = clamp(p.count || 4, 3, v === 'hours' ? 6 : 5); api.edited(); api.repaint(); }), 'pSpan'),
          field('Count', select2('pCount', String(clamp(p.count || 4, 3, hours ? 6 : 5)), (hours ? [3, 4, 5, 6] : [3, 4, 5]).map(n => [String(n), String(n)]), v => api.set('count', Number(v))), 'pCount'));
      }
      host.append(colour('color'));
      return;
    }
    case 'visualizer':
      host.append(
        field('Style', select2('pStyle', p.style || 'bars-h', [['bars-h', 'Bars'], ['ring', 'Ring'], ['wave-fade', 'Wave']], v => setStyle('style', v)), 'pStyle'),
        field('Bands', range('pBands', clamp(p.bands || 32, 16, 64), 16, 64, 1, v => api.set('bands', Math.round(v)), v => String(v)), 'pBands'),
        colour('color', 'Colour', '#5ac8ff'),
        field('Sensitivity', range('pSens', clamp(p.sensitivity ?? 1, 0.2, 4), 0.2, 4, 0.1, v => api.set('sensitivity', r4(v)), v => '×' + v.toFixed(1)), 'pSens'));
      return;
    case 'slideshow': return slideshowPanel();
    case 'info': return infoPanel();
    default: return More.isMore(w.type) ? More.paintProps(w, host, api, { METRICS, metricName, tempUnit: defaultTempUnit }) : false;
  }

  function sensorPanel() {
    const metric = p.metric || 'cpu.load';
    const style = p.style || 'number';
    // the metric, grouped
    const sel = el('select', { id: 'pMetric', onchange: e => { p.metric = e.target.value; api.edited(); api.repaint(); api.relabel(); } });
    let og = null, last = '';
    for (const [id, group, name] of METRICS) {
      if (group !== last) { og = el('optgroup', { label: group }); sel.append(og); last = group; }
      og.append(el('option', { value: id, text: name }));
    }
    sel.value = metric;
    host.append(field('Metric', sel, 'pMetric'), More.extNote(api, metric));
    if (metric === 'cpu.core') host.append(field('Core', num('pCore', 'core', { min: 0, max: 255 }), 'pCore'));
    if (metric === 'disk.used') {
      const d = el('input', { type: 'text', id: 'pDrive', maxLength: 2, spellcheck: false, placeholder: 'C:', onchange: e => { const v = e.target.value.trim().toUpperCase(); api.set('drive', /^[A-Z]:?$/.test(v) ? v.replace(/:?$/, ':') : 'C:'); e.target.value = p.drive; } });
      d.value = p.drive || 'C:';
      host.append(field('Drive', d, 'pDrive'));
    }
    // the six styles as small pictures
    const grid = el('div', { class: 'styles', role: 'group', 'aria-label': 'Style' });
    for (const [id, text] of SENSOR_STYLES) {
      const b = el('button', { type: 'button', class: 'sty', 'aria-pressed': String(style === id), title: text, onclick: () => setStyle('style', id) });
      b.innerHTML = styleIcon(id) + `<span>${text}</span>`;
      grid.append(b);
    }
    host.append(el('p', { class: 'small muted', text: 'Style' }), grid);
    // label: default name, own text, or none
    const none = typeof p.label === 'string' && p.label === '';
    const lab = el('input', { type: 'text', id: 'pLabel', maxLength: 40, spellcheck: false, placeholder: metricName(metric), disabled: none,
      oninput: e => { api.set('label', e.target.value.trim() ? e.target.value : null); } });
    lab.value = typeof p.label === 'string' ? p.label : '';
    host.append(field('Label', lab, 'pLabel'), swRow('Show label', !none, v => { p.label = v ? null : ''; api.edited(); api.repaint(); api.relabel(); }));
    if (isTemp(metric)) host.append(field('Unit', select2('pUnit', p.unit || defaultTempUnit(), [['C', '°C'], ['F', '°F']], v => api.set('unit', v)), 'pUnit'));
    host.append(field('Decimals', num('pDec', 'decimals', { min: 0, max: 3, placeholder: 'auto' }), 'pDec'));
    if (['bar', 'ring', 'gauge', 'graph'].includes(style)) {
      host.append(el('div', { class: 'props-grid' },
        field('Min', num('pMin', 'min', { min: -1e9, max: 1e9, step: 'any', placeholder: 'auto' }), 'pMin'),
        field('Max', num('pMax', 'max', { min: -1e9, max: 1e9, step: 'any', placeholder: 'auto' }), 'pMax')));
    }
    host.append(thresholdsEditor(), colour('color', 'Colour', '#5ac8ff', true));
    if (style === 'graph' || style === 'sparkline') host.append(field('History (s)', num('pHist', 'history', { min: 5, max: 600, placeholder: '60' }), 'pHist'));
    host.append(field('Rate', select2('pRate', String(p.rate || 1), [['1', '1 / s'], ['2', '2 / s'], ['4', '4 / s']], v => api.set('rate', Number(v))), 'pRate'));
  }

  function thresholdsEditor() {
    const wrap = el('div', { class: 'thr' });
    const draw = () => {
      wrap.textContent = '';
      const head = el('div', { class: 'row between' }, el('span', { class: 'small muted', text: 'Colour steps' }));
      wrap.append(head);
      if (!Array.isArray(p.thresholds)) {
        head.append(el('button', { type: 'button', class: 'btn ghost small-btn', text: 'Edit', onclick: () => { p.thresholds = DEFAULT_STEPS(); api.edited(); draw(); } }));
        wrap.append(el('p', { class: 'muted small', text: 'Default' }));
        return;
      }
      head.append(el('span', { class: 'inline-ctl' },
        el('button', { type: 'button', class: 'iconbtn', title: 'Default', 'aria-label': 'Back to the default steps', text: '↺', onclick: () => { delete p.thresholds; api.edited(); draw(); } }),
        el('button', { type: 'button', class: 'iconbtn', title: 'Add a step', 'aria-label': 'Add a step', text: '+', onclick: () => {
          if (p.thresholds.length >= 6) return;
          const last = p.thresholds.length ? p.thresholds[p.thresholds.length - 1].at : 50;
          p.thresholds.push({ at: last + 10, color: '#ff6b6b' }); api.edited(); draw();
        } })));
      if (!p.thresholds.length) wrap.append(el('p', { class: 'muted small', text: 'None' }));
      p.thresholds.forEach((t, i) => {
        const at = el('input', { type: 'number', 'aria-label': 'From value', step: 'any', onchange: e => {
          const v = Number(e.target.value);
          t.at = Number.isFinite(v) ? v : t.at; p.thresholds.sort((a, b) => a.at - b.at); api.edited(); draw();
        } });
        at.value = t.at;
        const col = el('input', { type: 'color', 'aria-label': 'Colour', oninput: e => { t.color = e.target.value; api.edited(); } });
        col.value = t.color;
        wrap.append(el('div', { class: 'thr-row' }, el('span', { class: 'small muted', text: '≥' }), at, col,
          el('button', { type: 'button', class: 'iconbtn', title: 'Remove', 'aria-label': 'Remove this step', text: '✕', onclick: () => { p.thresholds.splice(i, 1); api.edited(); draw(); } })));
      });
    };
    draw();
    return wrap;
  }

  function slideshowPanel() {
    const files = Array.isArray(p.files) ? p.files : (p.files = []);
    const list = el('div', { class: 'slides' });
    const url = f => api.backend.mediaUrl(api.oid, f.split('/').map(encodeURIComponent).join('/'));
    const move = (i, d) => { const j = i + d; if (j < 0 || j >= files.length) return; [files[i], files[j]] = [files[j], files[i]]; api.edited(); api.relabel(); draw(); };
    const draw = () => {
      list.textContent = '';
      if (!files.length) list.append(el('p', { class: 'muted small', text: 'No images yet.' }));
      files.forEach((f, i) => {
        const img = el('img', { class: 'slide-thumb', alt: '', loading: 'lazy', src: url(f) });
        list.append(el('div', { class: 'slide-row' }, img, el('span', { class: 'lname small', text: f.split('/').pop(), title: f.split('/').pop() }),
          el('button', { type: 'button', class: 'iconbtn', title: 'Move up', 'aria-label': 'Move up', text: '↑', disabled: i === 0, onclick: () => move(i, -1) }),
          el('button', { type: 'button', class: 'iconbtn', title: 'Move down', 'aria-label': 'Move down', text: '↓', disabled: i === files.length - 1, onclick: () => move(i, 1) }),
          el('button', { type: 'button', class: 'iconbtn', title: 'Remove', 'aria-label': 'Remove', text: '✕', onclick: () => { files.splice(i, 1); api.edited(); api.relabel(); draw(); } })));
      });
    };
    draw();
    const pick = el('input', { type: 'file', multiple: true, accept: 'image/png,image/jpeg,image/webp,image/gif,image/bmp', hidden: true, onchange: async e => {
      const chosen = [...e.target.files]; e.target.value = '';
      for (const f of chosen) {
        if (files.length >= 30) { api.toast('At most 30 images.', true); break; }
        try { const r = await api.backend.themeAsset(api.oid, f); files.push(r.file); api.edited(); api.relabel(); draw(); }
        catch (err) { api.toast(err.message, true); }
      }
    } });
    host.append(list, el('div', { class: 'row' }, el('button', { type: 'button', class: 'btn secondary', text: 'Add images…', onclick: () => pick.click() }), pick),
      field('Seconds each', num('pSecs', 'seconds', { min: 1, max: 3600, placeholder: '8' }), 'pSecs'),
      field('Fade (s)', range('pFade', clamp(p.fade ?? 1, 0, 5), 0, 5, 0.1, v => api.set('fade', Math.round(v * 10) / 10), v => v.toFixed(1)), 'pFade'),
      field('Fit', select2('pFit', p.fit || 'cover', [['cover', 'Fill'], ['contain', 'Whole picture']], v => api.set('fit', v)), 'pFit'));
  }

  function infoPanel() {
    const kind = p.kind || 'uptime';
    host.append(field('Show', select2('pKind', kind, INFO_KINDS, v => { setStyle('kind', v); api.relabel(); }), 'pKind'));
    if (kind === 'countdown') {
      const dt = el('input', { type: 'datetime-local', id: 'pTarget', onchange: e => { if (e.target.value) api.set('target', e.target.value); } });
      dt.value = (p.target || '').slice(0, 16);
      const lab = el('input', { type: 'text', id: 'pCdLabel', maxLength: 40, spellcheck: false, placeholder: 'Countdown', oninput: e => api.set('label', e.target.value.trim() ? e.target.value : null) });
      lab.value = p.label || '';
      host.append(field('Date and time', dt, 'pTarget'), field('Label', lab, 'pCdLabel'));
    } else if (kind === 'topProcesses') {
      host.append(
        field('By', select2('pBy', p.by || 'cpu', [['cpu', 'CPU'], ['ram', 'Memory']], v => api.set('by', v)), 'pBy'),
        field('Count', select2('pCnt', String(clamp(p.count || 4, 3, 5)), [3, 4, 5].map(n => [String(n), String(n)]), v => api.set('count', Number(v))), 'pCnt'));
    }
    host.append(colour('color'));
  }
}

/** The 4.0 item types have their property panels in morewidgets.js. */
export const hasMorePanel = More.isMore;
