// Placeholder PowerToys-style frame (phase 0): left list of pages, right pane with placeholder text.
// The chosen page is a setting (settings.json `ui.page`), saved as soon as it is picked (no Save button).
const T = window.__TAURI_INTERNALS__;
const invoke = (cmd, args) => (T ? T.invoke(cmd, args || {}) : Promise.reject(new Error('not in the app')));

const PAGES = {
  home: { title: 'Home', text: 'The status of every device will show here at a glance.' },
  dock: { title: 'Stream dock', text: 'The Ulanzi D200H stream dock page comes in phase 1: keys, pages, actions and the live usage keys.' },
  general: { title: 'General', text: 'Common settings will live here: start with Windows (off unless you turn it on), appearance, updates, privacy and diagnostics.' },
  about: { title: 'About', text: 'Unified Control: one tray app for your devices.' },
};

let state = null;
const pane = document.getElementById('pane');
const esc = s => String(s).replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]);

function render(page) {
  const p = PAGES[page] || PAGES.home;
  document.querySelectorAll('.nav-item').forEach(b => {
    if (b.dataset.page === page) b.setAttribute('aria-current', 'page');
    else b.removeAttribute('aria-current');
  });
  let html = `<h1>${esc(p.title)}</h1><p>${esc(p.text)}</p>`;
  if (page === 'about' && state) {
    const b = state.build || {};
    html += `<dl><dt>Version</dt><dd>${esc(state.appVersion)}</dd><dt>Build</dt><dd>${esc(b.hash)} (${esc(b.commitDate)})${esc(b.note || '')}</dd></dl>`;
  }
  pane.innerHTML = html;
}

document.querySelectorAll('.nav-item').forEach(b => {
  b.addEventListener('click', () => {
    render(b.dataset.page);
    invoke('settings_update', { patch: { page: b.dataset.page } }).then(s => { state = s; }).catch(() => {});
  });
});

invoke('app_state').then(s => { state = s; render(s.page); }).catch(() => render('home'));
