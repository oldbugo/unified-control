// A two-handle range slider for the curve band: the left handle sets how far the band reaches to the left of its centre,
// the right handle how far to the right; the fill between the handles is the band. Pointer and keyboard (arrows, Shift =
// big steps, Home/End). Values are px: { left, right }, each 0..span. The track covers -span..+span around the centre.
export function createBandSlider({ span = 600, step = 5, labelLeft = 'Curve left edge', labelRight = 'Curve right edge', onchange }) {
  const el = (cls, attrs = {}) => { const d = document.createElement('div'); d.className = cls; for (const [k, v] of Object.entries(attrs)) d.setAttribute(k, v); return d; };
  const root = el('div', { class: 'bs' });
  root.className = 'bs';
  const track = el('div'); track.className = 'bs-track';
  const fill = el('div'); fill.className = 'bs-fill';
  const mid = el('div'); mid.className = 'bs-mid';
  const hl = el('div', { role: 'slider', tabindex: '0', 'aria-label': labelLeft, 'aria-orientation': 'horizontal' }); hl.className = 'bs-handle bs-l';
  const hr = el('div', { role: 'slider', tabindex: '0', 'aria-label': labelRight, 'aria-orientation': 'horizontal' }); hr.className = 'bs-handle bs-r';
  const vl = document.createElement('span'); vl.className = 'bs-val bs-vl';
  const vr = document.createElement('span'); vr.className = 'bs-val bs-vr';
  track.append(fill, mid, hl, hr, vl, vr);
  root.append(track);
  const lim = { left: span, right: span }; // the most each handle can reach (the band stays on the canvas)
  const v = { left: 0, right: 0 };
  const pct = px => ((px + span) / (2 * span)) * 100;
  const snap = n => Math.round(n / step) * step;

  function paint() {
    const a = pct(-v.left), b = pct(v.right);
    fill.style.left = a + '%'; fill.style.width = (b - a) + '%';
    mid.style.left = pct(0) + '%';
    hl.style.left = a + "%"; hr.style.left = b + "%";
    vl.style.left = a + '%'; vr.style.left = b + '%';
    vl.textContent = String(v.left); vr.textContent = String(v.right);
    for (const [h, key] of [[hl, 'left'], [hr, 'right']]) {
      h.setAttribute('aria-valuemin', '0'); h.setAttribute('aria-valuemax', String(lim[key]));
      h.setAttribute('aria-valuenow', String(v[key])); h.setAttribute('aria-valuetext', `${v[key]} px`);
    }
  }
  function set(key, n, fire = true) {
    n = Math.min(lim[key], Math.max(0, snap(n)));
    if (n === v[key]) return;
    v[key] = n; paint();
    if (fire && onchange) onchange({ left: v.left, right: v.right });
  }
  function drag(h, key, ev) {
    if (ev.button !== undefined && ev.button !== 0) return;
    ev.preventDefault(); h.focus();
    const r = track.getBoundingClientRect();
    try { h.setPointerCapture(ev.pointerId); } catch { /* gone */ }
    const move = e => { const px = ((e.clientX - r.left) / r.width) * 2 * span - span; set(key, key === 'left' ? -px : px); };
    const up = () => { h.removeEventListener('pointermove', move); h.removeEventListener('pointerup', up); h.removeEventListener('pointercancel', up); };
    h.addEventListener('pointermove', move); h.addEventListener('pointerup', up); h.addEventListener('pointercancel', up);
    move(ev);
  }
  function keys(key, e) {
    const big = e.shiftKey ? step * 5 : step;
    // the arrows move the handle on screen: Right widens the right handle / narrows the left one
    const dir = e.key === 'ArrowRight' || e.key === 'ArrowUp' ? 1 : e.key === 'ArrowLeft' || e.key === 'ArrowDown' ? -1 : 0;
    if (dir) { set(key, v[key] + (key === 'right' ? dir : -dir) * big); e.preventDefault(); }
    else if (e.key === 'Home') { set(key, 0); e.preventDefault(); }
    else if (e.key === 'End') { set(key, lim[key]); e.preventDefault(); }
  }
  hl.addEventListener('pointerdown', e => drag(hl, 'left', e)); hr.addEventListener('pointerdown', e => drag(hr, 'right', e));
  hl.addEventListener('keydown', e => keys('left', e)); hr.addEventListener('keydown', e => keys('right', e));
  paint();
  return {
    root,
    /** Set both values without firing onchange; `limits` = { left, right } most each handle can reach. */
    set(left, right, limits) {
      lim.left = limits ? Math.min(span, limits.left) : span; lim.right = limits ? Math.min(span, limits.right) : span;
      v.left = Math.min(span, Math.max(0, Math.round(left))); v.right = Math.min(span, Math.max(0, Math.round(right)));
      v.left = Math.min(v.left, lim.left); v.right = Math.min(v.right, lim.right);
      paint();
    },
  };
}
