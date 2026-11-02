// 4.x E M3: the Properties of the music item - a picture choice of five looks, then the switches. Follows docs/v4/ui-style-guide.md
// section 4: the `.styles` / `.sty` picture choice, `swRow` switches, every change saves at once through `set` (no Save button).
const LOOKS = [
  ['card', 'Card', '<rect x="4" y="6" width="40" height="20" rx="4" fill="none" stroke="currentColor" stroke-width="1.6"/><rect x="8" y="10" width="12" height="12" rx="2" fill="currentColor" opacity=".5"/><rect x="24" y="11" width="16" height="3" rx="1.5" fill="currentColor"/><rect x="24" y="17" width="10" height="2" rx="1" fill="currentColor" opacity=".6"/><rect x="24" y="21" width="16" height="2" rx="1" fill="currentColor" opacity=".35"/>'],
  ['cover', 'Cover', '<rect x="3" y="3" width="42" height="26" rx="4" fill="none" stroke="currentColor" stroke-width="1.6"/><rect x="6" y="6" width="20" height="20" rx="3" fill="currentColor" opacity=".5"/><rect x="29" y="9" width="13" height="3.5" rx="1.7" fill="currentColor"/><rect x="29" y="15" width="9" height="2" rx="1" fill="currentColor" opacity=".6"/><rect x="29" y="21" width="13" height="2" rx="1" fill="currentColor" opacity=".35"/>'],
  ['line', 'Line', '<rect x="3" y="9" width="42" height="14" rx="7" fill="none" stroke="currentColor" stroke-width="1.6"/><path d="M9 13.5v5l4-2.5z" fill="currentColor"/><rect x="17" y="14" width="16" height="3" rx="1.5" fill="currentColor"/><rect x="35" y="14" width="6" height="3" rx="1.5" fill="currentColor" opacity=".45"/><rect x="9" y="20" width="30" height="1.5" rx=".75" fill="currentColor" opacity=".35"/>'],
  ['disc', 'Disc', '<circle cx="14" cy="16" r="11" fill="none" stroke="currentColor" stroke-width="1.6"/><circle cx="14" cy="16" r="7.5" fill="none" stroke="currentColor" stroke-width="1" opacity=".4"/><circle cx="14" cy="16" r="5" fill="currentColor" opacity=".5"/><circle cx="14" cy="16" r="1.2" fill="var(--surface-2, #000)"/><rect x="29" y="10" width="14" height="3.5" rx="1.7" fill="currentColor"/><rect x="29" y="16" width="9" height="2" rx="1" fill="currentColor" opacity=".6"/><rect x="29" y="21" width="14" height="2" rx="1" fill="currentColor" opacity=".35"/>'],
  ['blur', 'Blur', '<rect x="3" y="3" width="42" height="26" rx="4" fill="currentColor" opacity=".28"/><rect x="3" y="3" width="42" height="26" rx="4" fill="none" stroke="currentColor" stroke-width="1.6"/><rect x="7" y="8" width="16" height="16" rx="2.5" fill="currentColor" opacity=".6"/><rect x="27" y="10" width="14" height="3.5" rx="1.7" fill="currentColor"/><rect x="27" y="16" width="9" height="2" rx="1" fill="currentColor" opacity=".6"/><rect x="27" y="21" width="14" height="2" rx="1" fill="currentColor" opacity=".35"/>'],
];
const IDS = LOOKS.map(l => l[0]);

/**
 * Append the Properties of a music item to `host`. `ctx`: { el, swRow, set(key, v), repaint() } from themes.js.
 * An unknown or missing `style` is the Card (today's look).
 */
export function paintMusicProps(w, host, ctx) {
  const { el, swRow, set, repaint } = ctx;
  const p = w.props;
  const cur = IDS.includes(p.style) ? p.style : 'card';
  const grid = el('div', { class: 'styles', role: 'group', 'aria-label': 'Look' });
  for (const [id, text, art] of LOOKS) {
    const b = el('button', { type: 'button', class: 'sty', 'aria-pressed': String(cur === id), title: text, onclick: () => {
      set('style', id === 'card' ? undefined : id);
      if (id === 'card') delete p.style; // the Card is the default: no key, so older themes stay as they were
      repaint();
    } });
    b.innerHTML = `<svg viewBox="0 0 48 32" width="44" height="30" aria-hidden="true">${art}</svg><span>${text}</span>`;
    grid.append(b);
  }
  host.append(el('p', { class: 'small muted', text: 'Look' }), grid,
    swRow('Show the cover', p.cover !== false, v => set('cover', v)),
    swRow('Show the progress bar', p.progress !== false, v => set('progress', v)));
  if (cur === 'disc') host.append(swRow('Turn while playing', p.spin !== false, v => set('spin', v)));
  host.append(el('p', { class: 'muted small', text: 'Shows what is playing in Spotify, the browser or any other player. Hidden when nothing plays (theme settings).' }));
}
