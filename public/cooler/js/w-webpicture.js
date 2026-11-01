// 4.0 W2 "Web picture" item. Registered by morewidgets.js. One picture from a link the user pasted, fetched again every N minutes while
// the item is shown. Follows docs/v4/ui-style-guide.md section 4: changes save at once through api.set / api.edited (no Save button);
// labels short, sentence case. The Corner radius row is added by the editor itself.
const MIN_MINUTES = 1, MAX_MINUTES = 1440, MAX_LINK = 500;

/** The link when it is an http(s) address without credentials, else null. */
function validLink(s) {
  const v = String(s ?? '').trim();
  if (!v || v.length > MAX_LINK || !/^https?:\/\//i.test(v)) return null;
  try {
    const u = new URL(v);
    if (u.username || u.password || !u.hostname) return null;
  } catch { return null; }
  return v;
}

export default {
  type: 'webpicture', key: 'webpicture', name: 'Web picture', icon: '▦', group: 'Media',
  props: () => ({ source: '', minutes: 15, fit: 'contain', opacity: 1 }),
  size: p => [0.3, 0.4],
  title: w => 'Web picture',
  panel(w, host, api, h) {
    const { el } = api;
    const { field, select2, range, numInput } = api.h;
    const p = w.props;
    const mins = () => Math.min(MAX_MINUTES, Math.max(MIN_MINUTES, Math.round(Number(p.minutes) || 15)));
    const src = el('input', { type: 'text', id: 'pWpSrc', maxLength: MAX_LINK, spellcheck: false, placeholder: 'https://…/image.jpg',
      onchange: e => {
        const v = validLink(e.target.value);
        if (v === null && e.target.value.trim() !== '') { e.target.value = p.source || ''; return; }
        p.source = v || '';
        e.target.value = p.source;
        api.edited();
      } });
    src.value = typeof p.source === 'string' ? p.source : '';
    const note = el('p', { class: 'muted small' });
    const noteText = () => { note.textContent = `Fetched from this link every ${mins()} minute${mins() === 1 ? '' : 's'} while the item is shown. Nothing else is sent.`; };
    noteText();
    host.append(
      field('Picture link', src, 'pWpSrc'),
      note,
      field('Refresh every (minutes)', numInput('pWpMin', mins(), { min: MIN_MINUTES, max: MAX_MINUTES, onchange: v => {
        const n = Math.min(MAX_MINUTES, Math.max(MIN_MINUTES, Math.round(v ?? 15)));
        api.set('minutes', n); noteText();
      } }), 'pWpMin'),
      field('Fit', select2('pWpFit', p.fit === 'cover' ? 'cover' : 'contain', [['contain', 'Whole picture'], ['cover', 'Fill']], v => api.set('fit', v)), 'pWpFit'),
      field('Opacity', range('pWpOp', Math.round(Math.min(1, Math.max(0, p.opacity ?? 1)) * 100), 0, 100, 5, v => api.set('opacity', v / 100), v => v + ' %'), 'pWpOp'));
    return true;
  },
};
