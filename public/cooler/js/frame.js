// Geometry shared by every screen-shaped preview. Mirrors src/convert.js exactly (frameGraph, bandPx) so the instant
// preview shows the same crop, position and notch as the converted version. Units: the 2400x1080 landscape canvas.
export const W = 2400, H = 1080;

const trunc2 = v => 2 * Math.trunc(v / 2);

/** Zoom at which the whole picture is visible (black bars), relative to "cover" (= 1). */
export function fitZoom(w, h) {
  if (!w || !h) return 1;
  return Math.min(W / w, H / h) / Math.max(W / w, H / h);
}

/** Where the source picture lands on the canvas: { left, top, width, height } in canvas pixels. */
export function frameGeometry(w, h, f) {
  const z = f && Number.isFinite(f.zoom) ? f.zoom : 1;
  const fx = f && Number.isFinite(f.x) ? f.x : 0.5, fy = f && Number.isFinite(f.y) ? f.y : 0.5;
  const S = z * Math.max(W / w, H / h);
  const dw = trunc2(w * S), dh = trunc2(h * S);
  // ffmpeg: pad (picture smaller than the screen) then crop (picture larger); both offsets are rounded down to even.
  const left = dw < W ? trunc2((W - dw) * fx) : -trunc2((dw - W) * fx);
  const top = dh < H ? trunc2((H - dh) * fy) : -trunc2((dh - H) * fy);
  return { left, top, width: dw, height: dh };
}

/** Same rounding as src/convert.js bandPx(): the band edge lands on the 16 px MJPEG block grid. */
export function bandPx(band) {
  const px = Math.round(band.px);
  if (px <= 0) return 0;
  const offset = band.edge === 'top' ? (1080 % 16) : 0;
  return Math.ceil((px - offset) / 16) * 16 + offset;
}

/** Style the notch overlay element (absolute, inside a 20:9 frame) for a mask; hides it when the mask is off. */
export function styleNotch(el, mask) {
  const pct = (v, of) => (v / of) * 100 + '%';
  el.style.cssText = '';
  el.hidden = true;
  if (!mask || mask.mode === 'off') return;
  if (mask.mode === 'band') {
    const px = bandPx(mask.band);
    if (!px) return;
    const e = mask.band.edge;
    Object.assign(el.style, {
      left: e === 'right' ? pct(W - px, W) : '0', top: e === 'bottom' ? pct(H - px, H) : '0',
      width: e === 'left' || e === 'right' ? pct(px, W) : '100%', height: e === 'top' || e === 'bottom' ? pct(px, H) : '100%',
    });
    el.className = 'notch band';
  } else {
    const { cx, cy, r, feather } = mask.circle;
    if (r <= 0) return;
    const R = r + feather;
    Object.assign(el.style, { left: pct(cx - R, W), top: pct(cy - R, H), width: pct(2 * R, W), height: pct(2 * R, H) });
    el.style.background = feather > 0
      ? `radial-gradient(circle closest-side, #000 ${(r / R) * 100}%, rgba(0,0,0,0) 100%)` : '#000';
    el.className = 'notch circle';
  }
  el.hidden = false;
}

const SVG_NS = 'http://www.w3.org/2000/svg';
/** Path of the area outside a rounded rectangle covering the canvas (even-odd fill), radius r in canvas pixels. */
/** The visible picture: the canvas minus the notch band (same as src/convert.js visibleRect()). */
export function visibleRect(mask, fit) {
  // the panel inset by the screen fit (the shift does not move the bezel), then the notch band from the panel's own edge
  const f = fit || {};
  const r = { x0: Math.min(f.left || 0, W - 2), y0: Math.min(f.top || 0, H - 2), x1: W - (f.right || 0), y1: H - (f.bottom || 0) };
  if (mask && mask.mode === 'band') {
    const px = bandPx(mask.band);
    if (px > 0) ({ left: () => (r.x0 = Math.max(r.x0, px)), right: () => (r.x1 = Math.min(r.x1, W - px)), top: () => (r.y0 = Math.max(r.y0, px)), bottom: () => (r.y1 = Math.min(r.y1, H - px)) })[mask.band.edge]();
  }
  return r;
}

/** Path of everything outside a rounded rectangle (even-odd fill): the visible rectangle with corner radius r. */
export function cornersPath(r, rect = { x0: 0, y0: 0, x1: W, y1: H }) {
  const { x0, y0, x1, y1 } = rect;
  const R = Math.min(Math.round(r), Math.floor(Math.min(x1 - x0, y1 - y0) / 2));
  return `M0 0H${W}V${H}H0Z M${x0 + R} ${y0}H${x1 - R}A${R} ${R} 0 0 1 ${x1} ${y0 + R}V${y1 - R}A${R} ${R} 0 0 1 ${x1 - R} ${y1}` +
    `H${x0 + R}A${R} ${R} 0 0 1 ${x0} ${y1 - R}V${y0 + R}A${R} ${R} 0 0 1 ${x0 + R} ${y0}Z`;
}

/** An <svg> overlay (absolute, full frame) for the rounded-corner mask; update it with styleCorners(). */
export function makeCornersOverlay() {
  const svg = document.createElementNS(SVG_NS, 'svg');
  svg.setAttribute('viewBox', `0 0 ${W} ${H}`);
  svg.setAttribute('preserveAspectRatio', 'none');
  svg.setAttribute('aria-hidden', 'true');
  svg.setAttribute('class', 'corners');
  const p = document.createElementNS(SVG_NS, 'path');
  p.setAttribute('fill', '#000');
  p.setAttribute('fill-rule', 'evenodd');
  svg.append(p);
  svg.style.display = 'none';
  return svg;
}

/** Same shape as src/convert.js cornerMaskFile(): black outside the visible rectangle rounded with radius corners.r. */
export function styleCorners(svg, corners, notchMask, fit) {
  const on = !!(corners && corners.on && Math.round(corners.r) > 0);
  svg.style.display = on ? '' : 'none';
  if (on) svg.firstChild.setAttribute('d', cornersPath(corners.r, visibleRect(notchMask, fit)));
}
