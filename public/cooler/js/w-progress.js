// 4.0 W1 "Progress" item: day / week / month / year progress bars. Registered by morewidgets.js. Follows docs/v4/ui-style-guide.md
// section 4: controls built from the helpers in `api.h` (field, select2, swRow) and `h.colour`; every change saves at once through
// api.set / h.restructure (no Save button); labels short, sentence case.
const PERIODS = [['day', 'Day'], ['week', 'Week'], ['month', 'Month'], ['year', 'Year']];
const ALL = PERIODS.map(p => p[0]);

export default {
  type: 'progress', key: 'progress', name: 'Progress', icon: '▦', group: 'Time',
  props: () => ({ items: [...ALL], labels: true, decimals: 0, color: '#5ac8ff', weekStart: 'windows' }),
  /** Default box [w, h] (fractions of the 2400x1080 canvas): taller with every bar. */
  size: p => {
    const n = Math.min(4, Math.max(1, Array.isArray(p.items) && p.items.length ? p.items.length : 4));
    return [0.26, Math.max(0.16, Math.round((0.075 * n + 0.04) * 10000) / 10000)];
  },
  title: w => 'Progress',
  /** Append the Properties controls to `host`; return true. `h`: { colour(key, text, dflt, resettable), restructure(fn), clamp, rateField() }. */
  panel(w, host, api, h) {
    const { el } = api;
    const { field, select2, swRow } = api.h;
    const p = w.props;
    const items = () => (Array.isArray(p.items) ? p.items.filter(i => ALL.includes(i)) : []);
    if (!items().length) p.items = [...ALL];

    const box = el('div', { class: 'checks', role: 'group', 'aria-label': 'What to show' });
    for (const [id, text] of PERIODS) {
      const c = el('input', { type: 'checkbox', checked: items().includes(id), onchange: e => {
        const cur = items().filter(x => x !== id);
        if (e.target.checked) cur.push(id);
        if (!cur.length) { e.target.checked = true; return; } // at least one bar stays
        h.restructure(() => { p.items = ALL.filter(i => cur.includes(i)); });
      } });
      c.checked = items().includes(id);
      box.append(el('label', {}, c, text));
    }
    host.append(
      el('p', { class: 'small muted', text: 'Show' }), box,
      swRow('Labels', p.labels !== false, v => api.set('labels', v)),
      field('Decimals', select2('pDec', String(h.clamp(Number(p.decimals) || 0, 0, 2)), [['0', '0'], ['1', '1'], ['2', '2']], v => api.set('decimals', Number(v))), 'pDec'),
      h.colour('color', 'Colour', '#5ac8ff'),
      field('Week starts', select2('pWeek', p.weekStart || 'windows', [['windows', 'Windows setting'], ['monday', 'Monday'], ['sunday', 'Sunday']], v => api.set('weekStart', v)), 'pWeek'));
    return true;
  },
};
