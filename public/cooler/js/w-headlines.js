// 4.0 W2 "Headlines" item: the latest titles of an RSS or Atom feed whose link the user pastes. Registered by morewidgets.js.
// Follows docs/v4/ui-style-guide.md section 4: helpers in `api.h` (field, select2, swRow) and `h.colour`; changes save at once
// through api.set / api.edited (no Save button); labels short, sentence case.

/** A usable feed link: http or https, a host, no spaces, no user:pass@ (the engine checks the same again). */
function feedLink(v) {
  const s = String(v || '').trim();
  if (!s || s.length > 500 || /\s/.test(s)) return null;
  let u;
  try { u = new URL(s); } catch { return null; }
  if (u.protocol !== 'http:' && u.protocol !== 'https:') return null;
  if (!u.hostname || u.username || u.password || /^[a-z]+:\/\/[^/?#]*@/i.test(s)) return null;
  return s;
}

export default {
  type: 'headlines', key: 'headlines', name: 'Headlines', icon: '▦', group: 'Info',
  props: () => ({ source: '', count: 4, showSource: true, color: '#ffffff' }),
  /** Default box [w, h] (fractions of the 2400x1080 canvas): taller with every headline. */
  size: p => {
    const n = Math.min(5, Math.max(3, Math.round(Number(p.count)) || 4));
    return [0.36, Math.min(1, Math.max(0.2, Math.round((0.1 + 0.07 * n) * 10000) / 10000))];
  },
  title: w => 'Headlines',
  /** Append the Properties controls to `host`; return true. `h`: { colour(key, text, dflt, resettable), restructure(fn), clamp, rateField() }. */
  panel(w, host, api, h) {
    const { el } = api;
    const { field, select2, swRow } = api.h;
    const p = w.props;
    const src = el('input', { type: 'text', id: 'pFeed', maxLength: 500, spellcheck: false, placeholder: 'https://…/feed.xml',
      onchange: e => {
        const ok = feedLink(e.target.value);
        p.source = ok === null && String(e.target.value).trim() ? (feedLink(p.source) || '') : (ok || '');
        e.target.value = p.source;
        api.edited();
      } });
    src.value = feedLink(p.source) || '';
    host.append(
      field('Feed link', src, 'pFeed'),
      el('p', { class: 'muted small', text: 'Fetches this link about every 15 minutes while the item is shown. Nothing else is sent.' }),
      field('Headlines', select2('pHeads', String(h.clamp(Math.round(Number(p.count)) || 4, 3, 5)), [3, 4, 5].map(n => [String(n), String(n)]), v => h.restructure(() => { p.count = Number(v); })), 'pHeads'),
      swRow('Source and age', p.showSource !== false, v => api.set('showSource', v)),
      h.colour('color', 'Colour', '#ffffff'));
    return true;
  },
};
