// The app's one icon set (docs/v4/ui-style-guide.md): 24x24 stroke icons drawn in currentColor, shown at 16 px.
//   icon(name, size = 16)  -> SVG markup (aria-hidden), or '' for an unknown name
//   widgetIcon(keyOrType)  -> the icon of a palette entry or item type ('' when there is none: the caller keeps its glyph)
//   upgradeGlyphs()        -> icon-only buttons and warning badges that still carry a text glyph (↺ ✕ + ↑ ↓ ⋯ ⚠ ▶ ❚❚)
//                             get the matching SVG; their aria-label and title stay as they are.
const P = {
  clock: '<circle cx="12" cy="12" r="8.5"/><path d="M12 7.5V12l3 2"/>',
  date: '<rect x="3.5" y="5" width="17" height="15" rx="2"/><path d="M3.5 9.5h17M8 3v4M16 3v4"/>',
  sensor: '<path d="M4 15a8 8 0 1 1 16 0"/><path d="M12 15l4-5"/><circle cx="12" cy="15" r="1.2"/>',
  chip: '<rect x="6" y="6" width="12" height="12" rx="1.5"/><rect x="9.5" y="9.5" width="5" height="5"/><path d="M9 3v3M15 3v3M9 18v3M15 18v3M3 9h3M3 15h3M18 9h3M18 15h3"/>',
  gpu: '<rect x="2.5" y="6.5" width="19" height="11" rx="1.5"/><circle cx="9" cy="12" r="2.8"/><path d="M15 10h3.5M15 14h3.5M5 17.5v2.5"/>',
  ram: '<rect x="2.5" y="7" width="19" height="9" rx="1"/><path d="M6 10.5v2M9.5 10.5v2M13 10.5v2M16.5 10.5v2M6 16v2.5M18 16v2.5"/>',
  net: '<path d="M8 4v15M8 19l-3.5-3.5M8 19l3.5-3.5M16 20V5M16 5l-3.5 3.5M16 5l3.5 3.5"/>',
  disk: '<rect x="4" y="4" width="16" height="16" rx="2"/><circle cx="12" cy="11" r="3.5"/><path d="M7.5 17h2"/>',
  stats: '<path d="M4 6h16M4 12h16M4 18h10"/>',
  meters: '<path d="M4 6h9M4 12h13M4 18h6"/><path d="M17 6h3M19 12h1M14 18h6" opacity=".5"/>',
  cores: '<rect x="4" y="4" width="7" height="7" rx="1"/><rect x="13" y="4" width="7" height="7" rx="1"/><rect x="4" y="13" width="7" height="7" rx="1"/><rect x="13" y="13" width="7" height="7" rx="1"/>',
  lyrics: '<path d="M4 7h9M4 12h6M4 17h9"/><path d="M17 17V6.5l3-.7"/><circle cx="15" cy="17" r="2"/>',
  music: '<path d="M9 17.5V5.5l10-2v12"/><circle cx="6.5" cy="17.5" r="2.5"/><circle cx="16.5" cy="15.5" r="2.5"/>',
  visualizer: '<path d="M5 19v-6M9 19V8M13 19v-9M17 19V5M21 19v-4"/>',
  slideshow: '<rect x="3" y="6" width="14" height="12" rx="1.5"/><path d="M20 8v8"/><path d="m3 15 4-4 4 4 2-2 4 4"/>',
  image: '<rect x="3.5" y="4.5" width="17" height="15" rx="2"/><circle cx="9" cy="10" r="1.8"/><path d="m4 18 5.5-5.5 4 4 2.5-2.5 4.5 4.5"/>',
  weather: '<path d="M7 18.5h10a4 4 0 0 0 .5-8 6 6 0 0 0-11.5 1.5A3.3 3.3 0 0 0 7 18.5z"/>',
  uptime: '<path d="M12 20a8 8 0 1 0-8-8"/><path d="M4 7v5h5M12 8v4l2.5 2"/>',
  countdown: '<path d="M7 3h10M7 21h10M8 3c0 5 8 5 8 9s-8 4-8 9M16 3c0 5-8 5-8 9s8 4 8 9"/>',
  info: '<circle cx="12" cy="12" r="8.5"/><path d="M12 11v5.5M12 7.6v.2"/>',
  procs: '<path d="M9 6h11M9 12h11M9 18h11"/><circle cx="5" cy="6" r="1"/><circle cx="5" cy="12" r="1"/><circle cx="5" cy="18" r="1"/>',
  text: '<path d="M5 6V4.5h14V6M12 4.5v15M9 19.5h6"/>',
  worldclock: '<circle cx="12" cy="12" r="8.5"/><path d="M3.5 12h17M12 3.5c2.5 2.5 3.5 5.5 3.5 8.5s-1 6-3.5 8.5c-2.5-2.5-3.5-5.5-3.5-8.5s1-6 3.5-8.5z"/>',
  timer: '<circle cx="12" cy="13" r="7.5"/><path d="M12 13V9M10 2.5h4M18.5 6.5l1.5-1.5"/>',
  volume: '<path d="M4 9.5h3.5L12 5.5v13l-4.5-4H4z"/><path d="M15.5 9a4 4 0 0 1 0 6M18 6.5a7.5 7.5 0 0 1 0 11"/>',
  ping: '<path d="M4 17h3l2.5-7 3 11 3-14 2 10h2.5"/>',
  agenda: '<rect x="3.5" y="5" width="17" height="15" rx="2"/><path d="M3.5 9.5h17M8 3v4M16 3v4M7.5 13h4M7.5 16.5h9"/>',
  livetext: '<path d="M4 6V4.5h11V6M9.5 4.5v15M7 19.5h5"/><path d="M15.5 13.5h5M18 11v5" />',
  effect: '<path d="M12 3l1.8 4.7L18.5 9l-4.7 1.8L12 15.5l-1.8-4.7L5.5 9l4.7-1.3z"/><path d="M18 15l.8 2.2L21 18l-2.2.8L18 21l-.8-2.2L15 18l2.2-.8z"/>',
  group: '<rect x="3.5" y="3.5" width="17" height="17" rx="2" stroke-dasharray="3 2.5"/><rect x="7.5" y="7.5" width="9" height="9" rx="1"/>',
  ungroup: '<rect x="3.5" y="3.5" width="10" height="10" rx="1"/><rect x="10.5" y="10.5" width="10" height="10" rx="1" stroke-dasharray="3 2.5"/>',
  close: '<path d="M6 6l12 12M18 6 6 18"/>',
  trash: '<path d="M4 7h16M9.5 7V4.5h5V7M6.5 7l1 13h9l1-13M10 11v6M14 11v6"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  up: '<path d="M12 19V5M6 11l6-6 6 6"/>',
  down: '<path d="M12 5v14M6 13l6 6 6-6"/>',
  reset: '<path d="M20 11a8 8 0 0 0-14.3-4.5M4 4v4h4M4 13a8 8 0 0 0 14.3 4.5M20 20v-4h-4"/>',
  more: '<circle cx="6" cy="12" r="1.2" fill="currentColor"/><circle cx="12" cy="12" r="1.2" fill="currentColor"/><circle cx="18" cy="12" r="1.2" fill="currentColor"/>',
  warning: '<path d="M12 4 2.8 19.5h18.4z"/><path d="M12 10v4.5M12 17v.2"/>',
  play: '<path d="M8 5.5v13l10.5-6.5z" fill="currentColor"/>',
  pause: '<path d="M8 5v14M16 5v14" stroke-width="3"/>',
  back: '<path d="M15 5l-7 7 7 7"/>',
  chevronDown: '<path d="M6 9l6 6 6-6"/>',
  browse: '<circle cx="12" cy="12" r="8.5"/><path d="m15.5 8.5-2 5-5 2 2-5z"/>',
};
// palette keys and item types that share an icon
const ALIAS = { cpu: 'chip', cpuName: 'info', network: 'net', disks: 'disk', countdown: 'countdown' };

export function icon(name, size = 16) {
  const p = P[ALIAS[name] || name];
  if (!p) return '';
  return `<svg class="ico" viewBox="0 0 24 24" width="${size}" height="${size}" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" focusable="false">${p}</svg>`;
}

const WIDGET = { info: 'info', image: 'image', sensor: 'sensor' };
export const widgetIcon = keyOrType => icon(WIDGET[keyOrType] || keyOrType);

// text glyphs still used for icon-only buttons in older or stream-owned modules
const GLYPH = { '↺': 'reset', '✕': 'close', '+': 'plus', '↑': 'up', '↓': 'down', '⋯': 'more', '⚠': 'warning', '▶': 'play', '❚❚': 'pause' };
function upgrade(el) {
  if (!el || el.nodeType !== 1 || el.querySelector('svg')) return;
  const name = GLYPH[el.textContent.trim()];
  if (name) el.innerHTML = icon(name, el.matches('.wwarn, .lwarn') ? 14 : 16);
}
export function upgradeGlyphs(root = document.body) {
  const SEL = '.iconbtn, .icon-btn, .wwarn, .lwarn';
  root.querySelectorAll(SEL).forEach(upgrade);
  new MutationObserver(list => {
    for (const m of list) {
      if (m.type === 'characterData') { const p = m.target.parentElement; if (p && p.matches(SEL)) upgrade(p); continue; }
      if (m.target.nodeType === 1 && m.target.matches(SEL)) upgrade(m.target);
      for (const n of m.addedNodes) if (n.nodeType === 1) { if (n.matches(SEL)) upgrade(n); n.querySelectorAll && n.querySelectorAll(SEL).forEach(upgrade); }
    }
  }).observe(root, { childList: true, subtree: true, characterData: true });
}
