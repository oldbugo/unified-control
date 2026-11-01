// Extra Properties rows that apply to every item kind (plan "Editor refinement round"; E2 adds "Corner radius" here).
// A module registers a function once:
//   addItemControls((w, host, api) => { host.append(...) })
// It runs at the end of each item's Properties, above the pinned "Delete this item" bar. `api` is the same object the
// item panels get (el, h { field, select2, range, numInput, swRow }, set, edited, repaint, S, ...). Changes save at
// once through api.set / api.edited like every other control (no Save button).
export const itemControls = [];
export const addItemControls = fn => { if (typeof fn === 'function' && !itemControls.includes(fn)) itemControls.push(fn); };
