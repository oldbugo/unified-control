// 4.x E M5 "Lyrics" item. Registered by morewidgets.js. Synced lyrics of the song that plays (karaoke line). Follows
// docs/v4/ui-style-guide.md section 4: order = what it shows -> look -> behaviour; changes save at once through api.set / api.edited
// (no Save button); labels short, sentence case. The timing buttons talk to the live engine through two commands:
// lyrics_status ({ track, key, state, source, offset }) and lyrics_offset_set ({ key, seconds }).

const STEP = 0.25;
let poll = null; // the one status timer; cleared when the panel is repainted or closed

const fmtOffset = o => (o < 0 ? '−' : '+') + Math.abs(o).toFixed(2) + ' s'; // no break inside the readout
const STATE_NOTE = {
  loading: 'Looking for the lyrics…',
  notFound: 'No synced lyrics for this song.',
  noSynced: 'No synced lyrics for this song.',
  instrumental: 'This song is instrumental.',
  off: 'Lyrics lookup is off. Switch on “Find lyrics online” or choose a folder.',
};

export default {
  type: 'lyrics', key: 'lyrics', name: 'Lyrics', icon: '♪', group: 'Media',
  props: () => ({ online: false, folder: null, neighbours: true, background: 'card', accent: null }),
  /** Default box [w, h] (fractions of the 2400x1080 canvas): wide, with room for the previous and next line. */
  size: () => [0.70, 0.30],
  title: () => 'Lyrics',
  /** Append the Properties controls to `host`; return true. `h`: { colour(key, text, dflt, resettable), restructure(fn), clamp, rateField(), segField() }. */
  panel(w, host, api, h) {
    const { el } = api;
    const { field, swRow } = api.h;
    const p = w.props;
    if (poll) { clearInterval(poll); poll = null; }

    // what it shows
    const folder = el('input', { type: 'text', id: 'pLyrFolder', maxLength: 260, spellcheck: false, placeholder: 'No folder',
      onchange: e => { const v = e.target.value.trim(); p.folder = v || null; e.target.value = v; api.edited(); } });
    folder.value = typeof p.folder === 'string' ? p.folder : '';
    host.append(
      swRow('Find lyrics online', !!p.online, v => api.set('online', v), 'Sends the song title, artist and length to lrclib.net when the song changes.'),
      field('Lyrics folder', folder, 'pLyrFolder'),
      el('p', { class: 'muted small', text: 'Optional: a folder with .lrc files, used first' }));

    // how it looks
    host.append(
      h.segField('Background', 'pLyrBg', p.background || 'card', [['card', 'Card'], ['gradient', 'Gradient'], ['none', 'None']], v => h.restructure(() => { p.background = v; })),
      swRow('Show previous and next', p.neighbours !== false, v => api.set('neighbours', v)),
      h.colour('accent', 'Accent', '#5ac8ff', true),
      el('p', { class: 'muted small', text: 'Automatic takes a colour from the cover.' }));

    // behaviour: timing of the song that plays
    const track = el('p', { class: 'small', id: 'pLyrTrack', text: 'Nothing playing' });
    const note = el('p', { class: 'muted small', id: 'pLyrNote', text: 'Timing can be adjusted while a song plays with Live on.' });
    const val = el('span', { class: 'num', id: 'pLyrOff', text: fmtOffset(0), 'aria-live': 'polite' });
    let cur = null; // the last status that allowed a change: { key, offset }
    const nudge = async d => {
      if (!cur) return;
      try {
        const r = await api.backend.call('lyrics_offset_set', { key: cur.key, seconds: d === null ? 0 : Math.round((cur.offset + d) * 100) / 100 });
        cur.offset = Number(r.offset) || 0;
        val.textContent = fmtOffset(cur.offset);
      } catch { /* the engine is not reachable: the next status says so */ }
    };
    const btn = (text, label, d) => el('button', { type: 'button', class: 'btn secondary', text, 'aria-label': label, title: label, disabled: true, onclick: () => nudge(d) });
    const minus = btn(`−${STEP.toFixed(2)} s`, 'Show each line later', -STEP);
    const plus = btn(`+${STEP.toFixed(2)} s`, 'Show each line sooner', STEP);
    const reset = el('button', { type: 'button', class: 'icon-btn', title: 'Reset the timing for this song', 'aria-label': 'Reset the timing for this song', disabled: true, text: '↺', onclick: () => nudge(null) });
    host.append(
      el('span', { class: 'field-label', text: 'Timing for this song' }), track,
      el('div', { class: 'inline-ctl' }, minus, val, plus, reset), note);

    const show = s => {
      const can = !!(s && s.key && s.state === 'found');
      track.textContent = s && s.track ? s.track : 'Nothing playing';
      cur = can ? { key: s.key, offset: Number(s.offset) || 0 } : null;
      val.textContent = fmtOffset(can ? cur.offset : 0);
      minus.disabled = plus.disabled = reset.disabled = !can;
      note.textContent = !s || s.state === 'none' ? 'Timing can be adjusted while a song plays with Live on.' : (can ? 'Plus shows each line sooner. Remembered for this song.' : (STATE_NOTE[s.state] || ''));
    };
    const tick = async () => {
      // stop once this panel is gone (another item or nothing selected); skip while it is hidden (another page)
      if (!track.isConnected) { if (poll) { clearInterval(poll); poll = null; } return; }
      if (!track.offsetParent) return;
      try { show(await api.backend.call('lyrics_status')); } catch { show(null); }
    };
    tick();
    poll = setInterval(tick, 1000);

    host.append(el('p', { class: 'muted small', text: 'Lyrics come from lrclib.net (community, free). Shown only when the song is found.' }));
    return true;
  },
};
