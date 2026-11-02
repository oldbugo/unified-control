// 4.0 G2 "Game FPS" item: the frame rate of the game in front. Registered by morewidgets.js. Same shape as w-todo.js / w-headlines.js.
// Follows docs/v4/ui-style-guide.md section 4: helpers in `api.h` (field, select2, range, numInput, swRow) and `h.colour`; changes save at
// once through api.set / api.edited (no Save button); labels short, sentence case.
export default {
  type: 'gamefps', key: 'gamefps', name: 'Game FPS', icon: '▦', group: 'Sensors',
  props: () => ({ low: true, graph: true, history: 30, hideIdle: false, color: '#5ac8ff' }),
  size: p => [0.3, p.graph === false ? 0.2 : 0.36],
  title: w => 'Game FPS',
  panel(w, host, api, h) {
    const { el } = api;
    const { field, range, swRow } = api.h;
    const p = w.props;
    const clamp = (v, a, b) => Math.min(b, Math.max(a, v));

    // Status block: why there is (not) a number, and the one-time command when Windows needs it.
    const status = el('div', { class: 'gamefps-status' });
    const msg = el('p', { class: 'muted small', text: 'Checking…' });
    const extra = el('div', { class: 'gamefps-setup' });
    status.append(msg, extra);
    const load = () => {
      let call;
      try { call = api.backend.call('game_fps_status'); } catch (e) { status.hidden = true; return; }
      Promise.resolve(call).then(s => {
        status.hidden = false;
        msg.textContent = (s && s.message) || '';
        msg.classList.toggle('warn-text', !(s && s.ok) && !(s && s.state === 'noGame'));
        extra.textContent = '';
        if (s && s.command) {
          const box = el('textarea', { readOnly: true, rows: 3, spellcheck: false, 'aria-label': 'Command to run once', style: 'font-family:Consolas,monospace;width:100%;resize:none' });
          box.value = s.command;
          const copy = el('button', { type: 'button', class: 'btn secondary', text: 'Copy' });
          copy.onclick = async () => {
            try {
              await navigator.clipboard.writeText(s.command);
              if (typeof api.toast === 'function') api.toast('Copied');
              else { copy.textContent = 'Copied'; setTimeout(() => { copy.textContent = 'Copy'; }, 1000); }
            } catch (e) { box.select(); }
          };
          extra.append(box,
            el('div', { class: 'row' }, copy),
            el('p', { class: 'muted small', text: 'Run this once in PowerShell as administrator, then sign out and in. It lets your account read Windows’ graphics events (which programs show frames). It does not make you an administrator.' }));
        }
        if (s && !s.ok) extra.append(el('div', { class: 'row' }, el('button', { type: 'button', class: 'btn ghost', text: 'Check again', onclick: () => { msg.textContent = 'Checking…'; load(); } })));
      }, () => { status.hidden = true; });
    };
    load();

    host.append(
      status,
      swRow('1 % low', p.low !== false, v => api.set('low', v)),
      swRow('Frame-time graph', p.graph !== false, v => h.restructure(() => { p.graph = v; })));
    if (p.graph !== false) {
      host.append(field('History (s)', range('pHist', clamp(p.history || 30, 10, 120), 10, 120, 5, v => api.set('history', Math.round(v)), v => String(v)), 'pHist'));
    }
    host.append(
      swRow('Hide when no game', !!p.hideIdle, v => api.set('hideIdle', v)),
      el('p', { class: 'muted small', text: 'Draws nothing while no game is in front.' }),
      h.colour('color', 'Colour', '#5ac8ff'));
    return true;
  },
};
