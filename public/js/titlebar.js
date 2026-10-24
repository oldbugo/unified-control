// Seamless frameless window chrome for the Tauri shell (vault pattern "Seamless frameless window"):
// a transparent 32 px title bar with three Windows 11-style buttons (46 x 32) drawn over the app's own top bar.
// No-op outside Tauri (browser, Electron). The Rust side (src-tauri/src/window) keeps an invisible native
// window over the maximise button so Windows 11 Snap Layouts work; that window swallows the mouse, so hover,
// pressed and click for that one button arrive as `cs-snap` events instead of DOM events.
import { openDialog } from './dialog.js';
const T = window.__TAURI_INTERNALS__;
const BTN_W = 46, BAR_H = 32;

if (T) {
  const inv = (cmd, args) => T.invoke(cmd, args || {});
  const listen = (event, fn) =>
    inv('plugin:event|listen', { event, target: { kind: 'Any' }, handler: T.transformCallback(e => fn(e.payload)) });

  const ICON = {
    min: '<svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M0 5h10" stroke="currentColor" stroke-width="1"/></svg>',
    max: '<svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><rect x=".5" y=".5" width="9" height="9" rx="1" fill="none" stroke="currentColor"/></svg>',
    restore: '<svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><rect x=".5" y="2.5" width="7" height="7" rx="1" fill="none" stroke="currentColor"/><path d="M2.5 2.5V1.5a1 1 0 0 1 1-1h5a1 1 0 0 1 1 1v5a1 1 0 0 1-1 1h-1" fill="none" stroke="currentColor"/></svg>',
    close: '<svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M0 0l10 10M10 0L0 10" stroke="currentColor" stroke-width="1"/></svg>',
  };
  const CSS = `
:root{--cs-controls-w:${BTN_W * 3}px;--cs-bar-h:${BAR_H}px}
#cs-titlebar{position:fixed;top:0;right:0;height:${BAR_H}px;width:${BTN_W * 3}px;z-index:2147483000;display:flex;
  background:transparent;pointer-events:none;-webkit-user-select:none;user-select:none}
#cs-titlebar button{pointer-events:auto;width:${BTN_W}px;height:${BAR_H}px;border:0;margin:0;padding:0;border-radius:0;display:grid;
  place-items:center;background:transparent;color:var(--cs-tb-fg,#e8eaee);cursor:default;outline:none}
#cs-titlebar button:hover,#cs-titlebar button.hot{background:rgba(255,255,255,.08)}
#cs-titlebar button:active,#cs-titlebar button.down{background:rgba(255,255,255,.05)}
#cs-titlebar #cs-close:hover{background:#c42b1c;color:#fff}
#cs-titlebar #cs-close:active{background:#a82116;color:#fff}
@media (prefers-color-scheme: light){:root:not(.cs-dark-ui) #cs-titlebar button:hover,:root:not(.cs-dark-ui) #cs-titlebar button.hot{background:rgba(0,0,0,.06)}}`;

  const mount = () => {
    if (document.getElementById('cs-titlebar')) return;
    const st = document.createElement('style');
    st.textContent = CSS;
    document.head.appendChild(st);
    const bar = document.createElement('div');
    bar.id = 'cs-titlebar';
    bar.innerHTML =
      `<button id="cs-min" type="button" aria-label="Minimise" tabindex="-1">${ICON.min}</button>` +
      `<button id="cs-max" type="button" aria-label="Maximise" tabindex="-1">${ICON.max}</button>` +
      `<button id="cs-close" type="button" aria-label="Close" tabindex="-1">${ICON.close}</button>`;
    document.body.appendChild(bar);

    // Tauri drags only when the pressed element itself carries the attribute, so buttons inside stay clickable.
    // Double-click on it maximises / restores.
    const markDrag = () =>
      document.querySelectorAll('.topbar, .topbar .appname, .topbar .spacer, [data-cs-drag]')
        .forEach(e => e.setAttribute('data-tauri-drag-region', ''));
    markDrag();
    new MutationObserver(markDrag).observe(document.body, { childList: true, subtree: true });

    const maxBtn = bar.querySelector('#cs-max');
    const setMax = m => {
      maxBtn.innerHTML = m ? ICON.restore : ICON.max;
      maxBtn.setAttribute('aria-label', m ? 'Restore' : 'Maximise');
    };
    const toggle = () => inv('win_toggle_maximize').then(setMax);
    bar.querySelector('#cs-min').onclick = () => inv('win_minimize');
    maxBtn.onclick = toggle;
    bar.querySelector('#cs-close').onclick = () => inv('win_close');
    window.addEventListener('keydown', e => { if (e.key === 'F11') { e.preventDefault(); toggle(); } });

    listen('window-state', p => setMax(!!(p && p.maximized)));
    listen('cs-snap', ev => {
      if (ev === 'enter') maxBtn.classList.add('hot');
      else if (ev === 'leave') maxBtn.classList.remove('hot', 'down');
      else if (ev === 'down') maxBtn.classList.add('down');
      else if (ev === 'up') maxBtn.classList.remove('down');
      else if (ev === 'click') { maxBtn.classList.remove('down'); toggle(); }
    });
    // An upload is running: the app's own dialog (never a native one) asks before closing.
    let asking = false;
    listen('close-guard', async () => {
      if (asking) return;
      asking = true;
      const ok = await openDialog({
        title: 'An upload is in progress',
        body: 'If you close now, the screen may show a broken picture until you send again. Close anyway?',
        actions: [{ label: 'Keep uploading', value: false, kind: 'primary' }, { label: 'Close anyway', value: true, kind: 'danger' }],
      });
      asking = false;
      if (ok) inv('win_force_close');
    });
    inv('win_state').then(setMax).catch(() => {});
  };
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', mount);
  else mount();
}
