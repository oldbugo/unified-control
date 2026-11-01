// 4.0 E2: the part of the canvas that is actually visible on the screen. The notch band (black, measured from the panel's own
// edge) and the screen fit (px the bezel hides at each edge; the design canvas is scaled into the visible rectangle) decide it.
// Gaps, snap lines and the default place of new items start from this area, so nothing is put under the band.
// Units: the 2400x1080 design canvas (px) unless a name says "normalised" (0..1).
import { bandPx, W, H } from './frame.js';

/** How far in from each design-canvas edge the visible, unmasked area starts: { left, right, top, bottom } px (all 0 when
 *  there is no band). `app`: the app state ({ mask, screenFit }). */
export function insets(app) {
  const out = { left: 0, right: 0, top: 0, bottom: 0 };
  const mask = app && app.mask;
  if (!mask || mask.mode !== 'band' || !mask.band) return out;
  const px = bandPx(mask.band);
  const edge = mask.band.edge;
  if (!px || !(edge in out)) return out;
  const f = (app && app.screenFit) || {};
  const hidden = k => Math.max(0, Number(f[k]) || 0);
  // visible panel rectangle; design x maps to panel x = hidden.left + x * visibleW / W (same for y)
  const vw = W - hidden('left') - hidden('right'), vh = H - hidden('top') - hidden('bottom');
  if (vw <= 0 || vh <= 0) return out;
  // the band covers the panel from its own edge: how much of it lies inside the visible rectangle, in design px
  const inside = ({ left: px - hidden('left'), right: px - hidden('right'), top: px - hidden('top'), bottom: px - hidden('bottom') })[edge];
  const scale = (edge === 'left' || edge === 'right') ? W / vw : H / vh;
  out[edge] = Math.max(0, Math.round(inside * scale));
  return out;
}

/** The safe rectangle, normalised: the visible area inset by `gap` px on every side. */
export function safeRect(app, gap = 0) {
  const i = insets(app);
  const g = Math.max(0, Number(gap) || 0);
  const x0 = (i.left + g) / W, y0 = (i.top + g) / H, x1 = 1 - (i.right + g) / W, y1 = 1 - (i.bottom + g) / H;
  return { x0: Math.min(x0, 0.5), y0: Math.min(y0, 0.5), x1: Math.max(x1, 0.5), y1: Math.max(y1, 0.5) };
}

/** Move a box (normalised x, y, w, h) into the safe rectangle: shifted when it fits, otherwise also shrunk. Returns the box. */
export function placeInside(box, app, gap = 0, { shrink = false } = {}) {
  const r = safeRect(app, gap);
  if (shrink) { box.w = Math.min(box.w, r.x1 - r.x0); box.h = Math.min(box.h, r.y1 - r.y0); }
  box.x = Math.min(Math.max(box.x, r.x0), Math.max(r.x0, r.x1 - box.w));
  box.y = Math.min(Math.max(box.y, r.y0), Math.max(r.y0, r.y1 - box.h));
  return box;
}
