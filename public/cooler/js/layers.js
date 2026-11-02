// Pure helpers for the theme editor (no DOM): layer order and groups, screen guides, snapping.
// `theme.widgets` is the drawing order (first = bottom, last = top); the Layers panel lists it top-first.
// Group members are contiguous in `widgets`; `theme.groups` = [{ id, name, hidden, collapsed }].

export const GW = 2400, GH = 1080;
const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));

/** Members contiguous (a group sits where its lowest member is), empty groups removed, unknown group ids dropped. */
export function normalizeGroups(t) {
  const known = new Set((t.groups || []).map(g => g.id));
  for (const w of t.widgets) if (w.group && !known.has(w.group)) w.group = null;
  const out = [], done = new Set();
  for (const w of t.widgets) {
    if (!w.group) out.push(w);
    else if (!done.has(w.group)) { done.add(w.group); out.push(...t.widgets.filter(x => x.group === w.group)); }
  }
  t.widgets = out;
  const used = new Set(out.map(w => w.group).filter(Boolean));
  t.groups = (t.groups || []).filter(g => used.has(g.id));
  return t;
}

export const membersOf = (t, gid) => t.widgets.filter(w => w.group === gid);

/** The rows of the Layers panel, top of the drawing order first: groups with their members (unless collapsed). */
export function layerRows(t) {
  const rows = [], seen = new Set();
  for (let i = t.widgets.length - 1; i >= 0; i--) {
    const w = t.widgets[i];
    if (!w.group) { rows.push({ kind: 'layer', w, depth: 0 }); continue; }
    if (seen.has(w.group)) continue;
    seen.add(w.group);
    const g = t.groups.find(x => x.id === w.group);
    if (!g) { rows.push({ kind: 'layer', w, depth: 0 }); continue; }
    const mem = membersOf(t, g.id).reverse();
    rows.push({ kind: 'group', g, members: mem });
    if (!g.collapsed) for (const m of mem) rows.push({ kind: 'layer', w: m, depth: 1 });
  }
  return rows;
}

/** Ids of the layers in panel order (top first); a collapsed group's members are still listed (they are selected with it). */
export const orderTopFirst = t => [...t.widgets].reverse().map(w => w.id);

/**
 * Move layers `ids` next to `target` ({ kind: 'layer', id } or { kind: 'group', id }); pos 'above' = drawn on top of it
 * (later in `widgets`), 'below' = under it. A layer dropped on a layer joins that layer's group; dropped next to a group
 * header it goes outside, or into the group's top when dropped on its lower half ('into'). A whole group moves as a block.
 */
export function moveLayers(t, ids, target, pos) {
  const idset = new Set(ids);
  const moving = t.widgets.filter(w => idset.has(w.id));
  if (!moving.length) return t;
  // a dragged group (every member of exactly one group is moving) keeps its membership
  const gids = [...new Set(moving.map(w => w.group))];
  const wholeGroup = gids.length === 1 && gids[0] && membersOf(t, gids[0]).every(w => idset.has(w.id));
  const rest = t.widgets.filter(w => !idset.has(w.id));
  let at, newGroup = null;
  const lastOf = g => Math.max(...rest.map((w, i) => (w.group === g ? i : -1)));
  const firstOf = g => rest.findIndex(w => w.group === g);
  if (target.kind === 'layer') {
    const tw = rest.find(w => w.id === target.id);
    if (!tw) return t;
    const idx = rest.indexOf(tw);
    if (wholeGroup && tw.group) { at = pos === 'above' ? lastOf(tw.group) + 1 : firstOf(tw.group); }
    else { at = pos === 'above' ? idx + 1 : idx; newGroup = tw.group || null; }
  } else {
    const g = target.id;
    if (!rest.some(w => w.group === g)) return t;
    if (pos === 'into' && !wholeGroup) { at = lastOf(g) + 1; newGroup = g; }
    else { at = pos === 'above' || pos === 'into' ? lastOf(g) + 1 : firstOf(g); }
  }
  if (!wholeGroup) for (const w of moving) w.group = newGroup;
  rest.splice(at, 0, ...moving);
  t.widgets = rest;
  return normalizeGroups(t);
}

/** Group the layers `ids` (contiguous at the place of the topmost one). Returns the new group's id. */
export function groupLayers(t, ids) {
  const idset = new Set(ids);
  const moving = t.widgets.filter(w => idset.has(w.id));
  if (!moving.length) return null;
  const used = new Set(t.groups.map(g => g.id));
  let n = 1; while (used.has('g' + n)) n++;
  const id = 'g' + n;
  let k = 1; const names = new Set(t.groups.map(g => g.name));
  while (names.has('Group ' + k)) k++;
  t.groups.push({ id, name: 'Group ' + k, hidden: false, collapsed: false });
  const topIdx = Math.max(...moving.map(w => t.widgets.indexOf(w)));
  const rest = t.widgets.filter(w => !idset.has(w.id));
  const before = t.widgets.slice(0, topIdx + 1).filter(w => !idset.has(w.id)).length;
  for (const w of moving) w.group = id;
  rest.splice(before, 0, ...moving);
  t.widgets = rest;
  normalizeGroups(t);
  return id;
}

/** Dissolve the groups of the given layers/groups. */
export function ungroupLayers(t, ids, groupIds = []) {
  const gs = new Set(groupIds);
  for (const w of t.widgets) if (ids.includes(w.id) && w.group) gs.add(w.group);
  for (const w of t.widgets) if (gs.has(w.group)) w.group = null;
  t.groups = t.groups.filter(g => !gs.has(g.id));
  return normalizeGroups(t);
}

/** One step towards the top (dir +1, "bring forward") or the bottom (-1). */
export function stepLayers(t, ids, dir) {
  const idset = new Set(ids);
  const ws = t.widgets;
  const order = dir > 0 ? [...ws.keys()].reverse() : [...ws.keys()];
  for (const i of order) {
    const w = ws[i];
    if (!idset.has(w.id)) continue;
    let j = i + dir;
    if (j < 0 || j >= ws.length) continue;
    const nb = ws[j];
    if (idset.has(nb.id)) continue; // a neighbour that moves too: the block moves as one
    if (nb.group !== w.group) {
      if (w.group) continue; // at the edge of its group
      // an ungrouped layer hops over a whole neighbouring group
      while (j + dir >= 0 && j + dir < ws.length && ws[j + dir].group === nb.group) j += dir;
    }
    ws.splice(i, 1); ws.splice(j, 0, w);
  }
  return normalizeGroups(t);
}

// ---------------------------------------------------------------------------- screen guides (v1.4)
// { left: { size }, right: { size }, curve: { offset, left, right } } in px on the 2400x1080 canvas.

export const DEFAULT_GAP = 24;
const num = v => (Number.isFinite(Number(v)) ? Number(v) : 0);

/** Clamp one set of guides like the engine does (sides 0..half; the curve band stays inside the canvas). */
export function clampGuides(g) {
  g = g || {};
  const c = g.curve || {};
  const offset = clamp(num(c.offset), -GW / 2, GW / 2), centre = GW / 2 + offset;
  return {
    left: { size: clamp(num((g.left || {}).size), 0, GW / 2) },
    right: { size: clamp(num((g.right || {}).size), 0, GW / 2) },
    curve: { offset, left: clamp(num(c.left), 0, centre), right: clamp(num(c.right), 0, GW - centre) },
  };
}

/** Global guides + a theme's offsets (same keys, deltas), clamped. */
export function effectiveGuides(g, off) {
  g = g || {}; off = off || {};
  const a = (k, f) => num((g[k] || {})[f]) + num((off[k] || {})[f]);
  return clampGuides({
    left: { size: a('left', 'size') }, right: { size: a('right', 'size') },
    curve: { offset: a('curve', 'offset'), left: a('curve', 'left'), right: a('curve', 'right') },
  });
}

/** The curve band's left and right edge, px. */
export const curveEdges = g => { const c = GW / 2 + g.curve.offset; return [c - g.curve.left, c + g.curve.right]; };

/** Normalised (0..1) rectangles of the guide bands that have a width: { key: 'left'|'right'|'curve', name, x0, y0, x1, y1 }. */
export function bandRects(g) {
  const out = [];
  if (g.left.size > 0) out.push({ key: 'left', name: 'Left', x0: 0, y0: 0, x1: g.left.size / GW, y1: 1 });
  if (g.right.size > 0) out.push({ key: 'right', name: 'Right', x0: 1 - g.right.size / GW, y0: 0, x1: 1, y1: 1 });
  const [a, b] = curveEdges(g);
  if (b - a > 0) out.push({ key: 'curve', name: 'Curve', x0: a / GW, y0: 0, x1: b / GW, y1: 1 });
  return out;
}

/** Does the box overlap a band? -> the band's name (or null). */
export function bandHit(w, g) {
  for (const z of bandRects(g)) {
    if (w.x < z.x1 - 1e-6 && w.x + w.w > z.x0 + 1e-6 && w.y < z.y1 - 1e-6 && w.y + w.h > z.y0 + 1e-6) return z.name;
  }
  return null;
}

// ---------------------------------------------------------------------------- snapping

/**
 * Vertical (x) and horizontal (y) snap lines in 0..1. Each line is { v, k }: k 'a' = any side of a box may snap to it,
 * 's' = only a box's start edge (left/top), 'e' = only its end edge (right/bottom).
 * Exact alignment: canvas edges and centre, other boxes' edges and centres, guide band edges.
 * Gap (px on the 2400x1080 canvas, 0 = none): inside the canvas edges, outside each band, and between boxes.
 */
export function snapLines(others, guides, gap = 0, insets = null) {
  // `insets` (px, { left, right, top, bottom }): the notch band and screen fit. Lines start from the visible area, not the panel edge:
  // a side with an inset has no line at the canvas edge (nothing snaps under the band), its edge line is the band's edge.
  const ins = { left: 0, right: 0, top: 0, bottom: 0, ...(insets || {}) };
  const ax = [ins.left / GW, 1 - ins.right / GW], ay = [ins.top / GH, 1 - ins.bottom / GH];
  const xs = [{ v: ax[0], k: 'a' }, { v: (ax[0] + ax[1]) / 2, k: 'a' }, { v: ax[1], k: 'a' }], ys = [{ v: ay[0], k: 'a' }, { v: (ay[0] + ay[1]) / 2, k: 'a' }, { v: ay[1], k: 'a' }];
  const bands = bandRects(guides || { left: { size: 0 }, right: { size: 0 }, curve: { offset: 0, left: 0, right: 0 } });
  for (const b of others) {
    xs.push({ v: b.x, k: 'a' }, { v: b.x + b.w / 2, k: 'a' }, { v: b.x + b.w, k: 'a' });
    ys.push({ v: b.y, k: 'a' }, { v: b.y + b.h / 2, k: 'a' }, { v: b.y + b.h, k: 'a' });
  }
  for (const r of bands) xs.push({ v: r.x0, k: 'a' }, { v: r.x1, k: 'a' });
  gap = Math.max(0, num(gap));
  if (gap > 0) {
    const gx = gap / GW, gy = gap / GH;
    xs.push({ v: ax[0] + gx, k: 's' }, { v: ax[1] - gx, k: 'e' });
    ys.push({ v: ay[0] + gy, k: 's' }, { v: ay[1] - gy, k: 'e' });
    for (const r of bands) {
      if (r.x1 + gx < 1) xs.push({ v: r.x1 + gx, k: 's' });
      if (r.x0 - gx > 0) xs.push({ v: r.x0 - gx, k: 'e' });
    }
    for (const b of others) {
      xs.push({ v: b.x + b.w + gx, k: 's' }, { v: b.x - gx, k: 'e' });
      ys.push({ v: b.y + b.h + gy, k: 's' }, { v: b.y - gy, k: 'e' });
    }
  }
  return { xs, ys };
}

// cands: [{ v, side: 's'|'c'|'e' }]; a line with k 's'/'e' only takes that side.
function nearest(cands, lines, thr) {
  let best = null;
  for (const c of cands) for (const l of lines) {
    if (l.k !== 'a' && l.k !== c.side) continue;
    const d = l.v - c.v;
    if (Math.abs(d) <= thr && (!best || Math.abs(d) < Math.abs(best.d) - 1e-12)) best = { d, line: l.v, ...c };
  }
  return best;
}

/**
 * Snap a moving box (x, y, w, h): its left/centre/right and top/middle/bottom to the lines. Returns the corrected
 * { x, y } and the lines that were hit ({ vx: [...], hy: [...] }) for drawing.
 */
export function snapMove(b, L, thrX, thrY) {
  const sx = nearest([{ v: b.x, side: 's' }, { v: b.x + b.w / 2, side: 'c' }, { v: b.x + b.w, side: 'e' }], L.xs, thrX);
  const sy = nearest([{ v: b.y, side: 's' }, { v: b.y + b.h / 2, side: 'c' }, { v: b.y + b.h, side: 'e' }], L.ys, thrY);
  return { x: b.x + (sx ? sx.d : 0), y: b.y + (sy ? sy.d : 0), vx: sx ? [sx.line] : [], hy: sy ? [sy.line] : [] };
}

/**
 * Snap a resize: only the edges being dragged (`mode` letters n/s/e/w) go to lines; the new width/height also snaps to the
 * width/height of other boxes. `r` = { x, y, w, h } as computed by the pointer; returns the corrected rect + lines hit.
 */
export function snapResize(r, mode, L, others, thrX, thrY) {
  let { x, y, w, h } = r; const vx = [], hy = [];
  if (mode.includes('e')) { const s = nearest([{ v: x + w, side: 'e' }], L.xs, thrX); if (s) { w += s.d; vx.push(s.line); } }
  if (mode.includes('w')) { const s = nearest([{ v: x, side: 's' }], L.xs, thrX); if (s) { x += s.d; w -= s.d; vx.push(s.line); } }
  if (mode.includes('s')) { const s = nearest([{ v: y + h, side: 'e' }], L.ys, thrY); if (s) { h += s.d; hy.push(s.line); } }
  if (mode.includes('n')) { const s = nearest([{ v: y, side: 's' }], L.ys, thrY); if (s) { y += s.d; h -= s.d; hy.push(s.line); } }
  // matching sizes (only when no edge snapped on that axis)
  const sizes = (key, thr, v) => nearest([{ v, side: 'a' }], others.map(o => ({ v: o[key], k: 'a' })), thr);
  if ((mode.includes('e') || mode.includes('w')) && !vx.length) {
    const s = sizes('w', thrX, w);
    if (s) { if (mode.includes('w')) x -= s.d; w += s.d; }
  }
  if ((mode.includes('n') || mode.includes('s')) && !hy.length) {
    const s = sizes('h', thrY, h);
    if (s) { if (mode.includes('n')) y -= s.d; h += s.d; }
  }
  return { x, y, w, h, vx, hy };
}
