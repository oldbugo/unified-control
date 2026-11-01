// Preview on the real screen: selecting a theme (My themes) or an item (Browse) shows it on the screen at once; leaving without
// using it puts the screen back. The page only asks (debounced, so quick clicking sends the last one); the engine keeps one stream
// and swaps the picture. Nothing is saved. Ends: another view, the selection cleared, the window closing, or 2 minutes with no
// activity (the engine also ends a preview nobody has asked for in 150 s, so it can never keep running unseen).
export const IDLE_MS = 120000;
const DEBOUNCE_MS = 150;
const KEEPALIVE_MS = 30000;

export function createScreenPreview({ call, toast, onChange }) {
  const P = { key: null, shown: false, timer: 0, idle: 0, keep: 0, seq: 0, expired: false };
  const change = () => { try { onChange && onChange(); } catch (e) { /* the page is mid-render */ } };

  function clearTimers() {
    clearTimeout(P.timer); clearTimeout(P.idle); clearInterval(P.keep);
    P.timer = P.idle = P.keep = 0;
  }
  function armIdle() {
    clearTimeout(P.idle);
    P.idle = setTimeout(() => { P.expired = true; end(); toast('Preview ended.'); }, IDLE_MS);
  }
  // any activity in the page counts as "still there"
  const touch = () => { if (P.key && P.shown) armIdle(); };
  for (const ev of ['pointerdown', 'keydown', 'wheel']) window.addEventListener(ev, touch, { passive: true, capture: true });

  async function send(args, key) {
    const seq = ++P.seq;
    try {
      await call('screen_preview', args);
      if (seq !== P.seq) return; // a newer click came while this one was on its way
      P.shown = true; P.expired = false;
      armIdle();
      if (!P.keep) P.keep = setInterval(() => call('screen_preview_end', { keepalive: true }).catch(() => {}), KEEPALIVE_MS);
    } catch (e) {
      if (seq !== P.seq) return;
      P.shown = false;
      if (P.key === key) toast(e.message || 'Could not preview this.', false, 4000);
    }
    change();
  }

  // Ask for a preview (debounced). `key` identifies what is shown, so the page can mark it.
  function show(key, args) {
    P.key = key; P.expired = false; P.shown = false;
    clearTimeout(P.timer);
    P.timer = setTimeout(() => send(args, key), DEBOUNCE_MS);
    change();
  }
  const showTheme = oid => show('t:' + oid, { originalId: oid });
  const showPicture = (sourceId, item, thumb) => show(`s:${sourceId}/${item}`, { sourceId, item, thumb: /^https:\/\//.test(thumb || '') ? thumb : null });

  // Put the screen back. Safe to call any time (nothing previewed: nothing happens).
  async function end() {
    const had = !!P.key;
    clearTimers();
    P.seq++; P.key = null; P.shown = false;
    if (had) { try { await call('screen_preview_end', {}); } catch (e) { /* the engine's own timer ends it */ } }
    change();
  }

  // best effort when the window closes (the app also ends it on close, and the engine on its own timer)
  window.addEventListener('pagehide', () => { if (P.key) call('screen_preview_end', {}).catch(() => {}); });

  return { show, showTheme, showPicture, end, key: () => P.key, shown: () => P.shown, expired: () => P.expired };
}
