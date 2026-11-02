// The On screen editor's frame (plan "Editor refinement round", E1): resizable side panels, the palette / Layers divider,
// palette accordions and the deselect rule. Pure UI: nothing here touches the theme or the screen, and every size and
// open/closed state is remembered per viewer at once (no Save button).
//
// Resizing follows the vault pattern "Panel width snapping on release": the size follows the drag continuously, a small
// resisted overshoot past the bounds, and on release it eases to the nearest preset.

const ls = {
  get(k) { try { return localStorage.getItem(k); } catch { return null; } },
  set(k, v) { try { localStorage.setItem(k, v); } catch { /* not remembered */ } },
};
const reduced = () => { try { return matchMedia('(prefers-reduced-motion: reduce)').matches; } catch { return false; } };
const OVERSHOOT = 36;      // px the size may stretch past a bound while dragging
const RESIST = 0.3;        // how much of the pointer's travel past a bound shows
const SETTLE_MS = 220;

/** Size past a bound shows as a short, resisted stretch. */
function resist(v, min, max) {
  if (v < min) return min - Math.min(OVERSHOOT, (min - v) * RESIST);
  if (v > max) return max + Math.min(OVERSHOOT, (v - max) * RESIST);
  return v;
}
const nearest = (v, list) => list.reduce((a, b) => (Math.abs(b - v) < Math.abs(a - v) ? b : a));
function animate(from, to, apply, done) {
  if (reduced() || Math.abs(to - from) < 0.5) { apply(to); done && done(); return; }
  const t0 = performance.now();
  const step = now => {
    const t = Math.min(1, (now - t0) / SETTLE_MS);
    const e = 1 - Math.pow(1 - t, 3); // ease-out
    apply(from + (to - from) * e);
    if (t < 1) requestAnimationFrame(step); else done && done();
  };
  requestAnimationFrame(step);
}

/**
 * A drag handle that resizes something.
 *   handle: the element to drag; axis 'x' or 'y'; sign: +1 if moving the pointer right/down makes it bigger, -1 if smaller
 *   get(): current size (px); set(px): show a size; bounds(): [min, max]; presets(): px list; save(px)
 *   label: accessible name. Arrow keys step through the presets; Home/End go to the ends.
 */
export function makeResizer({ handle, axis, sign, get, set, bounds, presets, save, label }) {
  handle.setAttribute('role', 'separator');
  handle.setAttribute('aria-orientation', axis === 'x' ? 'vertical' : 'horizontal');
  handle.setAttribute('aria-label', label);
  handle.tabIndex = 0;
  const settle = (from, to) => { animate(from, to, set, () => { save(to); aria(); }); };
  const aria = () => { const [mn, mx] = bounds(); handle.setAttribute('aria-valuemin', String(Math.round(mn))); handle.setAttribute('aria-valuemax', String(Math.round(mx))); handle.setAttribute('aria-valuenow', String(Math.round(get()))); };
  handle.addEventListener('pointerdown', ev => {
    if (ev.button !== 0) return;
    ev.preventDefault(); ev.stopPropagation();
    const start = axis === 'x' ? ev.clientX : ev.clientY;
    const base = get();
    let last = base;
    handle.setPointerCapture(ev.pointerId);
    handle.classList.add('dragging');
    document.documentElement.classList.add(axis === 'x' ? 'resizing-x' : 'resizing-y');
    const move = e => {
      const [mn, mx] = bounds();
      const raw = base + sign * ((axis === 'x' ? e.clientX : e.clientY) - start);
      last = resist(raw, mn, mx);
      set(last);
    };
    const up = () => {
      handle.removeEventListener('pointermove', move); handle.removeEventListener('pointerup', up); handle.removeEventListener('pointercancel', up);
      handle.classList.remove('dragging');
      document.documentElement.classList.remove('resizing-x', 'resizing-y');
      const [mn, mx] = bounds();
      const inside = Math.min(mx, Math.max(mn, last));
      const ps = presets().filter(p => p >= mn && p <= mx);
      settle(last, ps.length ? nearest(inside, ps) : inside);
    };
    handle.addEventListener('pointermove', move); handle.addEventListener('pointerup', up); handle.addEventListener('pointercancel', up);
  });
  handle.addEventListener('keydown', ev => {
    const [mn, mx] = bounds();
    const ps = presets().filter(p => p >= mn && p <= mx).sort((a, b) => a - b);
    if (!ps.length) return;
    const cur = get();
    const grow = (axis === 'x' ? { ArrowRight: 1, ArrowLeft: -1 } : { ArrowDown: 1, ArrowUp: -1 })[ev.key];
    let to = null;
    if (grow) { const d = grow * sign; to = d > 0 ? (ps.find(p => p > cur + 1) ?? ps[ps.length - 1]) : ([...ps].reverse().find(p => p < cur - 1) ?? ps[0]); }
    else if (ev.key === 'Home') to = ps[0]; else if (ev.key === 'End') to = ps[ps.length - 1];
    if (to == null) return;
    ev.preventDefault(); ev.stopPropagation();
    settle(cur, to);
  });
  aria();
  return { aria };
}

/**
 * The editor frame. ed: the .ed grid; left/right: the side panels; split: the divider between the palette and Layers;
 * layersBox: the Layers section inside the left panel.
 */
export function initEditorShell({ ed, left, right, split, layersBox }) {
  const px = v => Math.round(v) + 'px';
  // ---- side panels (widths in px, on the grid as --ed-left-w / --ed-right-w)
  const sides = [
    { el: left, varName: '--ed-left-w', key: 'cs.ed.leftW', presets: [220, 300, 380], dflt: 300, sign: 1, label: 'Resize the left panel' },
    { el: right, varName: '--ed-right-w', key: 'cs.ed.rightW', presets: [280, 360, 480], dflt: 280, sign: -1, label: 'Resize the properties panel' },
  ];
  for (const s of sides) {
    const saved = Number(ls.get(s.key));
    let w = Number.isFinite(saved) && saved >= 200 && saved <= 560 ? saved : s.dflt;
    const set = v => { w = v; ed.style.setProperty(s.varName, px(v)); };
    set(w);
    const handle = document.createElement('div');
    handle.className = 'ed-resizer ' + (s.sign > 0 ? 'at-right' : 'at-left');
    s.el.append(handle);
    makeResizer({
      handle, axis: 'x', sign: s.sign, label: s.label,
      get: () => w, set,
      // the preview keeps at least 360 px between the panels
      bounds: () => { const other = sides.find(o => o !== s); const room = ed.clientWidth - 360 - (other.el.getBoundingClientRect().width || 0); return [200, Math.max(200, Math.min(560, room))]; },
      presets: () => s.presets,
      save: v => ls.set(s.key, String(Math.round(v))),
    });
  }
  // ---- palette / Layers divider (Layers height as a share of the left panel, so it follows the window height)
  const savedF = Number(ls.get('cs.ed.layersF'));
  let f = Number.isFinite(savedF) && savedF > 0.1 && savedF < 0.9 ? savedF : 0.4;
  const colH = () => left.clientHeight || 600;
  const setH = h => { f = h / colH(); left.style.setProperty('--ed-layers-h', px(h)); };
  const sync = () => left.style.setProperty('--ed-layers-h', px(f * colH()));
  sync();
  new ResizeObserver(sync).observe(left);
  makeResizer({
    handle: split, axis: 'y', sign: -1, label: 'Resize the layer list',
    get: () => layersBox.getBoundingClientRect().height, set: setH,
    bounds: () => [120, Math.max(120, colH() - 200)],
    presets: () => [0.3, 0.45, 0.6].map(x => x * colH()),
    save: () => ls.set('cs.ed.layersF', f.toFixed(3)),
  });
}

/** Palette group as an accordion: several open at once, remembered, all open the first time. */
export function makeAccordion(head, body, group) {
  const key = 'cs.ed.palClosed';
  const closed = () => { try { return new Set(JSON.parse(ls.get(key) || '[]')); } catch { return new Set(); } };
  const btn = document.createElement('button');
  btn.type = 'button';
  btn.className = 'pal-toggle';
  btn.innerHTML = '<svg class="ico chev" viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M6 9l6 6 6-6"/></svg>';
  btn.append(document.createTextNode(group));
  const id = 'pal-' + group.toLowerCase().replace(/\W+/g, '-');
  body.id = id;
  btn.setAttribute('aria-controls', id);
  const show = open => { btn.setAttribute('aria-expanded', String(open)); body.hidden = !open; head.classList.toggle('closed', !open); };
  show(!closed().has(group));
  btn.onclick = () => {
    const c = closed();
    const open = c.has(group);
    if (open) c.delete(group); else c.add(group);
    ls.set(key, JSON.stringify([...c]));
    show(open);
  };
  head.textContent = '';
  head.append(btn);
}

/**
 * Deselect rule (user, 2026-10-19): a click anywhere outside the selected item and outside the right-hand panel
 * deselects. Kept: the right panel, the items on the preview, the layer rows and their tools, dialogs and the
 * Settings drawer, the panel resize handles, and moving the window by its title bar (only a click there deselects).
 */
export function installDeselect({ root, hasSelection, deselect }) {
  const KEEP = '.ed-right, .wbox, .lrow, .layers-head, .ed-resizer, .ed-split, .metric-pick, .backdrop, .drawer-back, .apply-back, #cs-titlebar, .toast, .pal';
  document.addEventListener('pointerdown', ev => {
    if (root.hidden || ev.button !== 0 || !hasSelection()) return;
    const t = ev.target;
    if (!(t instanceof Element) || t.closest(KEEP)) return;
    // the title bar also moves the window: deselect only on a click there (no movement)
    if (t.closest('[data-tauri-drag-region]')) {
      const x = ev.clientX, y = ev.clientY;
      const up = e => { document.removeEventListener('pointerup', up, true); if (Math.hypot(e.clientX - x, e.clientY - y) < 4 && hasSelection()) deselect(); };
      document.addEventListener('pointerup', up, true);
      return;
    }
    deselect();
  }, true);
}
