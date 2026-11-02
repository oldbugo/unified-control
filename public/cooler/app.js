// CoolerScreen UI (3.0): On screen (theme editor) | My themes | Browse | Base settings (a clip's loop, quality, versions) | Activity strip | Settings drawer.
import { W, H, frameGeometry, fitZoom, bandPx, styleNotch, makeCornersOverlay, styleCorners } from './js/frame.js';
import { openDialog } from './js/dialog.js';
import { icon, upgradeGlyphs } from './js/icons.js';
import { initSettingsNav } from './js/settingsnav.js';
import { backend } from './js/backend.js';
import { createBandSlider } from './js/bandslider.js';
import { createThemes, initApply, runApply, fillGuides, layers as Ly } from './js/themes.js';
import { createScreenPreview } from './js/screenpreview.js';
import { screenStatus, rememberedScreen, connectionMessage, liveMessage, shouldWatchPresence, presenceArrival } from './js/connection-status.js';

// ---------- small helpers ----------
const $ = id => document.getElementById(id);
function el(tag, props = {}, ...kids) {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(props)) {
    if (v == null || v === false) continue;
    if (k === 'class') e.className = v;
    else if (k === 'text') e.textContent = v;
    else if (k === 'style') e.style.cssText = v;
    else if (k.startsWith('on')) e.addEventListener(k.slice(2), v);
    else if (k in e && !k.startsWith('aria') && k !== 'role' && !k.startsWith('data')) e[k] = v;
    else e.setAttribute(k, v === true ? '' : v);
  }
  for (const c of kids.flat()) if (c != null && c !== false) e.append(c);
  return e;
}
const mb = b => (b / 1e6).toFixed(1) + ' MB';
const mb2 = b => (b / 1e6).toFixed(2) + ' MB';
const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));
const r1 = v => Math.round(v * 10) / 10;
function fmtTime(s, tenths = true) {
  if (!Number.isFinite(s)) return '–';
  s = Math.max(0, s);
  const m = Math.floor(s / 60), sec = s - m * 60;
  return `${m}:${(tenths ? sec.toFixed(1) : Math.floor(sec).toString()).padStart(tenths ? 4 : 2, '0')}`;
}
const fmtSecs = s => (Math.round(s * 10) / 10).toString().replace(/\.0$/, '') + ' s';
function fmtWhen(iso) {
  const d = new Date(iso);
  if (isNaN(d)) return '';
  const now = new Date();
  const t = d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  if (d.toDateString() === now.toDateString()) return 'today ' + t;
  const y = new Date(now); y.setDate(now.getDate() - 1);
  if (d.toDateString() === y.toDateString()) return 'yesterday ' + t;
  return d.toLocaleDateString([], { day: 'numeric', month: 'short', year: d.getFullYear() === now.getFullYear() ? undefined : 'numeric' }) + ' ' + t;
}
const fmtDate = iso => new Date(iso).toLocaleDateString([], { day: 'numeric', month: 'short', year: 'numeric' });
function fmtDuration(sec) {
  if (sec < 60) return 'less than a minute';
  const m = Math.ceil(sec / 60);
  return `about ${m} minute${m > 1 ? 's' : ''}`;
}
function fmtLeft(sec) {
  sec = Math.max(0, Math.round(sec));
  const m = Math.floor(sec / 60), s = sec % 60;
  return m ? `about ${m} min ${s} s left` : `about ${s} s left`;
}

const call = (command, args) => backend.call(command, args);
const TRIM_MAX = 30; // the longest loop the loop finder looks for; what really fits is shown by the size meter ("Best that fits")

let toastT;
function toast(msg, bad = false, ms = 5000) {
  const t = $('toast');
  t.textContent = msg; t.className = 'toast show' + (bad ? ' bad' : '');
  clearTimeout(toastT); toastT = setTimeout(() => (t.className = 'toast'), ms);
}

// ---------- state ----------
// Preview on the real screen (My themes selection, Browse selection); see js/screenpreview.js. Nothing is saved.
const SP = createScreenPreview({ call: (...a) => call(...a), toast: (...a) => toast(...a), onChange: () => { if (S.view === 'mine') renderSelBar(); if (S.view === 'browse') renderBrowsePreviewBar(); } });
const S = {
  app: null, originals: [], longMissing: [], rev: -1, sel: null, detail: null,
  jobs: [], deviceBusy: false,
  screen: { state: 'checking', info: null, error: null },
  showing: null, // null = the tuning (original), or a version id
  draft: null, est: { state: 'idle' }, adding: [],
  missingDismissed: false, outcomes: new Map(), dismissed: new Set(), loadedAt: Date.now(),
  view: null, // 'screen' (On screen) | 'mine' (My themes) | 'browse' (Browse) | 'detail' (a theme's base settings)
  home: 'screen', // the view the open base settings were reached from ('screen' | 'mine')
  currentTheme: null, // the LIVE theme: what Live shows (the one chosen with "Use this theme", else the Embedded one)
  embeddedTheme: null, // the theme whose picture is stored on the screen (Embedded)
  selTheme: null, // the card selected in My themes
  baseFor: null, // the theme whose base is being chosen in Base settings
  manualLoop: false, // "Edit loop manually…" panel open
};
const DEFAULT_FRAME = { zoom: 1, x: 0.5, y: 0.5 };

// ---------- top bar ----------
const PILL = {
  checking: 'Checking screen…', ready: 'Screen ready', setup: 'Needs one-time setup', failedUsb: 'Not connected',
  notFound: 'Screen not found', busy: 'Screen in use', updating: 'Updating screen…', error: 'Screen problem',
  noReply: 'Screen not responding', connecting: 'Connecting screen…',
};
function currentScreen() {
  return screenStatus(S.screen, S.app?.live, { checking, deviceBusy: S.deviceBusy, jobs: S.jobs });
}
function pillState() {
  return currentScreen().state;
}
function renderTop() {
  const st = pillState();
  $('pill').dataset.state = st;
  $('pillText').textContent = PILL[st] || st;
  renderLiveSwitch();
}
// The Live switch sits in the same indicator as the screen status; it is the same setting as Settings > Live screen.
function renderLiveSwitch() {
  const l = (S.app && S.app.live) || {}, c = l.settings || {}, stt = l.status || {};
  const sw = $('liveSwitch'), box = sw.closest('.live-switch');
  sw.checked = !!c.inApp;
  sw.disabled = !S.app;
  const hold = stt.paused || stt.deviceHold || stt.systemHold;
  box.dataset.state = !c.inApp ? 'off' : hold ? 'paused' : 'on';
  box.title = !c.inApp ? 'Live is off: the screen shows Embedded.'
    : stt.deviceHold ? 'Live is paused: the screen is being updated.' : stt.systemHold ? 'Live is paused: the PC sleeps or is locked.' : stt.paused ? 'Live is paused from the tray.'
    : liveMessage(l, currentScreen()) || 'Live is on.';
  sw.setAttribute('aria-label', 'Live on the screen: ' + (c.inApp ? 'on' : 'off'));
  // Live is on but the theme draws nothing live: say so, and where to fix it
  const none = !!c.inApp && stt.liveItems === false;
  $('liveHint').hidden = !none;
  if (none) { $('liveHintText').textContent = stt.layerOff ? 'Live layer is off.' : 'No live items.'; $('liveHintGo').textContent = stt.layerOff ? 'Turn on' : 'Add some'; }
}
$('liveHintGo').onclick = () => goHome('screen');
$('liveSwitch').onchange = () => liveSet({ inApp: $('liveSwitch').checked });
$('pill').onclick = () => openDrawer('setScreen');
$('gear').onclick = () => openDrawer();

// ---------- banners ----------
function renderBanners() {
  const box = $('banners');
  box.textContent = '';
  const screen = currentScreen(), st = screen.state;
  const add = (kind, text, ...buttons) => {
    const row = el('div', { class: 'banner ' + kind }, el('p', { text }), ...buttons);
    box.append(row); return row;
  };
  const connection = (kind, ...buttons) => { add(kind, connectionMessage(screen), ...buttons).dataset.connectionState = st; };
  if (st === 'setup') add('warn', 'Your screen will switch off 30 seconds after this app closes until you run the one-time setup.',
    el('button', { class: 'btn primary', text: 'Set up screen', onclick: runSetup }));
  if (st === 'failedUsb') connection('danger',
    el('button', { class: 'btn primary', text: 'Reconnect', title: 'Windows will ask for permission', onclick: runReconnect }));
  if (st === 'notFound') connection('danger',
    el('button', { class: 'btn secondary', text: 'Check again', onclick: checkScreen }));
  if (st === 'noReply') connection('danger',
    el('button', { class: 'btn primary', text: 'Restart screen', onclick: runRestart }));
  if (st === 'busy') connection('warn',
    el('button', { class: 'btn secondary', text: 'Try again', onclick: () => checkScreen() }));
  if (st === 'error') connection('danger',
    el('button', { class: 'btn secondary', text: 'Check again', onclick: checkScreen }));
  const sc = S.app && S.app.screenChanged;
  if (sc) {
    const mb = b => (b / 1e6).toFixed(1) + ' MB';
    add('warn', `The screen changed since CoolerScreen last stored Embedded (free space ${mb(sc.wasFree)} then, ${mb(sc.nowFree)} now). Another program may have written to it.`,
      el('button', { class: 'btn secondary', text: 'It’s fine', title: 'Treat what is on the screen now as correct', onclick: async () => { try { S.app = await call('settings_update', { screenChanged: 'accept' }); } catch (e) { toast(e.message, true); } renderBanners(); } }));
  }
  const os = S.app && S.app.onScreen;
  if (os && os.incomplete && !S.jobs.some(j => j.kind === 'send' && (j.status === 'running' || j.status === 'waiting'))) {
    add('danger', `Embedded (“${os.originalName} · ${os.label}”) didn't finish storing; the screen may show a broken picture.`,
      os.exists ? el('button', { class: 'btn primary', text: 'Store again', onclick: () => startSend(os.versionId) }) : null);
  }
  if (S.longMissing.length && !S.missingDismissed) {
    const n = S.longMissing.length;
    add('info', `${n} clip${n > 1 ? 's have' : ' has'} been missing from the Gallery folder for more than ${S.app ? S.app.missingNoticeDays : 30} days. Their versions are still kept.`,
      el('button', { class: 'btn secondary', text: 'Review…', onclick: reviewMissing }),
      el('button', { class: 'btn ghost', text: 'Not now', onclick: () => { S.missingDismissed = true; renderBanners(); } }));
  }
}

async function reviewMissing() {
  const list = el('ul', {}, S.longMissing.map(m => el('li', { text: `${m.name} – missing since ${fmtDate(m.since)}` })));
  const ok = await openDialog({
    title: 'Delete versions of missing clips?',
    body: el('div', {}, el('p', { text: 'These files are no longer in the Gallery folder. If you moved them on purpose, their saved versions can be deleted. Nothing is deleted unless you choose to.' }), list),
    actions: [{ label: 'Keep them', value: false, kind: 'secondary' }, { label: 'Delete their versions', value: true, kind: 'danger' }],
  });
  S.missingDismissed = true;
  if (ok) {
    try { await call('missing_delete', { ids: S.longMissing.map(m => m.id) }); toast('Deleted.'); } catch (e) { toast(e.message, true); }
    await loadOriginals();
  }
  renderBanners();
}

// ---------- screen check (launch, explicit action or device return; healthy streams are never polled) ----------
let checking = false;
let busyTimer = 0;
async function checkScreen(opts) {
  if (checking) return;
  clearTimeout(busyTimer);
  const quiet = !!(opts && opts.quiet); // a retry while busy: the banner stays until the screen is free
  checking = true;
  if (!quiet) { S.screen = { ...S.screen, state: 'checking' }; renderTop(); renderBanners(); renderScreenSettings(); }
  try {
    const info = await call('device_check');
    S.screen = { state: info._needsSetup ? 'setup' : 'ready', info, error: null };
    if (info._restarted) toast('The screen wasn\'t answering, so CoolerScreen restarted it. It is fine now.', false, 8000);
    loadApp(); // the check may have flagged a change outside the app, or measured the capacity
  } catch (e) {
    if (e.status === 409) S.screen = { state: 'updating', info: null, error: null, retry: true };
    else S.screen = { state: e.failedUsb ? 'failedUsb' : e.code === 'notFound' ? 'notFound' : e.code === 'busy' ? 'busy' : e.code === 'noReply' ? 'noReply' : 'error', info: null, error: e.message };
  }
  checking = false;
  if (S.screen.state === 'busy' && !S.app?.live?.settings?.inApp) busyTimer = setTimeout(() => checkScreen({ quiet: true }), 4000); // Live already retries on its own
  renderTop(); renderBanners(); renderScreenSettings();
}

// ---------- data loading ----------
async function loadApp() {
  S.app = await call('app_state');
  S.screen = rememberedScreen(S.screen, S.app.live, { checking, deviceBusy: S.deviceBusy, jobs: S.jobs });
  if (S.app.build) document.title = `CoolerScreen ${S.app.appVersion} (${S.app.build.hash})${S.app.build.screen || ''}`;
  backend.setMediaBase(S.app.mediaBase);
  renderTop(); renderBanners(); renderScreenSettings();
  if (typeof themes !== 'undefined') themes.appChanged();
  paintDetailGuides();
}
async function loadOriginals() {
  const r = await call('originals_list');
  S.originals = r.originals; S.longMissing = r.longMissing; S.rev = r.rev; S.currentTheme = r.currentTheme || null; S.embeddedTheme = r.embeddedTheme || null;
  if (S.selTheme && !S.originals.some(o => o.id === S.selTheme)) S.selTheme = null;
  if (S.sel && !S.originals.find(o => o.id === S.sel)) { S.sel = null; S.detail = null; if (S.view === 'detail') setView(S.home); }
  renderLibrary();
  renderBanners();
  renderGallery();
  if (S.view === 'detail') await loadDetail();
  if (S.view === 'screen') themes.enter();
}
let detailFor = null;
async function loadDetail() {
  if (!S.sel) { S.detail = null; detailFor = null; renderDetail(true); return; }
  let d;
  try { d = await call('original_get', { originalId: S.sel }); } catch { d = null; }
  const fresh = detailFor !== S.sel || !S.detail || (S.detail.previewUrl !== (d && d.previewUrl)) || (S.detail.canTune !== (d && d.canTune));
  S.detail = d;
  if (detailFor !== S.sel) {
    S.showing = null;
    S.draft = initialDraft(d);
    S.est = { state: 'idle' };
  }
  detailFor = S.sel;
  renderDetail(fresh);
  if (fresh && d && d.canTune) scheduleEstimate(50);
}

function initialDraft(d) {
  if (!d) return null;
  const max = (d.loops && d.loops.suggestedMax) || (S.app ? S.app.maxSeconds : 10);
  const dr = d.draft || {};
  const dur = d.duration || 0;
  const sf = d.fps || 25, of = S.app ? S.app.deviceFps : 18;
  // Start on a source frame, length a whole number of screen frames.
  const start = d.isImage ? 0 : Math.round(clamp(Number(dr.start) || 0, 0, Math.max(0, dur - 0.05)) * sf) / sf;
  const len0 = d.isImage ? (S.app ? S.app.imageSeconds : 3) : clamp(Number(dr.length) || Math.min(max, dur || max), 1 / of, Math.max(1 / of, Math.min(max, (dur || max) - start)));
  const length = d.isImage ? len0 : Math.max(1, Math.floor(len0 * of + 1e-6)) / of;
  const cd = (S.app && S.app.cornersDefault) || { on: false, r: 80 };
  return { start: Math.round(start * 1000) / 1000, length: Math.round(length * 1000) / 1000, quality: dr.quality ?? 'auto', frame: { ...DEFAULT_FRAME, ...(dr.frame || {}) },
    corners: { on: !!(dr.corners || cd).on, r: (dr.corners || cd).r ?? 80 }, detail: dr.detail || 0, blend: dr.blend || 0, manualLoop: !!dr.manualLoop };
}

async function select(id, showVersion = null) {
  const wasGallery = S.view !== 'detail';
  if (wasGallery) setView('detail');
  if (S.sel !== id || wasGallery) {
    S.sel = id; call("set_focus", { originalId: id, visible: [] }).catch(() => {});
    detailFor = null;
    renderLibrary();
    await loadDetail();
  }
  if (showVersion) showPreview(showVersion);
}

// ---------- library ----------
function renderLibrary() {
  const ul = $('libList');
  ul.textContent = '';
  for (const a of S.adding) {
    ul.append(el('li', { class: 'librow', role: 'option', 'aria-selected': 'false', 'aria-disabled': 'true' },
      el('div', { class: 'thumb skeleton' }), el('div', { class: 'txt' }, el('div', { class: 'name', text: a.name }),
        el('div', { class: 'sub' }, el('span', { class: 'spinner', 'aria-hidden': 'true' }), 'Adding…'))));
  }
  for (const o of S.originals) {
    const sub = el('div', { class: 'sub' });
    if (o.missing) sub.append(el('span', { class: 'chip warn', text: 'File missing' }));
    if (o.assets === 'error' && !o.versionCount) { sub.className = 'sub err'; sub.append('Can\'t read this file'); sub.title = o.assetsError || ''; }
    else if (o.busy) sub.append(el('span', { class: 'spinner', 'aria-hidden': 'true' }), 'Preparing version…');
    else sub.append(o.versionCount ? `${o.versionCount} version${o.versionCount > 1 ? 's' : ''}` : 'No versions yet');
    if (o.onScreen) sub.append(el('span', { class: 'chip accent', text: 'Embedded' }));
    const thumb = o.assets === 'ready' ? el('img', { class: 'thumb', src: backend.mediaUrl(o.id, 'thumb.jpg') + `?r=${encodeURIComponent(o.created)}`, alt: '', loading: 'lazy' })
      : el('div', { class: 'thumb' + (o.assets === 'pending' ? ' skeleton' : '') });
    const li = el('li', { class: 'librow', role: 'option', id: 'opt-' + o.id, 'aria-selected': String(o.id === S.sel), 'data-id': o.id, title: o.name },
      thumb, el('div', { class: 'txt' }, el('div', { class: 'name', text: o.name }), sub));
    li.onclick = () => select(o.id);
    ul.append(li);
  }
  if (S.sel) ul.setAttribute('aria-activedescendant', 'opt-' + S.sel); else ul.removeAttribute('aria-activedescendant');
}
$('libList').addEventListener('keydown', e => {
  const i = S.originals.findIndex(o => o.id === S.sel);
  let n = null;
  if (e.key === 'ArrowDown') n = Math.min(S.originals.length - 1, i + 1);
  else if (e.key === 'ArrowUp') n = Math.max(0, i - 1);
  else if (e.key === 'Home') n = 0;
  else if (e.key === 'End') n = S.originals.length - 1;
  else if (e.key === 'Enter') { e.preventDefault(); const t = document.querySelector('.detail-title'); if (t) t.focus(); return; }
  if (n == null || n < 0) return;
  e.preventDefault();
  const o = S.originals[n];
  select(o.id).then(() => { const r = $('opt-' + o.id); if (r) r.scrollIntoView({ block: 'nearest' }); });
});

// ---------- import ----------
$('addBtn').onclick = () => $('fileInput').click();
$('fileInput').onchange = e => { importFiles([...e.target.files]); e.target.value = ''; };
$('openFolder').onclick = () => call('gallery_open').catch(e => toast(e.message, true));

async function importFiles(files) {
  if (!files.length) { toast('Nothing to add: drop a file from your computer (not a link or text).', true); return; }
  const items = files.map(f => ({ name: f.name, file: f }));
  S.adding.push(...items);
  renderLibrary();
  let lastId = null;
  for (const it of items) {
    try {
      lastId = (await backend.importFile(it.file)).id;
    } catch (e) {
      openDialog({ title: 'Couldn\'t add this file', body: /can't read/i.test(e.message) ? e.message : `CoolerScreen can't read “${it.name}”. It may be damaged or in a format that isn't supported.`, actions: [{ label: 'OK', value: true, kind: 'primary' }] });
    }
    S.adding.splice(S.adding.indexOf(it), 1);
  }
  await loadOriginals();
  if (lastId) {
    S.selTheme = lastId;
    if (S.view !== 'mine' && S.view !== 'detail') goHome('mine'); else renderGallery();
    toast('Added to My themes.');
  }
}
let dragDepth = 0;
window.addEventListener('dragenter', e => { if (e.dataTransfer && [...e.dataTransfer.types].includes('Files')) { dragDepth++; $('dropHint').hidden = false; } });
window.addEventListener('dragleave', () => { dragDepth = Math.max(0, dragDepth - 1); if (!dragDepth) $('dropHint').hidden = true; });
window.addEventListener('dragover', e => e.preventDefault());
window.addEventListener('drop', e => {
  e.preventDefault(); dragDepth = 0; $('dropHint').hidden = true;
  if (!e.dataTransfer || ![...e.dataTransfer.types].includes('Files')) return; // a layer or an item dragged inside the app
  importFiles([...(e.dataTransfer?.files || [])]);
});

// ---------- home views: My clips (local clips) | Browse (the sources ticked there; only sources enabled in Settings > Sources) ----------
function lsGet(k) { try { return localStorage.getItem(k); } catch { return null; } }
function lsSet(k, v) { try { localStorage.setItem(k, v); } catch { /* storage unavailable: the choice just isn't remembered */ } }
const HOME_KEY = 'cs.home';
const NAV = { screen: 'navScreen', mine: 'navMine', browse: 'navBrowse' };
function setView(v) {
  const prev = S.view;
  if (prev !== v && (prev === 'mine' || prev === 'browse')) { SP.end(); S.selTheme = null; BV.sel = null; } // leaving without using it: the screen goes back
  S.view = v;
  if (v !== 'detail') S.home = v;
  for (const [name, id] of Object.entries(NAV)) {
    const b = $(id);
    if (v === name) b.setAttribute('aria-current', 'page'); else b.removeAttribute('aria-current');
    b.classList.toggle('parent', v === 'detail' && S.home === name); // the view the open base settings came from
  }
  $('screenView').hidden = v !== 'screen';
  $('mineView').hidden = v !== 'mine';
  $('browseView').hidden = v !== 'browse';
  document.querySelector('.panes').hidden = v !== 'detail';
  if (v !== 'detail') stopPreviewLoop();
  if (prev === 'screen' && v !== 'screen') themes.flush();
  if (v === 'screen' && prev !== 'screen') themes.enter();
  // Entering a home view is when its order is (re)computed; while you stay there, background updates never reorder cards.
  if (v === 'mine' && prev !== 'mine') freezeMine();
  if (v === 'browse' && prev !== 'browse') freezeBrowse();
  if (v !== 'detail') renderGallery();
}
function goHome(v) { // the user picked a view in the top bar
  S.userNavigated = true;
  lsSet(HOME_KEY, v);
  if (S.view !== v) setView(v);
}
$('navScreen').onclick = () => goHome('screen');
$('navMine').onclick = () => goHome('mine');
$('navBrowse').onclick = () => goHome('browse');
// Back from a theme's base settings to the view they were opened from: Esc, Backspace or Alt+Left (never while typing, or over a
// dialog / the drawer). On a home view, Ctrl+F or / focuses its filter field.
window.addEventListener('keydown', e => {
  if (e.defaultPrevented) return;
  if (document.querySelector('.backdrop') || !$('drawerBack').hidden) return;
  const t = e.target, typing = t && (t.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(t.tagName));
  if (S.view === 'screen') return;
  if (S.view === 'mine' || S.view === 'browse') {
    if ((e.ctrlKey && !e.altKey && (e.key === 'f' || e.key === 'F')) || (!typing && e.key === '/')) {
      e.preventDefault();
      const q = (S.view === 'mine' ? mineUI : browseUI).q;
      if (!q.closest('[hidden]')) { q.focus(); q.select(); }
    }
    return;
  }
  if (S.view !== 'detail') return;
  const back = (e.key === 'ArrowLeft' && e.altKey) || (!typing && !e.altKey && !e.ctrlKey && !e.metaKey && (e.key === 'Escape' || e.key === 'Backspace'));
  if (!back) return;
  if (!typing && t && t.closest && t.closest('.menu, .versions-pop')) return; // those handle Esc themselves
  e.preventDefault();
  showGallery();
});
// Back to the home view the clip was opened from, focusing the card you came from.
function showGallery() {
  setView(S.home);
  let f = null;
  if (S.home === 'mine') f = (S.sel && mineUI.box.querySelector(`.g-card[data-id="${CSS.escape(S.sel)}"]`)) || mineUI.box.querySelector('.g-card');
  else if (S.home === 'screen') f = null;
  else {
    const card = BV.lastKey && browseUI.box.querySelector(`.s-card[data-key="${CSS.escape(BV.lastKey)}"]`);
    f = (card && card.querySelector('button')) || browseUI.box.querySelector('.g-grid button');
  }
  if (f) f.focus();
}
const backLabel = () => (S.home === 'browse' ? '‹ Browse' : S.home === 'mine' ? '‹ My themes' : '‹ On screen');

const B = { sources: [], ticked: ['mine'], desktopApp: false, data: new Map(), jobs: new Map(), loaded: false };
const enabledSources = () => B.sources.filter(s => s.enabled && s.kind !== 'action');
async function loadSources() {
  try {
    const r = await call('sources_list');
    B.sources = r.sources; B.ticked = r.ticked; B.desktopApp = r.desktopApp; B.pluginsDir = r.pluginsDir; B.loaded = true;
  } catch (e) { toast(e.message, true); }
  for (const id of [...B.data.keys()]) if (!enabledSources().some(s => s.id === id)) B.data.delete(id);
  for (const s of enabledSources()) if (B.ticked.includes(s.id) && !B.data.has(s.id)) loadSourcePage(s.id);
  renderGallery();
}
async function setTicked(id, on) {
  B.ticked = on ? [...new Set([...B.ticked, id])] : B.ticked.filter(x => x !== id);
  renderGallery();
  try { B.ticked = (await call('browse_set', { ticked: B.ticked })).ticked; } catch {}
  if (on && id !== 'mine' && !B.data.has(id)) loadSourcePage(id);
}
// The theme servers return pages in an unstable order, so each page is sorted deterministically (newest id first, then
// title) and items already shown never move: later pages are appended, a refresh updates known items in place and puts
// new ones in front.
const byNewest = (x, y) => {
  const nx = /^\d+$/.test(x.id), ny = /^\d+$/.test(y.id);
  if (nx && ny && Number(x.id) !== Number(y.id)) return Number(y.id) - Number(x.id);
  return String(y.id).localeCompare(String(x.id)) || String(x.title).localeCompare(String(y.title));
};
async function loadSourcePage(id, refresh = false) {
  let d = B.data.get(id);
  const fresh = !d;
  if (fresh) { d = { items: [], page: 0, hasMore: true, loading: false, error: null, total: null }; B.data.set(id, d); }
  const reload = refresh && !fresh && d.items.length > 0; // refresh: re-read page 1, keep everything shown
  if (d.loading || (!d.hasMore && !reload)) return;
  d.loading = true; d.error = null; renderGallery();
  try {
    const r = await call('source_items', { sourceId: id, page: reload ? 1 : d.page + 1, refresh });
    const known = new Map(d.items.map(i => [i.id, i]));
    const added = [];
    for (const it of [...r.items].sort(byNewest)) {
      const old = known.get(it.id);
      if (old) Object.assign(old, it);
      else { known.set(it.id, it); added.push(it); }
    }
    if (reload) d.items.unshift(...added); else d.items.push(...added);
    if (!reload) { d.page = r.page; d.hasMore = r.hasMore && r.items.length > 0; }
    d.total = r.total;
  } catch (e) { d.error = e.message; }
  d.loading = false;
  renderGallery();
}
function loadMoreSources() {
  for (const s of enabledSources()) {
    const d = B.data.get(s.id);
    if (B.ticked.includes(s.id) && d && d.hasMore && !d.loading && !d.error) loadSourcePage(s.id);
  }
}
$('browseView').addEventListener('scroll', () => {
  const b = $('browseView');
  if (b.scrollTop + b.clientHeight > b.scrollHeight - 500) loadMoreSources();
}, { passive: true });

async function addFromSource(src, item, { record = false } = {}) {
  let seconds;
  if (record) {
    const input = el('select', { id: 'recSecs', autofocus: true },
      el('option', { value: 'auto', text: 'Automatic (recommended)' }),
      ...[30, 60, 120, 180].map(s => el('option', { value: String(s), text: s < 60 ? `${s} seconds` : `${s / 60} minute${s > 60 ? 's' : ''}` })));
    const ok = await openDialog({ title: `Record “${item.title}”`, body: el('div', {},
      el('p', { text: 'This wallpaper is drawn live, so CoolerScreen records it: it opens in a separate Wallpaper Engine window for the length of the recording, then closes that window. Your desktop wallpaper is not changed.' }),
      el('div', { class: 'field' }, el('label', { for: 'recSecs', text: 'Record for' }), input),
      el('p', { class: 'muted small', text: 'Automatic stops as soon as the recording holds a seamless loop (at least 30 seconds). Wallpapers with random effects like rain or fireflies never repeat exactly, so those record the full 2 minutes and get a cross-fade. The loops are found right after, in the background.' })),
      actions: [{ label: 'Cancel', value: false, kind: 'secondary' }, { label: 'Record', value: true, kind: 'primary' }] });
    if (!ok) return;
    seconds = input.value === 'auto' ? undefined : Number(input.value);
  }
  const key = `${src.id}/${item.id}`;
  const st = { progress: 0, status: record ? 'Starting the recording' : 'Starting' };
  B.jobs.set(key, st);
  renderGallery();
  try {
    const { jobId } = await call('source_add', { sourceId: src.id, itemId: item.id, record, seconds, title: item.title });
    const r = await pollJob(jobId, j => { st.progress = j.progress || 0; st.status = j.status === 'waiting' ? plainWait(j.message) : j.message || 'Working'; updateSourceCard(key); });
    B.jobs.delete(key);
    toast(`“${item.title}” was added to My themes as its own copy.`);
    await loadOriginals();
  } catch (e) { st.error = e.message; }
  renderGallery();
}

async function addFromLink() {
  const input = el('input', { type: 'url', id: 'linkUrl', placeholder: 'https://…/clip.mp4', autofocus: true, spellcheck: false });
  const ok = await openDialog({ title: 'Add from link', body: el('div', {},
    el('div', { class: 'field' }, el('label', { for: 'linkUrl', text: 'Direct link to a video, GIF, image or theme (.ht) file' }), input),
    el('p', { class: 'muted small', text: 'The link must open the file itself (up to 500 MB), not a web page.' })),
    actions: [{ label: 'Cancel', value: false, kind: 'secondary' }, { label: 'Add to My themes', value: true, kind: 'primary' }],
    onOpen: ({ close }) => input.addEventListener('keydown', e => { if (e.key === 'Enter') close(true); }) });
  if (!ok || !input.value.trim()) return;
  try {
    const { jobId } = await call('link_add', { url: input.value.trim() });
    toast('Downloading…');
    const r = await pollJob(jobId, () => {});
    toast(`Added “${r.name}” to My themes.`);
    await loadOriginals();
  } catch (e) { openDialog({ title: 'Couldn\'t add that link', body: e.message, actions: [{ label: 'OK', value: true, kind: 'primary' }] }); }
}

function sourceAction(src, item) {
  const key = `${src.id}/${item.id}`;
  const job = B.jobs.get(key);
  if (job && !job.error) {
    const pct = Math.round(job.progress * 100);
    return el('div', { class: 'th-action' }, el('div', { class: 'progress th-progress', role: 'progressbar', 'aria-label': `Adding ${item.title}`, 'aria-valuemin': '0', 'aria-valuemax': '100', 'aria-valuenow': String(pct) }, el('div', { style: `width:${pct}%` })),
      el('span', { class: 'small muted num one-line th-pct', text: `${job.status} ${pct}%` }));
  }
  if (item.action === 'none') return el('div', { class: 'th-action' }, el('span', { class: 'chip', text: 'Not supported', title: item.note || '' }), el('span', { class: 'small muted one-line', text: item.note || '' }));
  const rec = item.action === 'record';
  const canRec = !rec || B.desktopApp;
  return el('div', { class: 'th-action' },
    rec ? el('span', { class: 'chip warn', text: 'Needs recording' }) : null,
    el('button', { class: 'btn secondary', text: job && job.error ? 'Try again' : rec ? 'Record…' : 'Add to My themes', disabled: !canRec,
      title: canRec ? '' : 'Recording needs the desktop app (CoolerScreen.exe)', onclick: () => addFromSource(src, item, { record: rec }) }),
    job && job.error ? el('span', { class: 'small one-line th-err', text: job.error, title: job.error }) : null);
}
function updateSourceCard(key) {
  const card = document.querySelector(`.s-card[data-key="${CSS.escape(key)}"]`);
  if (!card) return;
  const [sid, ...rest] = key.split('/');
  const src = B.sources.find(s => s.id === sid), d = B.data.get(sid);
  const item = d && d.items.find(i => i.id === rest.join('/'));
  if (src && item) card.querySelector('.th-action').replaceWith(sourceAction(src, item));
}

// One grid per section (My clips has one; Browse has one per ticked source). The grids and their cards are kept between
// renders and reconciled by key, so a card whose state changes (Not added -> Downloading -> In your gallery) is replaced
// in its own <li> and every other card stays exactly where it is: nothing is re-sorted or rebuilt wholesale. The order of
// the cards is decided when you enter a view or change its sort (see freezeMine / freezeBrowse), never by a background
// update; later arrivals are only inserted or appended.
const GRID = new Map(); // section id ('mine' or a source id) -> { wrap, title, note, ul, cache: Map(key -> { li, sig }) }
function gridSection(id) {
  let g = GRID.get(id);
  if (!g) {
    const title = el('h2', { class: 'g-sec' });
    const note = el('div', { class: 'g-none small muted', hidden: true });
    const ul = el('ul', { class: 'g-grid', 'aria-label': 'Clips' });
    g = { wrap: el('section', { class: 'g-section' }, title, note, ul), title, note, ul, cache: new Map() };
    GRID.set(id, g);
  }
  return g;
}
// `retain`: keys that are only filtered out for now; their cards stay cached so clearing the filter doesn't rebuild them.
function reconcile(g, entries, retain = null) {
  const keep = new Set();
  let cur = g.ul.firstChild;
  for (const e of entries) {
    keep.add(e.key);
    let c = g.cache.get(e.key);
    if (!c) { c = { li: el('li'), sig: null }; g.cache.set(e.key, c); }
    if (c.sig !== e.sig) { c.li.textContent = ''; c.li.append(e.make()); c.sig = e.sig; }
    if (c.li === cur) cur = cur.nextSibling; else g.ul.insertBefore(c.li, cur);
  }
  while (cur) { const n = cur.nextSibling; cur.remove(); cur = n; }
  for (const k of [...g.cache.keys()]) if (!keep.has(k) && !(retain && retain.has(k))) g.cache.delete(k);
}
// Keeps the first visible card where it is even if cards above it were added or removed while `fn` re-renders `box`.
function keepScroll(box, fn) {
  const scroll = box.scrollTop;
  const boxTop = box.getBoundingClientRect().top;
  const idOf = c => c.dataset.key || c.dataset.id;
  const anchorEl = scroll > 0 && [...box.querySelectorAll('.g-card')].find(c => c.getBoundingClientRect().bottom > boxTop + 4 && idOf(c));
  const anchor = anchorEl ? { key: idOf(anchorEl), top: anchorEl.getBoundingClientRect().top } : null;
  fn();
  box.scrollTop = scroll;
  if (anchor) { const a2 = [...box.querySelectorAll('.g-card')].find(c => idOf(c) === anchor.key); if (a2) box.scrollTop += a2.getBoundingClientRect().top - anchor.top; }
}
const addingIds = new WeakMap();
let addingSeq = 0;
const matches = (text, q) => !q || String(text || '').toLowerCase().includes(q);
const byName = (a, b) => String(a || '').localeCompare(String(b || ''), undefined, { numeric: true, sensitivity: 'base' });

function renderGallery() {
  if (S.view === 'mine') renderMine();
  else if (S.view === 'browse') renderBrowse();
}

// ----- My clips -----
const MINE_SORTS = [['added', 'Recently added'], ['name', 'Name (A–Z)'], ['longest', 'Longest first'], ['shortest', 'Shortest first'], ['onscreen', 'Embedded first']];
const KIND_NAMES = { clock: 'clock', music: 'music', stats: 'stats', text: 'text', image: 'picture', weather: 'weather' };
const themeLabel = o => {
  const w = (o.theme && o.theme.widgets) || [];
  return w.length ? 'Picture + live: ' + [...new Set(w)].map(k => KIND_NAMES[k] || k).join(' · ') : 'Picture only';
};
const M = { q: '', sort: MINE_SORTS.some(([v]) => v === lsGet('cs.mineSort')) ? lsGet('cs.mineSort') : 'added', order: [] };
function mineCmp(sort) {
  const name = (a, b) => byName(a.name, b.name);
  const added = (a, b) => String(b.created || '').localeCompare(String(a.created || '')) || name(a, b);
  const dur = o => (o.isImage || !Number.isFinite(o.duration) ? null : o.duration);
  const byDur = sign => (a, b) => {
    const x = dur(a), y = dur(b);
    if (x == null || y == null) return (x == null) - (y == null) || name(a, b); // pictures (no length) last
    return sign * (x - y) || name(a, b);
  };
  if (sort === 'name') return name;
  if (sort === 'longest') return byDur(-1);
  if (sort === 'shortest') return byDur(1);
  if (sort === 'onscreen') return (a, b) => (!!b.onScreen - !!a.onScreen) || ((b.versionCount > 0) - (a.versionCount > 0)) || added(a, b);
  return added;
}
function freezeMine() { M.order = [...S.originals].sort(mineCmp(M.sort)).map(o => o.id); }
// The clips in the frozen order; a clip that arrived since is inserted where the sort puts it, without moving the others.
function mineOrdered() {
  const byId = new Map(S.originals.map(o => [o.id, o]));
  M.order = M.order.filter(id => byId.has(id));
  const placed = new Set(M.order);
  if (S.originals.some(o => !placed.has(o.id))) {
    const sorted = [...S.originals].sort(mineCmp(M.sort)).map(o => o.id);
    sorted.forEach((id, i) => {
      if (placed.has(id)) return;
      let at = M.order.length;
      for (let j = i + 1; j < sorted.length; j++) if (placed.has(sorted[j])) { at = M.order.indexOf(sorted[j]); break; }
      M.order.splice(at, 0, id);
      placed.add(id);
    });
  }
  return M.order.map(id => byId.get(id));
}
const mineUI = (() => {
  const box = $('mineView');
  const count = el('span', { class: 'muted num g-count' });
  const bar = el('div', { class: 'sel-bar', hidden: true, role: 'region', 'aria-label': 'Selected theme' });
  const linkBtn = el('button', { class: 'btn secondary', text: 'Add from link…', onclick: addFromLink, hidden: true });
  const head = el('div', { class: 'g-head' }, el('h1', { class: 'g-title', id: 'mineTitle', text: 'My themes' }), count, el('span', { class: 'grow' }),
    el('div', { class: 'g-actions' },
      el('button', { class: 'btn secondary', text: 'Add file…', onclick: () => $('fileInput').click() }), linkBtn,
      el('button', { class: 'btn ghost', text: 'Open Gallery folder', onclick: () => $('openFolder').click() })));
  const q = el('input', { type: 'search', id: 'mineQ', class: 'g-filter', placeholder: 'Filter by name', autocomplete: 'off' });
  q.spellcheck = false;
  q.addEventListener('input', () => { M.q = q.value; renderGallery(); });
  const sort = el('select', { id: 'mineSort' }, MINE_SORTS.map(([value, text]) => el('option', { value, text })));
  sort.value = M.sort;
  sort.onchange = () => { M.sort = sort.value; lsSet('cs.mineSort', M.sort); freezeMine(); renderGallery(); };
  const tools = el('div', { class: 'g-tools', role: 'search', 'aria-label': 'Filter and sort my themes' },
    el('label', { class: 'g-tool g-tool-q' }, el('span', { class: 'sr-only', text: 'Filter my themes by name' }), q),
    el('label', { class: 'g-tool' }, el('span', { class: 'muted small', text: 'Sort' }), sort));
  const body = el('div');
  box.append(head, tools, body, bar);
  return { box, count, linkBtn, q, tools, body, bar };
})();
function renderMine() {
  const u = mineUI;
  if (u.box.hidden) return;
  keepScroll(u.box, () => {
    u.body.textContent = '';
    u.linkBtn.hidden = !B.sources.some(s => s.id === 'link' && s.enabled);
    const all = mineOrdered();
    const q = M.q.trim().toLowerCase();
    const list = q ? all.filter(o => matches(o.name, q) || matches(o.fileName, q)) : all;
    u.count.textContent = !all.length ? '' : q ? `${list.length} of ${all.length}` : `${all.length} theme${all.length > 1 ? 's' : ''}`;
    u.tools.hidden = all.length < 2 && !q; // nothing to sort or filter yet
    if (!all.length && !S.adding.length) u.body.append(el('p', { class: 'muted g-empty' }, 'Add a video, GIF, image or LOVINGCOOL theme (.ht). Every item here is a theme. ',
      'Or ', el('button', { class: 'btn ghost inline-btn', text: 'browse your sources', onclick: () => goHome('browse') }), '.'));
    const entries = [];
    if (!q) entries.push({ key: 'add-tile', sig: 'x', make: () => el('button', { class: 'g-card g-add', onclick: () => $('fileInput').click() },
      (t => { t.innerHTML = icon('plus', 32); return t; })(el('div', { class: 'g-thumb g-add-thumb', 'aria-hidden': 'true' })),
      el('div', { class: 'g-name', text: 'Add a file' }), el('div', { class: 'small muted', text: 'or drop files anywhere' })) });
    for (const a of S.adding) {
      if (!addingIds.has(a)) addingIds.set(a, ++addingSeq);
      entries.push({ key: 'adding:' + addingIds.get(a), sig: a.name, make: () => el('div', { class: 'g-card adding', 'aria-busy': 'true' }, el('div', { class: 'g-thumb skeleton' }),
        el('div', { class: 'g-name', text: a.name }), el('div', { class: 'g-chips' }, el('span', { class: 'spinner', 'aria-hidden': 'true' }), el('span', { class: 'small muted', text: 'Adding…' }))) });
    }
    for (const o of list) entries.push({ key: 'o:' + o.id, sig: JSON.stringify([o, S.selTheme === o.id]), make: () => themeCard(o) });
    const g = gridSection('mine');
    g.title.hidden = true;
    g.note.hidden = !(q && !list.length);
    g.note.textContent = '';
    if (q && !list.length) g.note.append(`No themes match “${M.q.trim()}”. `, el('button', { class: 'btn ghost inline-btn', text: 'Clear filter', onclick: () => { u.q.value = ''; M.q = ''; renderGallery(); u.q.focus(); } }));
    reconcile(g, entries, new Set(all.map(o => 'o:' + o.id)));
    u.body.append(g.wrap);
    renderSelBar();
  });
}

// The selected theme's actions. Live on: "Use this theme" (live switches now; Embedded is not touched) and "Store as Embedded".
// Live off: one action, "Use and store as Embedded".
function liveOn() { return !!(S.app && S.app.live && S.app.live.settings && S.app.live.settings.inApp); }
function renderSelBar() {
  const bar = mineUI.bar, o = S.originals.find(x => x.id === S.selTheme);
  bar.hidden = !o;
  bar.textContent = '';
  if (!o) return;
  const isLive = S.currentTheme === o.id, isEmb = S.embeddedTheme === o.id;
  const off = o.missing && !o.versionCount;
  const kids = [el('span', { class: 'sel-name', text: o.name, title: o.name })];
  if (isLive) kids.push(el('span', { class: 'chip ok', text: 'Live' }));
  if (isEmb) kids.push(el('span', { class: 'chip accent', text: 'Embedded' }));
  kids.push(el('span', { class: 'small muted', text: themeLabel(o) }));
  kids.push(el('button', { class: 'btn primary big', text: isLive ? 'Using' : 'Use this theme', disabled: off || isLive, title: 'Make it the theme on the screen, then edit it on On screen', onclick: () => useTheme(o.id, o.name) }));
  if (SP.key() === 't:' + o.id) kids.splice(1, 0, el('span', { class: 'chip accent pv-chip', text: SP.shown() ? 'Previewing on the screen' : 'Starting preview…' }));
  else if (!off) kids.splice(1, 0, el('button', { class: 'btn ghost', text: 'Preview', title: 'Show it on the screen', onclick: () => SP.showTheme(o.id) }));
  kids.push(el('button', { class: 'btn secondary', text: 'Picture…', title: 'Choose its picture: any clip, its loop, quality and versions', onclick: () => openBase(o.id, { name: o.name }) }),
    el('button', { class: 'btn secondary', text: 'Rename…', onclick: () => renameTheme(o) }),
    el('button', { class: 'btn danger', text: o.missing ? 'Remove…' : 'Delete…', onclick: () => deleteTheme(o) }));
  bar.append(...kids);
}
async function renameTheme(o) {
  const input = el('input', { type: 'text', value: o.name, id: 'thRename', autofocus: true, maxLength: 80 });
  const err = el('div', { class: 'field-err', role: 'alert' });
  const v = await openDialog({ title: 'Rename theme', body: el('div', {}, el('div', { class: 'field' }, el('label', { for: 'thRename', text: 'Name' }), input), err),
    actions: [{ label: 'Cancel', value: null, kind: 'secondary' }, { label: 'Rename', value: 'save', kind: 'primary' }],
    onOpen: ({ close }) => input.addEventListener('keydown', e => { if (e.key === 'Enter') close('save'); }) });
  if (v !== 'save' || !input.value.trim() || input.value.trim() === o.name) return;
  try { await call('original_patch', { originalId: o.id, name: input.value.trim() }); await loadOriginals(); } catch (e) { toast(e.message, true); }
}
async function deleteTheme(o) {
  try { S.detail = await call('original_get', { originalId: o.id }); S.sel = o.id; } catch (e) { toast(e.message, true); return; }
  await confirmDeleteOriginal();
  S.detail = null; S.sel = null; detailFor = null;
}

// ----- Browse -----
// Adding from Browse makes an independent copy in My themes (no link back, no "already added"). The Name sort is applied
// when you enter Browse, change Sort or press Refresh.
const BROWSE_SORTS = [['default', 'Source order'], ['name', 'Name (A–Z)']];
const pick = (list, v, dflt) => (list.some(([x]) => x === v) ? v : dflt);
const BV = { sel: null, q: '', sort: pick(BROWSE_SORTS, lsGet('cs.browseSort'), 'default'), frozen: new Map(), lastKey: null, refreshing: false };
function arrangeBrowse(items) {
  const list = [...items];
  if (BV.sort === 'name') list.sort((a, b) => byName(a.title, b.title));
  return list;
}
function freezeBrowse() {
  BV.frozen.clear();
  for (const [sid, d] of B.data) if (d.items.length) BV.frozen.set(sid, arrangeBrowse(d.items).map(i => i.id));
}
// A source's items in the frozen order; items loaded since (next pages, refresh) are appended, arranged among themselves.
function browseOrdered(sid, d) {
  let fo = BV.frozen.get(sid);
  if (!fo) { fo = []; BV.frozen.set(sid, fo); }
  const placed = new Set(fo);
  const fresh = d.items.filter(i => !placed.has(i.id));
  if (fresh.length) fo.push(...arrangeBrowse(fresh).map(i => i.id));
  const byId = new Map(d.items.map(i => [i.id, i]));
  return fo.filter(id => byId.has(id)).map(id => byId.get(id));
}
async function refreshBrowse() {
  if (BV.refreshing) return;
  BV.refreshing = true; renderGallery();
  const ids = enabledSources().filter(s => B.ticked.includes(s.id)).map(s => s.id);
  await Promise.all(ids.map(id => loadSourcePage(id, true)));
  BV.refreshing = false;
  if (S.view === 'browse') freezeBrowse();
  renderGallery();
}
const browseUI = (() => {
  const box = $('browseView');
  const linkBtn = el('button', { class: 'btn secondary', text: 'Add from link…', onclick: addFromLink, hidden: true });
  const refresh = el('button', { class: 'btn ghost', text: 'Refresh', title: 'Check the sources for new clips', onclick: refreshBrowse });
  const head = el('div', { class: 'g-head' }, el('h1', { class: 'g-title', id: 'browseTitle', text: 'Browse' }), el('span', { class: 'grow' }),
    el('div', { class: 'g-actions' }, linkBtn, refresh));
  const chips = el('div', { class: 'src-chips', role: 'group', 'aria-label': 'Show clips from' });
  const note = el('p', { class: 'muted small browse-note', text: 'Adding makes your own copy in My themes.' });
  const q = el('input', { type: 'search', id: 'browseQ', class: 'g-filter', placeholder: 'Filter by title', autocomplete: 'off' });
  q.spellcheck = false;
  q.addEventListener('input', () => { BV.q = q.value; renderGallery(); });
  const sort = el('select', { id: 'browseSort' }, BROWSE_SORTS.map(([value, text]) => el('option', { value, text })));
  sort.value = BV.sort;
  sort.onchange = () => { BV.sort = sort.value; lsSet('cs.browseSort', BV.sort); freezeBrowse(); renderGallery(); };
  const tools = el('div', { class: 'g-tools', role: 'search', 'aria-label': 'Filter and sort Browse' },
    el('label', { class: 'g-tool g-tool-q' }, el('span', { class: 'sr-only', text: 'Filter Browse by title' }), q),
    el('label', { class: 'g-tool' }, el('span', { class: 'muted small', text: 'Sort' }), sort));
  const body = el('div');
  const bar = el('div', { class: 'sel-bar', hidden: true, role: 'region', 'aria-label': 'Previewing on the screen' });
  box.append(head, note, chips, tools, body, bar);
  return { box, linkBtn, refresh, note, chips, q, tools, body, bar };
})();
// Browse: the selected item is shown on the real screen (its picture); nothing is added until you press Add.
function renderBrowsePreviewBar() {
  const bar = browseUI.bar;
  const [sid, ...rest] = (BV.sel || '').split('/');
  const d = B.data.get(sid), it = d && d.items.find(i => i.id === rest.join('/'));
  bar.hidden = !it || S.view !== 'browse';
  bar.textContent = '';
  if (bar.hidden) return;
  bar.append(el('span', { class: 'sel-name', text: it.title, title: it.title }),
    el('span', { class: 'chip accent pv-chip', text: SP.key() === 's:' + BV.sel ? (SP.shown() ? 'Previewing on the screen' : 'Starting preview…') : 'Preview off' }),
    el('button', { class: 'btn ghost', text: SP.key() === 's:' + BV.sel ? 'Stop' : 'Preview', onclick: () => { if (SP.key() === 's:' + BV.sel) SP.end(); else SP.showPicture(sid, it.id, it.thumb); } }));
}
function renderBrowse() {
  const u = browseUI;
  if (u.box.hidden) return;
  keepScroll(u.box, () => {
    u.linkBtn.hidden = !B.sources.some(s => s.id === 'link' && s.enabled);
    u.refresh.disabled = BV.refreshing;
    u.refresh.hidden = !enabledSources().length; u.note.hidden = !enabledSources().length;
    u.refresh.textContent = BV.refreshing ? 'Refreshing…' : 'Refresh';
    // source chips (only sources enabled in Settings); a chip that had focus keeps it across the rebuild
    const focused = document.activeElement && document.activeElement.dataset && document.activeElement.dataset.src;
    u.chips.textContent = '';
    const srcs = enabledSources();
    for (const s of srcs) {
      const d = B.data.get(s.id), on = B.ticked.includes(s.id);
      u.chips.append(el('label', { class: 'src-chip' + (on ? ' on' : '') },
        el('input', { type: 'checkbox', checked: on, 'data-src': s.id, onchange: e => setTicked(s.id, e.target.checked) }), s.name,
        d && d.total != null ? el('span', { class: 'muted num', text: ` ${d.total}` }) : null));
    }
    if (srcs.length) u.chips.append(el('button', { class: 'btn ghost small-btn', text: 'Sources…', title: 'Settings > Sources', onclick: () => openDrawer('setSources') })); // with none, the note below offers it
    if (focused) { const f = u.chips.querySelector(`input[data-src="${CSS.escape(focused)}"]`); if (f) f.focus(); }

    u.body.textContent = '';
    const ticked = srcs.filter(s => B.ticked.includes(s.id));
    u.tools.hidden = !ticked.length;
    if (!B.loaded) return;
    if (!srcs.length) {
      u.body.append(el('p', { class: 'muted g-empty' }, 'Themes and clips from Wallpaper Engine, Lively or the LOVINGCOOL library. ',
        el('button', { class: 'btn ghost inline-btn', text: 'Turn on sources…', onclick: () => openDrawer('setSources') })));
      return;
    }
    if (!ticked.length) { u.body.append(el('p', { class: 'muted g-empty', text: 'Tick a source above to see its clips.' })); return; }
    const q = BV.q.trim().toLowerCase();
    const filtering = !!q;
    const shown = [], notes = [];
    for (const s of ticked) {
      const d = B.data.get(s.id);
      if (!d) continue;
      const all = browseOrdered(s.id, d);
      const list = all.filter(it => matches(it.title, q));
      const entries = list.map(it => {
        const job = B.jobs.get(`${s.id}/${it.id}`);
        return { key: `${s.id}/${it.id}`, sig: JSON.stringify([it.title, it.thumb, it.badge, it.action, it.note, job ? (job.error ? 'e:' + job.error : 'run') : '', BV.sel === `${s.id}/${it.id}`]),
          make: () => {
            const thumb = el('div', { class: 'g-thumb' }, it.thumb ? el('img', { src: it.thumb, alt: '', loading: 'lazy', referrerPolicy: 'no-referrer' }) : null,
              el('span', { class: 'src-badge', text: it.badge ? `${s.name} · ${it.badge}` : s.name }));
            const k = `${s.id}/${it.id}`;
            const card = el('div', { class: 'g-card s-card' + (BV.sel === k ? ' sel' : ''), 'data-key': k, 'aria-pressed': String(BV.sel === k), title: 'Click to preview it on the screen',
              onclick: e => {
                if (e.target.closest('button, a, input')) return; // the card's own buttons do their own thing
                if (BV.sel === k) { BV.sel = null; SP.end(); } else { BV.sel = k; SP.showPicture(s.id, it.id, it.thumb); }
                renderBrowse(); renderBrowsePreviewBar();
              } }, thumb,
              el('div', { class: 'g-name', text: it.title, title: it.title }), sourceAction(s, it));
            return card;
          } };
      });
      if (d.loading) for (let i = 0; i < 3; i++) entries.push({ key: `skel:${i}`, sig: 's', make: () => el('div', { class: 'g-card', 'aria-hidden': 'true' }, el('div', { class: 'g-thumb skeleton' }), el('div', { class: 'g-name', text: ' ' }), el('div', { class: 'th-action' })) });
      const g = gridSection(s.id);
      reconcile(g, entries, new Set(all.map(it => `${s.id}/${it.id}`)));
      // filtered: say when nothing matches, and offer to look further when the source has more pages
      g.note.textContent = '';
      if (filtering && !d.loading && d.items.length) {
        if (!list.length) g.note.append(d.hasMore ? `Nothing matches among the ${d.items.length} loaded so far. ` : 'Nothing here matches. ');
        if (d.hasMore && !d.error) g.note.append(el('button', { class: 'btn ghost inline-btn', text: 'Load more', onclick: () => loadSourcePage(s.id) }));
      }
      g.note.hidden = !g.note.childNodes.length;
      shown.push({ id: s.id, name: s.name, g });
      if (d.error) notes.push(el('div', { class: 'notice danger' }, `${s.name}: ${d.error} `, el('button', { class: 'btn secondary', text: 'Try again', onclick: () => { d.error = null; loadSourcePage(s.id, !d.items.length); } })));
      if (s.id === 'lovingcool' || s.id === 'otherbrands') notes.push(el('p', { class: 'muted small', text: `${s.name}: the vendor's online library; the themes stay theirs.` }));
    }
    for (const k of [...GRID.keys()]) if (k !== 'mine' && !shown.some(x => x.id === k)) GRID.delete(k);
    for (const x of shown) {
      x.g.title.textContent = x.name;
      x.g.title.hidden = shown.length < 2; // headings only when several sections are shown
      u.body.append(x.g.wrap);
    }
    u.body.append(...notes);
  });
  renderBrowsePreviewBar();
}

// A theme card: the picture shows the base with its live layer (when it has one), the line below says which.
function themeCard(o) {
  const t = o.theme || {};
  const pic = (t.card || o.frameUrl);
  const thumb = el('div', { class: 'g-thumb' + (o.assets === 'pending' ? ' skeleton' : '') }, pic ? el('img', { src: pic, alt: '', loading: 'lazy' }) : null);
  const chips = el('div', { class: 'g-chips' });
  if (S.currentTheme === o.id) chips.append(el('span', { class: 'chip ok', text: 'Live' }));
  if (S.embeddedTheme === o.id) chips.append(el('span', { class: 'chip accent', text: 'Embedded' }));
  if (o.missing) chips.append(el('span', { class: 'chip warn', text: 'File missing' }));
  if (o.needsRemake) chips.append(el('span', { class: 'chip warn', text: 'Needs re-make' }));
  if (o.busy) chips.append(el('span', { class: 'chip accent', text: 'Preparing…' }));
  if (o.assets === 'error' && !o.versionCount) chips.append(el('span', { class: 'chip danger', text: 'Can\'t read this file' }));
  const sel = S.selTheme === o.id;
  const card = el('button', { class: 'g-card' + (sel ? ' sel' : ''), 'data-id': o.id, 'aria-pressed': String(sel), 'aria-label': `${o.name}, ${themeLabel(o)}${S.currentTheme === o.id ? ', live' : ''}${S.embeddedTheme === o.id ? ', embedded' : ''}`,
    onclick: () => { S.selTheme = S.selTheme === o.id ? null : o.id; if (S.selTheme && !(o.missing && !o.versionCount)) SP.showTheme(o.id); else SP.end(); renderGallery(); const c = mineUI.box.querySelector(`.g-card[data-id="${CSS.escape(o.id)}"]`); if (c) c.focus({ preventScroll: true }); } },
    thumb, el('div', { class: 'g-name', text: o.name, title: o.name }), el('div', { class: 'g-sub', text: themeLabel(o) }), chips);
  let v = null; // hover (or keyboard focus) plays a muted preview
  const play = () => {
    if (v || !o.previewUrl || o.isImage) return;
    v = el('video', { src: o.previewUrl, muted: true, loop: true, autoplay: true, playsInline: true, 'aria-hidden': 'true' });
    v.muted = true;
    thumb.append(v);
    v.play().catch(() => {});
  };
  const stop = () => { if (v) { v.remove(); v = null; } };
  card.addEventListener('mouseenter', play); card.addEventListener('mouseleave', stop);
  card.addEventListener('focus', play); card.addEventListener('blur', stop);
  card.addEventListener('dblclick', () => useTheme(o.id, o.name));
  return card;
}

// ---------- Settings > Sources ----------
async function renderSourcesSettings() {
  const box = $('srcList');
  if (!B.loaded) await loadSources();
  box.textContent = '';
  for (const s of B.sources) {
    const toggle = el('input', { type: 'checkbox', id: 'src-' + s.id, checked: s.enabled, role: 'switch' });
    toggle.onchange = async () => {
      if (toggle.checked && s.kind === 'plugin') {
        const ok = await openDialog({ title: 'Enable this plug-in?', body: s.warning || 'Plug-ins run code on your PC.', actions: [{ label: 'Cancel', value: false, kind: 'secondary' }, { label: 'Enable', value: true, kind: 'danger' }] });
        if (!ok) { toggle.checked = false; return; }
      }
      try { await call('source_set', { sourceId: s.id, enabled: toggle.checked }); } catch (e) { toast(e.message, true); }
      B.data.delete(s.id);
      await loadSources();
      renderSourcesSettings();
    };
    const row = el('div', { class: 'src-row' },
      el('div', { class: 'src-top' }, el('label', { for: 'src-' + s.id, class: 'src-name', text: s.name }), toggle),
      el('p', { class: 'muted small', text: s.description || '' }),
      el('p', { class: 'small ' + (s.available === false ? 'th-err' : 'muted'), text: s.enabled || s.kind !== 'plugin' ? (s.status || '') : 'Not loaded (disabled)' }),
      s.warning ? el('p', { class: 'small warn-text', text: '⚠ ' + s.warning }) : null);
    for (const opt of s.options || []) {
      if (opt.type !== 'folders') continue;
      const ta = el('textarea', { id: `opt-${s.id}-${opt.key}`, rows: 2, spellcheck: false, value: ((s.values || {})[opt.key] || []).join('\n') });
      ta.value = ((s.values || {})[opt.key] || []).join('\n');
      // kept as soon as the box is left
      ta.onchange = async () => {
        await call('source_set', { sourceId: s.id, options: { [opt.key]: ta.value.split(/\r?\n/).map(x => x.trim()).filter(Boolean) } }).catch(e => toast(e.message, true));
        B.data.delete(s.id); await loadSources();
      };
      row.append(el('div', { class: 'field' }, el('label', { for: ta.id, text: opt.label }), ta));
    }
    box.append(row);
  }
  box.append(el('p', { class: 'muted small', text: `Plug-in sources: put .js files in ${B.pluginsDir || 'Documents\\CoolerScreen\\Sources'} (see docs/sources.md). They appear here, off until you enable them.` }));
}

// ---------- detail ----------
let refs = {}; // elements of the current detail view
function renderDetail(full) {
  const main = $('detail');
  const d = S.detail;
  if (!d) {
    stopPreviewLoop();
    refs = {};
    main.textContent = '';
    return;
  }
  if (full || !refs.root) buildDetail(d);
  updateDetail(d);
}

function buildDetail(d) {
  stopPreviewLoop();
  const main = $('detail');
  main.textContent = '';
  refs = { root: main }; // root marks a built view: later updates re-render parts, not the whole view
  // header: back to Gallery · title · Rename · Versions ▾ · ⋯
  refs.title = el('h1', { class: 'detail-title', tabindex: '-1' });
  refs.meta = el('div', { class: 'meta' });
  refs.titles = el('div', { class: 'titles' }, refs.title, refs.meta);
  refs.renameBtn = el('button', { class: 'btn secondary', text: 'Rename', onclick: startRename });
  refs.more = menuButton('More actions for this clip', () => {
    const o = S.detail;
    const items = [];
    if (o.file && !o.missing) items.push({ label: 'Show in Gallery folder', run: () => call('original_reveal', { originalId: o.id }) });
    if (!o.isImage && o.canTune) items.push({ label: S.manualLoop ? 'Hide manual loop editing' : 'Edit loop manually…', run: () => { S.manualLoop = !S.manualLoop; buildTune(S.detail); } });
    items.push({ label: o.missing ? 'Remove from library…' : 'Delete…', danger: true, run: confirmDeleteOriginal });
    return items;
  });
  refs.versionsBtn = el('button', { class: 'btn secondary', 'aria-haspopup': 'dialog', 'aria-expanded': 'false', onclick: toggleVersions });
  refs.versions = el('div', { class: 'versions-pop', role: 'dialog', 'aria-label': 'Versions', hidden: true });
  refs.versions.addEventListener('keydown', e => { if (e.key === 'Escape' && !e.defaultPrevented) { e.stopPropagation(); closeVersions(true); } });
  main.append(el('nav', { class: 'crumbs', 'aria-label': 'Breadcrumb' },
    el('button', { class: 'btn ghost back', text: backLabel(), onclick: showGallery }), el('span', { class: 'muted', 'aria-hidden': 'true', text: '/' }),
    el('span', { class: 'muted small crumb-name', text: d.name })));
  refs.ctxBar = el('div', { class: 'ctx-bar', role: 'region', 'aria-label': 'Choosing the picture' });
  main.append(refs.ctxBar);
  main.append(el('div', { class: 'detail-head' }, refs.titles, refs.renameBtn,
    el('div', { class: 'menu-wrap' }, refs.versionsBtn, refs.versions), refs.more));
  refs.notices = el('div');
  main.append(refs.notices);

  // preview + read-only timeline of the chosen loop
  refs.stage = el('div', { class: 'screenframe', tabindex: '0', role: 'group', 'aria-label': 'Preview of the screen. Space plays or pauses.' });
  refs.notch = el('div', { class: 'notch', hidden: true, title: 'Your screen\'s camera hole is here. This area is kept black.' });
  refs.corners = makeCornersOverlay();
  refs.playBtn = el('button', { class: 'iconbtn', 'aria-label': 'Pause', onclick: togglePlay }, '❚❚');
  refs.time = el('span', { class: 'num time-readout' });
  refs.showing = el('span', { class: 'grow one-line' });
  refs.notchToggle = el('input', { type: 'checkbox', id: 'notchOutline' });
  refs.notchToggle.onchange = () => refs.stage.classList.toggle('outline', refs.notchToggle.checked);
  refs.backToTune = el('button', { class: 'btn ghost', text: 'Back to your tuning', hidden: true, onclick: () => showPreview(null) });
  refs.tlRange = el('div', { class: 'tl-range' });
  refs.tlHead = el('div', { class: 'tl-head' });
  refs.timeline = el('div', { class: 'timeline', role: 'img', hidden: d.isImage }, refs.tlRange, refs.tlHead);
  main.append(el('div', { class: 'preview-wrap' }, refs.stage, refs.timeline,
    el('div', { class: 'preview-bar' }, refs.playBtn, refs.time, refs.showing, refs.backToTune,
      el('label', { class: 'check', title: 'Your screen\'s camera hole is here. This area is kept black.' }, refs.notchToggle, 'Show notch'))));
  refs.stage.addEventListener('keydown', e => { if (e.key === ' ') { e.preventDefault(); togglePlay(); } });
  setupPan();

  refs.tune = el('section', { class: 'panel tune-panel', 'aria-labelledby': 'hTune' });
  main.append(refs.tune);
  // bottom action bar
  refs.saveBtn = el('button', { class: 'btn secondary big', text: 'Make a version', onclick: saveVersion });
  refs.pushBtn = el('button', { class: 'btn primary big', text: 'Use', title: 'Nothing is written to the screen', onclick: pushToScreen });
  refs.saveReason = el('span', { class: 'reason', role: 'status' });
  refs.saveFix = el('button', { class: 'btn secondary', hidden: true });
  refs.actionBar = el('div', { class: 'action-bar' }, refs.saveReason, refs.saveFix, el('span', { class: 'grow' }), refs.saveBtn, refs.pushBtn);
  main.append(refs.actionBar);
  buildTune(d);
  showPreview(S.showing, true);
  maybeAutoSearchLoops(d);
}

function updateDetail(d) {
  refs.title.textContent = d.name;
  const parts = [d.fileName || ''];
  parts.push(d.isImage ? 'Still image' : d.duration ? fmtTime(d.duration, false) : '');
  if (d.width) parts.push(`${d.width}x${d.height}`);
  refs.meta.textContent = parts.filter(Boolean).join(' · ');
  refs.renameBtn.disabled = false;
  const n = d.versions.length, busy = d.versions.some(v => v.status === 'queued' || v.status === 'converting');
  // same width whether or not a version is being prepared: the spinner slot is always there
  refs.versionsBtn.replaceChildren(el('span', { class: 'spinner' + (busy ? '' : ' invisible'), 'aria-hidden': 'true' }), `Versions (${n}) ▾`);
  refs.versionsBtn.setAttribute('aria-label', `Versions (${n})${busy ? ', one is being prepared' : ''}`);
  refs.notices.textContent = '';
  if (d.missing) refs.notices.append(el('div', { class: 'notice' },
    `The file is gone from the Gallery folder (since ${fmtDate(d.missing.since)}). Its versions are kept` +
    (d.canTune ? '.' : '; new ones need the file back.')));
  else if (d.sourceChanged) refs.notices.append(el('div', { class: 'notice info', text: 'The file changed since its versions were made. New versions use the changed file.' }));
  if (d.assets === 'error') refs.notices.append(el('div', { class: 'notice danger' }, 'CoolerScreen can\'t read this file. It may be damaged or in a format that isn\'t supported.',
    el('details', {}, el('summary', { text: 'Show details' }), el('pre', { text: d.assetsError || '' }))));
  renderCtxBar();
  renderVersions(d);
  renderLoopCards();
  updateTimeline();
  updateTuneState();
  paintDetailGuides();
  if (!d.canTune) { refs.saveBtn.disabled = true; refs.pushBtn.disabled = true; }
  updateShowingLabel();
}

// ----- versions dropdown -----
let versionsOutside = null;
function toggleVersions() { if (refs.versions.hidden) openVersions(); else closeVersions(true); }
function openVersions() {
  refs.versions.hidden = false;
  refs.versionsBtn.setAttribute('aria-expanded', 'true');
  renderVersions(S.detail);
  const f = refs.versions.querySelector('button');
  if (f) f.focus();
  versionsOutside = e => { if (!refs.versions.contains(e.target) && e.target !== refs.versionsBtn) closeVersions(false); };
  document.addEventListener('mousedown', versionsOutside, true);
}
function closeVersions(focusBtn) {
  if (!refs.versions || refs.versions.hidden) return;
  refs.versions.hidden = true;
  refs.versionsBtn.setAttribute('aria-expanded', 'false');
  document.removeEventListener('mousedown', versionsOutside, true);
  if (focusBtn) refs.versionsBtn.focus();
}

// ----- the timeline under the preview (read-only: the loop is chosen with the cards) -----
function updateTimeline() {
  const d = S.detail, dr = S.draft;
  if (!refs.timeline || !d || d.isImage || !d.duration || !dr) return;
  refs.tlRange.style.left = (dr.start / d.duration) * 100 + '%';
  refs.tlRange.style.width = (dr.length / d.duration) * 100 + '%';
  refs.timeline.setAttribute('aria-label', `The loop: ${fmtT2(dr.start)} to ${fmtT2(dr.start + dr.length)} of ${fmtT2(d.duration)}`);
}

// ----- loop cards -----
// How a loop's join looks: a hard cut like a normal frame step, or a cross-fade (blend) that hides it. A fade over a
// moving scene shows a brief double image (ghost = share of the picture), so it only counts as seamless when small.
const seamKind = c => {
  const r = c.ratio, fade = (c.blend || 0) > 0;
  if (!fade && r <= 1.05) return 'seamless';
  if (fade && r <= 1.35) return (c.ghost || 0) <= 0.03 ? 'fade' : 'soft';
  return r <= 1.6 ? 'nearly' : 'jump';
};
const SEAM_TEXT = { seamless: 'Seamless', fade: 'Seamless fade', soft: 'Soft fade', nearly: 'Nearly seamless', jump: 'Small jump' };
const SEAM_CHIP = { seamless: 'ok', fade: 'ok', soft: 'accent', nearly: 'accent', jump: 'warn' };
const SMOOTH = c => SEAM_TEXT[seamKind(c)];
const loopJobs = new Map(); // originalId -> running search
function sameLoop(c, dr) { return dr && Math.abs(c.start - dr.start) < 0.0015 && Math.abs(c.length - dr.length) < 0.0015; }
function maybeAutoSearchLoops(d) {
  if (!d || d.isImage || !d.canTune || d.previewKind !== 'video') return;
  if (d.loops) { autoPickLoop(); return; }
  if (!loopJobs.has(d.id)) runLoopSearch(d, {});
}
async function runLoopSearch(d, { minLen, maxLen }) {
  if (loopJobs.has(d.id)) return;
  const st = { status: 'Starting…' };
  loopJobs.set(d.id, st);
  renderLoopCards();
  try {
    const { jobId } = await call('loop_search', { originalId: d.id, minLen: minLen || null, maxLen: maxLen || null });
    await pollJob(jobId, j => { st.status = j.status === 'waiting' ? plainWait(j.message) : `${j.message || 'Searching'}… ${Math.round((j.progress || 0) * 100)}%`; if (S.detail && S.detail.id === d.id) renderLoopCards(); });
    loopJobs.delete(d.id);
    if (S.detail && S.detail.id === d.id) { await loadDetail(); autoPickLoop(true); }
  } catch (e) {
    loopJobs.delete(d.id);
    st.error = e.message;
    if (S.detail && S.detail.id === d.id) { refs.loopStatus && (refs.loopStatus.textContent = e.message); }
  }
}
/** Select the best loop unless the current tuning already is one of the results (or was edited by hand). */
function autoPickLoop(fresh = false) {
  const d = S.detail, L = d && d.loops;
  if (!L || !L.candidates.length || !S.draft || S.draft.manualLoop) return;
  if (L.candidates.some(c => sameLoop(c, S.draft))) { renderLoopCards(); return; }
  if (!fresh && d.draft && d.versions.length) { renderLoopCards(); return; } // keep an existing tuning on reopen
  pickLoop(L.candidates[0], false);
}
function pickLoop(c, focusPreview = true) {
  if (S.showing) showPreview(null);
  S.draft.manualLoop = false;
  setTrim(c.startFrame / srcFps(), c.frames / OUT_FPS(), c.startFrame / srcFps(), false);
  S.draft.blend = c.blend || 0;
  if (refs.blend) { refs.blend.value = S.draft.blend; updateLoopInfo(); }
  if (media && media.tagName === 'VIDEO') { play.mode = 'selection'; media.play().catch(() => {}); }
  updateTimeline();
  renderLoopCards();
  draftChanged();
  if (focusPreview && refs.stage) refs.stage.scrollIntoView({ block: 'nearest' });
}
let loopSig = '';
function renderLoopCards() {
  const box = refs.loopCards;
  if (!box) return;
  const d = S.detail, L = d.loops, job = loopJobs.get(d.id);
  // re-render only when something changed (re-creating the card videos would restart them)
  const sig = JSON.stringify([d.id, L && L.at, job && (job.error || job.status), S.draft && [S.draft.start, S.draft.length, S.draft.manualLoop], box.isConnected]);
  if (sig === loopSig && box.childElementCount) return;
  loopSig = sig;
  box.textContent = '';
  if (job && !job.error) {
    refs.loopStatus.textContent = job.status;
    for (let i = 0; i < 3; i++) box.append(el('li', {}, el('div', { class: 'loop-card skeleton-card', 'aria-hidden': 'true' })));
    return;
  }
  if (!L) { refs.loopStatus.textContent = job && job.error ? job.error : ''; return; }
  refs.loopStatus.textContent = L.candidates.length ? '' : 'No seamless loop found in that length range. Try other lengths.';
  if (refs.lsMin && document.activeElement !== refs.lsMin) refs.lsMin.value = L.minLen || L.searched.minLen || 3;
  if (refs.lsMax && document.activeElement !== refs.lsMax) { refs.lsMax.value = L.maxLen || ''; refs.lsMax.placeholder = `auto (${L.searched.maxLen})`; }
  L.candidates.forEach((c, i) => {
    const selected = sameLoop(c, S.draft) && !S.draft.manualLoop;
    const v = el('video', { src: c.preview, muted: true, loop: true, autoplay: true, playsInline: true, 'aria-hidden': 'true' });
    v.muted = true;
    const card = el('button', { class: 'loop-card' + (selected ? ' selected' : ''), role: 'radio', 'aria-checked': String(selected),
      'aria-label': `Loop ${i + 1}: ${fmtSecs(c.length)}, ${SMOOTH(c).toLowerCase()}${i === 0 ? ', best' : ''}`, onclick: () => pickLoop(c) },
      el('div', { class: 'loop-video' }, v),
      el('div', { class: 'loop-meta' }, el('b', { class: 'num', text: fmtSecs(c.length) }),
        el('span', { class: 'chip ' + SEAM_CHIP[seamKind(c)], text: SMOOTH(c), title: (c.blend || 0) > 0 ? `Cross-fades the last ${c.blend.toFixed(2)} s into the start` + (seamKind(c) === 'soft' ? ' (part of the picture moves, so it shows a brief double image)' : '') : 'Plays straight through: the end runs into the start like any other frame' }),
        i === 0 ? el('span', { class: 'small muted', text: 'Best' }) : null));
    box.append(el('li', {}, card));
  });
}
function infoTip(id, text) {
  return el('span', { class: 'info', tabindex: '0', role: 'button', 'aria-label': 'More information', 'aria-describedby': id }, 'ⓘ',
    el('span', { class: 'tip', role: 'tooltip', id, text }));
}
function buildLoopCards(d) {
  const g = el('fieldset', { class: 'group' }, el('legend', { class: 'group-label', text: 'Loop' }));
  refs.loopStatus = el('p', { class: 'small muted one-line status-line-fixed', role: 'status' });
  refs.loopCards = el('ul', { class: 'loop-cards', role: 'radiogroup', 'aria-label': 'Loops found in this clip' });
  refs.lsMin = el('input', { type: 'number', id: 'lsMin', min: '0.5', max: '30', step: '0.5', value: '3' });
  refs.lsMax = el('input', { type: 'number', id: 'lsMax', min: '0.5', max: '30', step: '0.5', placeholder: 'auto' });
  refs.lsAgain = el('button', { class: 'btn ghost', text: 'Search again', onclick: () => {
    const minLen = Number(refs.lsMin.value) || 3, maxLen = Number(refs.lsMax.value) || null;
    if (maxLen && maxLen < minLen) { refs.loopStatus.textContent = 'The longest loop must be at least as long as the shortest.'; return; }
    runLoopSearch(S.detail, { minLen, maxLen });
  } });
  g.append(refs.loopCards, refs.loopStatus,
    el('div', { class: 'row ls-range' },
      el('label', { for: 'lsMin', class: 'small', text: 'Shortest loop' }), refs.lsMin, el('span', { class: 'small muted', text: 's' }),
      infoTip('tipMin', 'Loops shorter than this are not suggested. Very short loops repeat often and are easy to notice.'),
      el('label', { for: 'lsMax', class: 'small', text: 'Longest loop' }), refs.lsMax, el('span', { class: 'small muted', text: 's' }),
      infoTip('tipMax', 'Leave empty ("auto") for the longest loop that still looks good in the space on your screen. Longer loops need stronger compression.'),
      refs.lsAgain));
  return g;
}

// ---------- tune ----------
function buildTune(d) {
  const t = refs.tune;
  t.textContent = '';
  t.append(el('h3', { id: 'hTune', class: 'sr-only', text: 'Tune' }));
  if (!d.canTune) {
    t.append(el('p', { class: 'muted small', text: d.assets === 'pending' ? 'Preparing the preview… You can tune the clip in a moment.' :
      d.missing ? 'The original file is missing, so new versions can\'t be made.' : 'This file can\'t be read, so versions can\'t be made.' }));
    return;
  }
  const dr = S.draft;
  dr.quality = 'auto'; // Best that fits is the only choice in the UI (Custom stays in the code)
  // 1. loop: the cards are the control; manual editing only on request (⋯ > Edit loop manually…)
  refs.trim = null; refs.blend = null;
  if (!d.isImage && d.duration) {
    t.append(buildLoopCards(d));
    if (S.manualLoop) t.append(buildLoopGroup(d));
  } else {
    t.append(el('fieldset', { class: 'group' }, el('legend', { class: 'group-label', text: 'Loop' }),
      el('p', { class: 'muted small', text: `Still image: it is shown as a ${fmtSecs(dr.length)} clip that repeats.` })));
  }
  // 2. framing ("size") and rounded corners
  const fg = el('fieldset', { class: 'group' }, el('legend', { class: 'group-label', text: 'Size and position on the screen' }));
  refs.zoom = el('input', { type: 'range', id: 'fZoom', min: '0.2', max: '4', step: '0.01' });
  refs.zoomVal = el('span', { class: 'num small muted', style: 'min-width:48px' });
  refs.posX = el('input', { type: 'range', id: 'fX', min: '0', max: '1', step: '0.001' });
  refs.posY = el('input', { type: 'range', id: 'fY', min: '0', max: '1', step: '0.001' });
  fg.append(
    el('div', { class: 'slider-row' }, el('label', { for: 'fZoom', class: 'small muted', text: 'Zoom' }), refs.zoom, refs.zoomVal),
    el('div', { class: 'slider-row' }, el('label', { for: 'fX', class: 'small muted', text: 'Left – right' }), refs.posX),
    el('div', { class: 'slider-row' }, el('label', { for: 'fY', class: 'small muted', text: 'Up – down' }), refs.posY),
    el('div', { class: 'row' },
      el('button', { class: 'btn secondary', text: 'Fill the screen', onclick: () => { dr.frame = { ...DEFAULT_FRAME }; applyGeometry(); draftChanged(); } }),
      el('button', { class: 'btn secondary', text: 'Show the whole picture', onclick: () => { dr.frame = { zoom: Math.round(fitZoom(natural.w, natural.h) * 1000) / 1000, x: 0.5, y: 0.5 }; applyGeometry(); draftChanged(); } }),
      el('span', { class: 'muted small', text: 'Tip: drag the preview to move the picture.' })),
    el('p', { class: 'muted small', text: 'Corners: per theme (On screen), default in Settings.' }));
  refs.zoom.oninput = () => { dr.frame.zoom = Number(refs.zoom.value); applyGeometry(); };
  refs.posX.oninput = () => { dr.frame.x = Number(refs.posX.value); applyGeometry(); };
  refs.posY.oninput = () => { dr.frame.y = Number(refs.posY.value); applyGeometry(); };
  for (const r of [refs.zoom, refs.posX, refs.posY]) r.onchange = draftChanged;
  t.append(fg);
  // 3. quality: always Best that fits; the meter says what it chose
  refs.meterState = el('span', { class: 'meter-state' });
  refs.meter = el('div', { class: 'meter', role: 'img' }, el('div', { class: 'fill' }), el('div', { class: 'over', hidden: true }));
  refs.meterText = el('div', { class: 'small muted num one-line' });
  refs.meterNote = el('div', { class: 'small muted one-line' });
  refs.meterRetry = el('button', { class: 'btn ghost', text: 'Retry', hidden: true, onclick: () => scheduleEstimate(0) });
  t.append(el('div', { class: 'group' }, el('div', { class: 'meter-head' }, el('span', { class: 'group-label', text: 'Quality and space on the screen' }), refs.meterState),
    el('div', { class: 'meter-wrap' }, refs.meter), refs.meterText, refs.meterNote, refs.meterRetry));
  syncTuneControls();
}

// ----- rename (inline) -----
function startRename() {
  const d = S.detail;
  const input = el('input', { type: 'text', value: d.name, 'aria-label': 'Clip name' });
  const err = el('div', { class: 'field-err', role: 'alert' });
  refs.titles.replaceChildren(input, err);
  refs.renameBtn.disabled = true;
  input.focus(); input.select();
  let done = false;
  const finish = async save => {
    if (done) return;
    if (save && input.value.trim() && input.value.trim() !== d.name) {
      try { S.detail = await call('original_patch', { originalId: d.id, name: input.value.trim() }); }
      catch (e) { err.textContent = e.message; input.focus(); return; }
    }
    done = true;
    refs.titles.replaceChildren(refs.title, refs.meta);
    updateDetail(S.detail);
    refs.title.focus();
    loadOriginals();
  };
  input.addEventListener('keydown', e => { if (e.key === 'Enter') finish(true); if (e.key === 'Escape') { e.stopPropagation(); finish(false); } });
  input.addEventListener('blur', () => setTimeout(() => { if (!done && document.activeElement !== input) finish(true); }, 150));
}

async function confirmDeleteOriginal() {
  const d = S.detail;
  const n = d.versions.length;
  const onScreen = d.versions.some(v => v.onScreen);
  const lines = [];
  if (d.missing || !d.file) lines.push(`Its ${n} version${n === 1 ? '' : 's'} will be deleted.`);
  else lines.push(`The file “${d.fileName}” goes to the Recycle Bin and ${n ? `its ${n} version${n === 1 ? '' : 's'} ${n === 1 ? 'is' : 'are'}` : 'nothing else is'} deleted.`);
  if (onScreen) lines.push('The screen keeps playing it until you store another Embedded.');
  const ok = await openDialog({
    title: `${d.missing ? 'Remove' : 'Delete'} “${d.name}”?`, body: el('div', {}, lines.map(t => el('p', { text: t }))),
    actions: [{ label: 'Cancel', value: false, kind: 'secondary' }, { label: d.missing ? 'Remove' : 'Delete', value: true, kind: 'danger' }],
  });
  if (!ok) return;
  try { await call('original_delete', { originalId: d.id }); } catch (e) { toast(e.message, true, 10000); return; }
  S.sel = null; detailFor = null;
  await loadOriginals();
  await loadApp();
}

// ----- menus -----
function menuButton(label, itemsFn) {
  const btn = el('button', { class: 'iconbtn', 'aria-label': label, 'aria-haspopup': 'menu', 'aria-expanded': 'false', text: '⋯' });
  const wrap = el('div', { class: 'menu-wrap' }, btn);
  let menu = null;
  const close = (focus = true) => {
    if (!menu) return;
    menu.remove(); menu = null; btn.setAttribute('aria-expanded', 'false');
    document.removeEventListener('mousedown', outside, true);
    if (focus) btn.focus();
  };
  const outside = e => { if (menu && !wrap.contains(e.target)) close(false); };
  btn.onclick = e => {
    e.stopPropagation();
    if (menu) return close();
    menu = el('div', { class: 'menu', role: 'menu' });
    for (const it of itemsFn()) {
      menu.append(el('button', { role: 'menuitem', class: it.danger ? 'danger' : '', text: it.label, tabindex: '-1',
        onclick: ev => { ev.stopPropagation(); close(false); Promise.resolve(it.run()).catch(err => toast(err.message, true)); } }));
    }
    menu.addEventListener('keydown', ev => {
      const items = [...menu.querySelectorAll('button')];
      const i = items.indexOf(document.activeElement);
      if (ev.key === 'ArrowDown') { ev.preventDefault(); items[(i + 1) % items.length].focus(); }
      if (ev.key === 'ArrowUp') { ev.preventDefault(); items[(i - 1 + items.length) % items.length].focus(); }
      if (ev.key === 'Escape') { ev.preventDefault(); ev.stopPropagation(); close(); }
      if (ev.key === 'Tab') close(false);
    });
    menu.addEventListener('click', ev => ev.stopPropagation());
    wrap.append(menu);
    btn.setAttribute('aria-expanded', 'true');
    document.addEventListener('mousedown', outside, true);
    menu.querySelector('button').focus();
  };
  return wrap;
}

// ---------- preview ----------
let media = null, blendVid = null, rafId = 0, natural = { w: 0, h: 0 }, scrubbing = false;
const play = { mode: 'selection', phase: 'end' }; // mode: 'selection' | 'seam'
function stopPreviewLoop() { cancelAnimationFrame(rafId); rafId = 0; }
const OUT_FPS = () => (S.app ? S.app.deviceFps : 18);
const srcFps = () => (S.detail && S.detail.fps) || 25;
const r3 = v => Math.round(v * 1000) / 1000;
function fmtT2(s) {
  if (!Number.isFinite(s)) return '–';
  s = Math.max(0, s);
  const m = Math.floor(s / 60), sec = s - m * 60;
  return `${m}:${sec.toFixed(2).padStart(5, '0')}`;
}

function showPreview(vid, force = false) {
  const d = S.detail;
  if (!d || !refs.stage) return;
  if (!force && S.showing === vid && media) return;
  S.showing = vid;
  stopPreviewLoop();
  refs.stage.querySelectorAll('.media, .empty-msg').forEach(n => n.remove());
  media = null; blendVid = null;
  const v = vid ? d.versions.find(x => x.id === vid) : null;
  refs.stage.classList.toggle('version', !!v);
  if (v) {
    media = el('video', { class: 'media', src: backend.mediaUrl(v.id, 'preview.mp4', 'v'), muted: true, loop: true, autoplay: true, playsInline: true });
    media.muted = true;
    refs.stage.prepend(media);
    refs.notch.hidden = true;
    media.play().catch(() => {});
  } else if (d.previewUrl) {
    if (d.previewKind === 'image') {
      media = el('img', { class: 'media', src: d.previewUrl, alt: '' });
      media.onload = () => { natural = { w: media.naturalWidth, h: media.naturalHeight }; applyGeometry(); };
    } else {
      media = el('video', { class: 'media', src: d.previewUrl, muted: true, autoplay: true, playsInline: true, preload: 'auto' });
      media.muted = true;
      media.addEventListener('loadedmetadata', () => {
        natural = { w: media.videoWidth, h: media.videoHeight };
        applyGeometry();
        media.currentTime = S.draft.start;
        media.play().catch(() => {});
        drawFilmstrip();
      });
      // Second copy of the source for the loop blend: shows the frames from just before the start, fading in.
      blendVid = el('video', { class: 'media blend', src: d.previewUrl, muted: true, playsInline: true, preload: 'auto', 'aria-hidden': 'true' });
      blendVid.muted = true;
      blendVid.style.opacity = '0';
    }
    natural = { w: d.width || W, h: d.height || H };
    if (blendVid) refs.stage.prepend(blendVid);
    refs.stage.prepend(media);
    applyGeometry();
  } else {
    refs.stage.prepend(el('div', { class: 'empty-msg', text: d.assets === 'error' ? 'No preview: this file can\'t be read.' : d.missing ? 'The original file is missing.' : 'Preparing the preview…' }));
  }
  if (!v && S.app) styleNotch(refs.notch, liveMask());
  applyCorners();
  if (!refs.guides) refs.guides = el('div', { class: 'guides-layer' });
  refs.stage.append(refs.notch, refs.corners, refs.guides);
  if (refs.stage.classList.contains('outline')) refs.notch.classList.add('outline');
  refs.stage.classList.toggle('pannable', !v && !!d.canTune);
  refs.backToTune.hidden = !v || !d.canTune;
  updateShowingLabel();
  renderVersionsHighlight();
  rafId = requestAnimationFrame(tickPreview);
}

// Corners are part of each version: the tuning shows the draft's corners; a version preview has them baked in.
function applyCorners() {
  if (!refs.corners) return;
  if (S.showing || !S.draft) { refs.corners.style.display = 'none'; return; }
  styleCorners(refs.corners, S.draft.corners, liveMask());
}

function applyGeometry() {
  if (!media || S.showing || !S.draft || !natural.w) return;
  const g = frameGeometry(natural.w, natural.h, S.draft.frame);
  const st = { left: (g.left / W) * 100 + '%', top: (g.top / H) * 100 + '%', width: (g.width / W) * 100 + '%', height: (g.height / H) * 100 + '%' };
  // "Simplify fine detail" = Gaussian blur on the 2400x1080 canvas (src/convert.js detailSigma); same deviation here.
  // In Best that fits the level is the one the size check chose; in Custom it is the slider.
  const level = S.draft.quality === 'auto' ? (S.est.r && S.est.r.mode === 'auto' && S.est.r.fits ? S.est.r.detail || 0 : 0) : S.draft.detail || 0;
  const sigma = (Math.min(100, Math.max(0, level)) / 100) * 3.5 * (refs.stage.clientWidth / W);
  st.filter = sigma > 0 ? `blur(${sigma.toFixed(2)}px)` : '';
  Object.assign(media.style, st);
  if (blendVid) Object.assign(blendVid.style, st);
  updateFrameControls();
}
window.addEventListener('resize', () => applyGeometry());

// Looping is driven by animation frames; a timer takes over when they are throttled (e.g. a hidden window).
let lastTick = 0;
setInterval(() => { if (rafId && performance.now() - lastTick > 120) tickBody(); }, 40);
function tickPreview() {
  rafId = requestAnimationFrame(tickPreview);
  tickBody();
}
function tickBody() {
  lastTick = performance.now();
  if (!refs.playBtn) return;
  refs.playBtn.hidden = !media || media.tagName !== 'VIDEO';
  if (!media || media.tagName !== 'VIDEO') { if (refs.time) refs.time.textContent = S.detail && S.detail.isImage && !S.showing ? `Still image · ${fmtSecs(S.draft.length)} on the screen` : ''; return; }
  const playing = !media.paused;
  refs.playBtn.textContent = playing ? '❚❚' : '▶';
  refs.playBtn.setAttribute('aria-label', playing ? 'Pause' : 'Play');
  if (S.showing) {
    refs.time.textContent = `${fmtTime(media.currentTime)} / ${fmtTime(media.duration)}`;
    return;
  }
  const { start, length } = S.draft;
  const end = start + length;
  const ct = media.currentTime;
  if (!scrubbing) {
    if (play.mode === 'seam') {
      // The seam: last second of the selection, then the first second, over and over.
      const a = Math.max(start, end - 1), b = Math.min(end, start + 1);
      if (play.phase === 'end' && (ct >= end - 0.01 || ct < a - 0.25)) { media.currentTime = start; play.phase = 'start'; }
      else if (play.phase === 'start' && (ct >= b - 0.01 || ct < start - 0.25)) { media.currentTime = a; play.phase = 'end'; }
    } else if (ct >= end - 0.01 || ct < start - 0.25) media.currentTime = start;
  }
  updateBlendOverlay(media.currentTime, playing);
  const rel = media.currentTime - start;
  refs.time.textContent = `${fmtT2(Math.max(0, rel))} / ${fmtT2(length)} · frame ${Math.min(Math.round(length * OUT_FPS()), Math.floor(Math.max(0, rel) * OUT_FPS() + 1e-6) + 1)}`;
  if (refs.playhead && S.detail.duration) { refs.playhead.style.left = trackPct(media.currentTime); }
  if (refs.tlHead && S.detail.duration) refs.tlHead.style.left = (media.currentTime / S.detail.duration) * 100 + '%';
}

/** Effective blend (as the converter will apply it): whole output frames, <= half the selection, <= time before start. */
function effectiveBlend() {
  const dr = S.draft;
  if (!dr || S.detail.isImage) return 0;
  const b = Math.max(0, Math.min(dr.blend || 0, 1.5, dr.length / 2, dr.start));
  return Math.floor(b * OUT_FPS() + 1e-6) / OUT_FPS();
}
function updateBlendOverlay(ct, playing) {
  if (!blendVid) return;
  const B = effectiveBlend();
  const end = S.draft.start + S.draft.length;
  if (B > 0 && ct >= end - B && ct < end) {
    const p = (ct - (end - B)) / B;
    const target = S.draft.start - B + (ct - (end - B));
    if (Math.abs(blendVid.currentTime - target) > 0.12 || (playing && blendVid.paused) || (!playing && Math.abs(blendVid.currentTime - target) > 0.02)) {
      blendVid.currentTime = target;
    }
    if (playing && blendVid.paused) blendVid.play().catch(() => {});
    if (!playing && !blendVid.paused) blendVid.pause();
    blendVid.style.opacity = String(p);
  } else {
    blendVid.style.opacity = '0';
    if (!blendVid.paused) blendVid.pause();
  }
}

function togglePlay() {
  if (!media || media.tagName !== 'VIDEO') return;
  if (media.paused) media.play().catch(() => {}); else media.pause();
}

function updateShowingLabel() {
  if (!refs.showing) return;
  const d = S.detail;
  const v = S.showing && d && d.versions.find(x => x.id === S.showing);
  if (v) refs.showing.textContent = `Showing: ${v.label || 'v' + v.n}${v.onScreen ? ' · on the screen now' : ''}`;
  else refs.showing.textContent = d && d.canTune ? 'Showing: your tuning' : d && d.previewUrl ? 'Showing: the original' : '';
}

// drag the picture inside the frame to move it
function setupPan() {
  let drag = null;
  refs.stage.addEventListener('pointerdown', e => {
    if (S.showing || !S.detail.canTune || !media || e.button !== 0) return;
    const g = frameGeometry(natural.w, natural.h, S.draft.frame);
    drag = { x: e.clientX, y: e.clientY, fx: S.draft.frame.x, fy: S.draft.frame.y, g, scale: W / refs.stage.clientWidth };
    refs.stage.setPointerCapture(e.pointerId);
    refs.stage.classList.add('dragging');
  });
  refs.stage.addEventListener('pointermove', e => {
    if (!drag) return;
    const dx = (e.clientX - drag.x) * drag.scale, dy = (e.clientY - drag.y) * drag.scale;
    const f = S.draft.frame;
    if (drag.g.width !== W) f.x = clamp(drag.fx + dx / (W - drag.g.width), 0, 1);
    if (drag.g.height !== H) f.y = clamp(drag.fy + dy / (H - drag.g.height), 0, 1);
    f.x = Math.round(f.x * 1000) / 1000; f.y = Math.round(f.y * 1000) / 1000;
    applyGeometry();
  });
  const end = () => { if (!drag) return; drag = null; refs.stage.classList.remove('dragging'); draftChanged(); };
  refs.stage.addEventListener('pointerup', end);
  refs.stage.addEventListener('pointercancel', end);
}

// ---------- frame grabbing (for the filmstrip and the seam check; reads the same media the preview plays) ----------
let grabber = null, grabChain = Promise.resolve();
function grabFrame(time, w, h) {
  const url = S.detail && S.detail.previewUrl;
  const run = () => new Promise(resolve => {
    if (!url) return resolve(null);
    if (!grabber || grabber.dataset.src !== url) {
      grabber = el('video', { muted: true, preload: 'auto', playsInline: true });
      grabber.muted = true; grabber.dataset.src = url; grabber.src = url;
    }
    const v = grabber;
    const draw = () => {
      const c = document.createElement('canvas');
      c.width = w; c.height = h;
      try { c.getContext('2d').drawImage(v, 0, 0, w, h); } catch { return resolve(null); }
      resolve(c);
    };
    const seek = () => {
      // Like ffmpeg's fps filter (round to the nearest source frame); +1 ms lands inside that frame.
      const t = clamp(Math.round(time * srcFps()) / srcFps() + 0.001, 0, Math.max(0, (v.duration || time) - 0.001));
      if (Math.abs(v.currentTime - t) < 0.0005 && v.readyState >= 2) return draw();
      const done = () => { v.removeEventListener('seeked', done); clearTimeout(to); draw(); };
      const to = setTimeout(done, 3000);
      v.addEventListener('seeked', done);
      v.currentTime = t;
    };
    if (v.readyState >= 1) seek(); else v.addEventListener('loadedmetadata', seek, { once: true });
  });
  grabChain = grabChain.then(run, run);
  return grabChain;
}

// ----- loop editing -----
const view = { zoomed: false, a: 0, b: 0 }; // visible part of the trim track (seconds)
function buildLoopGroup(d) {
  const g = el('fieldset', { class: 'group' }, el('legend', { class: 'group-label', text: 'Part of the clip (the loop)' }));
  refs.zoomTrack = el('button', { class: 'btn ghost', 'aria-pressed': 'false', text: 'Zoom to the selection', onclick: () => { view.zoomed = !view.zoomed; fitView(); positionTrim(); drawFilmstrip(); } });
  refs.trim = el('div', { class: 'trim' });
  refs.film = el('canvas', { class: 'filmstrip', 'aria-hidden': 'true' });
  refs.trimRange = el('div', { class: 'range', title: 'Drag to move the selected part' });
  refs.hStart = el('div', { class: 'handle', role: 'slider', tabindex: '0', 'aria-label': 'Start of the loop. Arrow keys move one frame, Shift+arrow ten frames.' });
  refs.hEnd = el('div', { class: 'handle', role: 'slider', tabindex: '0', 'aria-label': 'End of the loop. Arrow keys move one frame, Shift+arrow ten frames.' });
  refs.playhead = el('div', { class: 'playhead', 'aria-hidden': 'true' });
  refs.trim.append(refs.film, refs.trimRange, refs.playhead, refs.hStart, refs.hEnd);
  refs.viewLabel = el('span', { class: 'muted small num' });
  // start / end rows: frame buttons, numbers
  const nudgeBtn = (label, aria, fn) => el('button', { class: 'btn secondary nudge', text: label, 'aria-label': aria, onclick: e => fn(e.shiftKey ? 10 : 1) });
  refs.inStartF = el('input', { type: 'number', id: 'tStartF', min: '0', step: '1' });
  refs.inLenF = el('input', { type: 'number', id: 'tLenF', min: '1', step: '1' });
  refs.startInfo = el('span', { class: 'muted small num' });
  refs.endInfo = el('span', { class: 'muted small num' });
  const startRow = el('div', { class: 'frame-row' },
    el('span', { class: 'frame-row-label', text: 'Start' }),
    nudgeBtn('◀ frame', 'Start one frame earlier', n => nudgeStart(-n)), nudgeBtn('frame ▶', 'Start one frame later', n => nudgeStart(n)),
    el('label', { for: 'tStartF', class: 'small muted', text: 'source frame' }), refs.inStartF, refs.startInfo);
  const endRow = el('div', { class: 'frame-row' },
    el('span', { class: 'frame-row-label', text: 'End' }),
    nudgeBtn('◀ frame', 'End one frame earlier', n => nudgeEnd(-n)), nudgeBtn('frame ▶', 'End one frame later', n => nudgeEnd(n)),
    el('label', { for: 'tLenF', class: 'small muted', text: `length in frames (${OUT_FPS()} per second)` }), refs.inLenF, refs.endInfo);
  // play mode
  refs.pmSel = el('input', { type: 'radio', name: 'pm', value: 'selection', checked: play.mode === 'selection' });
  refs.pmSeam = el('input', { type: 'radio', name: 'pm', value: 'seam', checked: play.mode === 'seam' });
  const onMode = () => {
    play.mode = refs.pmSeam.checked ? 'seam' : 'selection';
    if (media && media.tagName === 'VIDEO') {
      const end = S.draft.start + S.draft.length;
      if (play.mode === 'seam') { play.phase = 'end'; media.currentTime = Math.max(S.draft.start, end - 1); } else media.currentTime = S.draft.start;
      media.play().catch(() => {});
    }
  };
  refs.pmSel.onchange = refs.pmSeam.onchange = onMode;
  const modes = el('div', { class: 'row', role: 'radiogroup', 'aria-label': 'Preview plays' },
    el('span', { class: 'small muted', text: 'Preview plays:' }),
    el('label', { class: 'radio' }, refs.pmSel, 'the selection'),
    el('label', { class: 'radio' }, refs.pmSeam, 'the seam (last second, then first second)'));
  // blend
  refs.blend = el('input', { type: 'range', id: 'bVal', min: '0', max: '1.5', step: String(1 / OUT_FPS()) });
  refs.blendNum = el('span', { class: 'num small', style: 'min-width:48px' });
  refs.blendNote = el('p', { class: 'muted small' });
  refs.blend.oninput = () => { S.draft.blend = r3(Number(refs.blend.value)); updateLoopInfo(); };
  refs.blend.onchange = draftChanged;
  const blendRow = el('div', { class: 'slider-row' }, el('label', { for: 'bVal', class: 'small muted', text: 'Blend the end into the start' }), refs.blend, refs.blendNum);
  // seam check
  refs.seamBtn = el('button', { class: 'btn secondary', text: 'Check the seam', 'aria-expanded': 'false', onclick: toggleSeamPanel });
  refs.findBtn = el('button', { class: 'btn ghost', text: 'Only near the current end', onclick: findLoopPoint });
  refs.findResult = el('div', { class: 'small', role: 'status' });
  refs.lsMin = el('input', { type: 'number', id: 'lsMin', min: '0.5', max: '30', step: '0.5', value: '3' });
  refs.lsMax = el('input', { type: 'number', id: 'lsMax', min: '0.5', max: '30', step: '0.5', placeholder: 'auto' });
  refs.lsBtn = el('button', { class: 'btn primary', text: 'Find loop points', onclick: searchLoops });
  refs.lsStatus = el('div', { class: 'small muted', role: 'status' });
  refs.lsResults = el('ul', { class: 'ls-results' });
  const lsBox = el('div', { class: 'ls-box' },
    el('div', { class: 'row' }, el('span', { class: 'small', text: 'Loop length between' }),
      el('label', { for: 'lsMin', class: 'sr-only', text: 'Shortest loop (seconds)' }), refs.lsMin, el('span', { class: 'small', text: 'and' }),
      el('label', { for: 'lsMax', class: 'sr-only', text: 'Longest loop (seconds; empty = the longest that fits)' }), refs.lsMax,
      el('span', { class: 'small', text: 's' }), refs.lsBtn),
    el('p', { class: 'muted small', text: 'Searches the whole clip, frame by frame, for the starts and ends that join most smoothly. Empty maximum = the longest loop that fits on the screen.' }),
    refs.lsStatus, refs.lsResults);
  refs.seamPanel = el('div', { class: 'seam-panel', hidden: true });
  g.append(el('div', { class: 'row trim-head' }, refs.viewLabel, el('span', { class: 'grow' }), refs.zoomTrack),
    refs.trim, startRow, endRow, modes, blendRow, refs.blendNote,
    lsBox, el('div', { class: 'row' }, refs.seamBtn, refs.findBtn), refs.findResult, refs.seamPanel);
  setupTrim();
  fitView();
  return g;
}

function fitView() {
  const d = S.detail, dr = S.draft;
  if (view.zoomed) {
    const pad = Math.max(0.5, dr.length * 0.15);
    view.a = Math.max(0, dr.start - pad); view.b = Math.min(d.duration, dr.start + dr.length + pad);
  } else { view.a = 0; view.b = d.duration; }
  if (refs.zoomTrack) {
    refs.zoomTrack.setAttribute('aria-pressed', String(view.zoomed));
    refs.zoomTrack.textContent = view.zoomed ? 'Show the whole clip' : 'Zoom to the selection';
  }
  if (refs.viewLabel) refs.viewLabel.textContent = `${fmtT2(view.a)} – ${fmtT2(view.b)}`;
}
const trackPct = t => (clamp((t - view.a) / Math.max(0.001, view.b - view.a), 0, 1) * 100) + '%';

// Filmstrip: thumbnails of the visible part of the track (cheap: grabbed from the preview media in the browser).
let filmSeq = 0, filmT = 0;
function drawFilmstrip() {
  clearTimeout(filmT);
  filmT = setTimeout(async () => {
    const c = refs.film, d = S.detail;
    if (!c || !d || !d.previewUrl || d.previewKind !== 'video') return;
    const seq = ++filmSeq;
    const w = c.clientWidth, h = c.clientHeight;
    if (!w || !h) return;
    c.width = w; c.height = h;
    const ctx = c.getContext('2d');
    const tw = Math.max(24, Math.round(h * (natural.w / natural.h || 16 / 9)));
    const n = Math.max(1, Math.ceil(w / tw));
    for (let i = 0; i < n; i++) {
      const t = view.a + ((i + 0.5) / n) * (view.b - view.a);
      const f = await grabFrame(t, tw, h);
      if (seq !== filmSeq) return;
      if (f) ctx.drawImage(f, i * tw, 0);
    }
  }, 250);
}

function syncTuneControls() {
  const dr = S.draft;
  if (!dr || !refs.saveBtn) return;
  if (refs.trim) { refs.blend.value = dr.blend || 0; updateLoopInfo(); positionTrim(); }
  if (refs.qAuto) { // Custom quality controls exist only if re-enabled
    refs.qAuto.checked = dr.quality === 'auto'; refs.qCustom.checked = dr.quality !== 'auto';
    refs.qSlider.value = dr.quality === 'auto' ? 80 : dr.quality;
    refs.qNum.textContent = refs.qSlider.value;
    refs.qRow.hidden = dr.quality === 'auto';
    refs.detail.value = dr.detail || 0; refs.detailNum.textContent = dr.detail || 'off';
    refs.detailBox.hidden = dr.quality === 'auto';
  }
  applyCorners();
  updateFrameControls();
}

function updateLoopInfo() {
  const d = S.detail, dr = S.draft;
  if (!refs.trim) return;
  const sf = Math.round(dr.start * srcFps());
  const n = Math.round(dr.length * OUT_FPS());
  if (document.activeElement !== refs.inStartF) refs.inStartF.value = sf;
  if (document.activeElement !== refs.inLenF) refs.inLenF.value = n;
  refs.inStartF.max = Math.max(0, Math.floor((d.duration - 0.05) * srcFps()));
  refs.inLenF.max = Math.round(Math.min(TRIM_MAX, d.duration - dr.start) * OUT_FPS());
  refs.startInfo.textContent = `at ${fmtT2(dr.start)}${d.fps ? '' : ' (source frame rate unknown, assuming 25)'}`;
  refs.endInfo.textContent = `= ${fmtSecs(dr.length)}, ends at ${fmtT2(dr.start + dr.length)}`;
  const B = effectiveBlend();
  refs.blendNum.textContent = (dr.blend || 0) > 0 ? `${(dr.blend).toFixed(2)} s` : 'off';
  refs.blendNote.textContent = (dr.blend || 0) <= 0 ? 'Cross-fades the last moments of the loop into the frames just before its start, so the jump back is invisible. The length stays the same.'
    : B + 1e-6 < (dr.blend || 0) ? `Only ${B.toFixed(2)} s can be blended: it needs that much of the clip before the start (and at most half the loop). Move the start later for more.`
      : `The last ${B.toFixed(2)} s fade into the ${B.toFixed(2)} s just before the start.`;
}

function updateFrameControls() {
  if (!refs.zoom || !S.draft) return;
  const f = S.draft.frame;
  const fz = fitZoom(natural.w, natural.h);
  refs.zoom.min = Math.min(1, Math.floor(fz * 100) / 100);
  refs.zoom.value = f.zoom; refs.posX.value = f.x; refs.posY.value = f.y;
  refs.zoomVal.textContent = Math.round(f.zoom * 100) + '%';
  refs.zoom.setAttribute('aria-valuetext', `${Math.round(f.zoom * 100)} percent${f.zoom === 1 ? ', fills the screen' : ''}`);
  const g = frameGeometry(natural.w, natural.h, f);
  refs.posX.disabled = g.width === W; refs.posY.disabled = g.height === H;
}

function positionTrim() {
  const d = S.detail, dr = S.draft;
  if (!refs.trim || !d.duration) return;
  const end = dr.start + dr.length;
  refs.hStart.style.left = trackPct(dr.start);
  refs.hEnd.style.left = trackPct(end);
  refs.trimRange.style.left = trackPct(dr.start);
  refs.trimRange.style.width = `calc(${trackPct(end)} - ${trackPct(dr.start)})`;
  for (const [h, v, lo, hi] of [[refs.hStart, dr.start, 0, d.duration - 0.05], [refs.hEnd, end, dr.start + 1 / OUT_FPS(), Math.min(d.duration, dr.start + TRIM_MAX)]]) {
    h.setAttribute('aria-valuemin', r3(lo)); h.setAttribute('aria-valuemax', r3(hi)); h.setAttribute('aria-valuenow', r3(v));
    h.setAttribute('aria-valuetext', `${fmtT2(v)}, ${h === refs.hStart ? `source frame ${Math.round(v * srcFps())}` : `${Math.round(dr.length * OUT_FPS())} frames long`}`);
  }
  updateLoopInfo();
}

/** Start snaps to source frames; the length is a whole number of screen frames (1/18 s). */
function setTrim(start, length, seekTo, manual = true) {
  const d = S.detail;
  const sf = srcFps(), of = OUT_FPS();
  start = clamp(Math.round(start * sf) / sf, 0, Math.max(0, d.duration - 0.05));
  const maxN = Math.max(1, Math.floor(Math.min(TRIM_MAX, d.duration - start) * of + 1e-6));
  const n = clamp(Math.round(length * of), 1, maxN);
  S.draft.start = r3(start); S.draft.length = r3(n / of);
  if (manual) S.draft.manualLoop = true; // edited by hand: don't replace it with a found loop
  positionTrim();
  updateTimeline();
  if (media && media.tagName === 'VIDEO' && !S.showing) media.currentTime = seekTo != null ? seekTo : S.draft.start;
}
// Nudge by whole frames and show the frame that moved (paused), so 1-frame changes can be seen.
function nudgeStart(frames) {
  const end = S.draft.start + S.draft.length;
  const s = S.draft.start + frames / srcFps();
  if (media && media.tagName === 'VIDEO') media.pause();
  setTrim(s, end - s, Math.max(0, s));
  draftChangedSoon();
}
function nudgeEnd(frames) {
  const l = S.draft.length + frames / OUT_FPS();
  if (media && media.tagName === 'VIDEO') media.pause();
  setTrim(S.draft.start, l, S.draft.start + Math.max(1, Math.round(l * OUT_FPS())) / OUT_FPS() - 1 / OUT_FPS());
  draftChangedSoon();
}

function setupTrim() {
  const d = S.detail;
  const timeAt = x => { const r = refs.trim.getBoundingClientRect(); return view.a + clamp((x - r.left) / r.width, 0, 1) * (view.b - view.a); };
  const drag = (elm, onMove) => {
    elm.addEventListener('pointerdown', e => {
      e.preventDefault(); e.stopPropagation();
      elm.setPointerCapture(e.pointerId);
      scrubbing = true;
      if (media && media.tagName === 'VIDEO') media.pause();
      const t0 = timeAt(e.clientX), s0 = S.draft.start, l0 = S.draft.length;
      const move = ev => onMove(timeAt(ev.clientX), t0, s0, l0);
      const up = () => {
        elm.removeEventListener('pointermove', move); elm.removeEventListener('pointerup', up); elm.removeEventListener('pointercancel', up);
        scrubbing = false;
        if (media && media.tagName === 'VIDEO') { media.currentTime = S.draft.start; media.play().catch(() => {}); }
        if (view.zoomed) { fitView(); positionTrim(); drawFilmstrip(); }
        draftChanged();
      };
      elm.addEventListener('pointermove', move); elm.addEventListener('pointerup', up); elm.addEventListener('pointercancel', up);
    });
  };
  drag(refs.hStart, t => { const end = S.draft.start + S.draft.length; const s = clamp(t, Math.max(0, end - TRIM_MAX), end - 1 / OUT_FPS()); setTrim(s, end - s, s); });
  drag(refs.hEnd, t => { const e = clamp(t, S.draft.start + 1 / OUT_FPS(), d.duration); setTrim(S.draft.start, e - S.draft.start, e); });
  drag(refs.trimRange, (t, t0, s0, l0) => { const s = clamp(s0 + t - t0, 0, d.duration - l0); setTrim(s, l0, s); });
  const keys = (h, isEnd) => h.addEventListener('keydown', e => {
    const dir = { ArrowLeft: -1, ArrowDown: -1, ArrowRight: 1, ArrowUp: 1 }[e.key];
    if (dir) { e.preventDefault(); (isEnd ? nudgeEnd : nudgeStart)(dir * (e.shiftKey ? 10 : 1)); return; }
    let s = S.draft.start, l = S.draft.length;
    if (e.key === 'PageUp' || e.key === 'PageDown') { const st = e.key === 'PageUp' ? 1 : -1; if (isEnd) l += st; else { s += st; l -= st; } }
    else if (e.key === 'Home') { if (isEnd) l = 1 / OUT_FPS(); else { l += s; s = 0; } }
    else if (e.key === 'End') { if (isEnd) l = d.duration - s; else { s = s + l - 1 / OUT_FPS(); l = 1 / OUT_FPS(); } }
    else return;
    e.preventDefault();
    if (!isEnd && l > TRIM_MAX) l = TRIM_MAX;
    setTrim(s, Math.max(1 / OUT_FPS(), l), s);
    draftChangedSoon();
  });
  keys(refs.hStart, false); keys(refs.hEnd, true);
  refs.inStartF.onchange = () => { const end = S.draft.start + S.draft.length; const s = (Number(refs.inStartF.value) || 0) / srcFps(); setTrim(s, Math.max(1 / OUT_FPS(), end - s)); draftChanged(); };
  refs.inLenF.onchange = () => { setTrim(S.draft.start, (Number(refs.inLenF.value) || 1) / OUT_FPS()); draftChanged(); };
}

// ----- seam check -----
const seam = { mode: 'onion', opacity: 0.5 };
function toggleSeamPanel() {
  const open = refs.seamPanel.hidden;
  refs.seamPanel.hidden = !open;
  refs.seamBtn.setAttribute('aria-expanded', String(open));
  if (open) buildSeamPanel();
}
function buildSeamPanel() {
  const p = refs.seamPanel;
  p.textContent = '';
  const modeSel = el('select', { id: 'seamMode', 'aria-label': 'Comparison' },
    el('option', { value: 'onion', text: 'Onion skin (first frame over the last)' }),
    el('option', { value: 'side', text: 'Side by side (last | first)' }),
    el('option', { value: 'diff', text: 'Difference (black = identical)' }));
  modeSel.value = seam.mode;
  const op = el('input', { type: 'range', id: 'seamOp', min: '0', max: '1', step: '0.05', value: String(seam.opacity) });
  const opRow = el('div', { class: 'slider-row' }, el('label', { for: 'seamOp', class: 'small muted', text: 'First frame opacity' }), op);
  refs.seamCanvas = el('canvas', { class: 'seam-canvas', role: 'img', 'aria-label': 'Comparison of the last and first frame of the loop' });
  refs.seamScore = el('p', { class: 'small', role: 'status' });
  modeSel.onchange = () => { seam.mode = modeSel.value; opRow.hidden = seam.mode !== 'onion'; drawSeam(); };
  op.oninput = () => { seam.opacity = Number(op.value); drawSeam(); };
  opRow.hidden = seam.mode !== 'onion';
  p.append(el('div', { class: 'row' }, el('label', { for: 'seamMode', class: 'small muted', text: 'Compare' }), modeSel,
    el('button', { class: 'btn ghost', text: 'Refresh', onclick: () => updateSeam() })), opRow, refs.seamCanvas, refs.seamScore);
  updateSeam();
}
let seamFrames = null, seamSeq = 0, seamT = 0;
function updateSeamSoon() { if (refs.seamPanel && !refs.seamPanel.hidden) { clearTimeout(seamT); seamT = setTimeout(updateSeam, 300); } }
async function updateSeam() {
  const dr = S.draft, seq = ++seamSeq;
  const w = 480, h = Math.round(w * (natural.h / natural.w || 9 / 16));
  const last = dr.start + dr.length - 1 / OUT_FPS();
  const B = effectiveBlend();
  const a = await grabFrame(last, w, h);
  const first = await grabFrame(dr.start, w, h);
  // With a blend, the last frame is mostly the frame just before the start: mix like the converter does.
  let lastC = a;
  if (B > 0 && a) {
    const pre = await grabFrame(dr.start - 1 / OUT_FPS(), w, h);
    const p = (B - 1 / OUT_FPS()) / B;
    lastC = document.createElement('canvas'); lastC.width = w; lastC.height = h;
    const x = lastC.getContext('2d');
    x.drawImage(a, 0, 0); x.globalAlpha = p; if (pre) x.drawImage(pre, 0, 0);
  }
  // The score compares the frame that would follow the end with the first frame (the same measure as
  // "Find best loop point"): identical means the jump back to the start is invisible.
  const next = B > 0 ? null : await grabFrame(dr.start + dr.length, w, h);
  if (seq !== seamSeq) return;
  seamFrames = { last: lastC, first, next, blended: B > 0, w, h };
  drawSeam();
}
function drawSeam() {
  const c = refs.seamCanvas, f = seamFrames;
  if (!c || !f || !f.last || !f.first) { if (refs.seamScore) refs.seamScore.textContent = 'Couldn\'t read the frames.'; return; }
  const side = seam.mode === 'side';
  c.width = side ? f.w * 2 + 8 : f.w; c.height = f.h;
  const x = c.getContext('2d');
  x.clearRect(0, 0, c.width, c.height);
  if (side) { x.drawImage(f.last, 0, 0); x.drawImage(f.first, f.w + 8, 0); }
  else if (seam.mode === 'diff') { x.drawImage(f.last, 0, 0); x.globalCompositeOperation = 'difference'; x.drawImage(f.first, 0, 0); x.globalCompositeOperation = 'source-over'; }
  else { x.drawImage(f.last, 0, 0); x.globalAlpha = seam.opacity; x.drawImage(f.first, 0, 0); x.globalAlpha = 1; }
  // similarity of small grey versions (same measure as "Find best loop point")
  const sm = cv => { const s = document.createElement('canvas'); s.width = 96; s.height = 54; const g = s.getContext('2d'); g.drawImage(cv, 0, 0, 96, 54); return g.getImageData(0, 0, 96, 54).data; };
  const hint = side ? ' Left: last frame of the loop. Right: first frame.' : seam.mode === 'diff' ? ' Bright areas change at the jump.' : '';
  if (f.blended || !f.next) { refs.seamScore.textContent = (f.blended ? 'The blend fades the end into the start, so the jump back is smooth.' : '') + hint; return; }
  const A = sm(f.next), Bd = sm(f.first);
  let sum = 0;
  for (let i = 0; i < A.length; i += 4) sum += Math.abs((A[i] * 0.299 + A[i + 1] * 0.587 + A[i + 2] * 0.114) - (Bd[i] * 0.299 + Bd[i + 1] * 0.587 + Bd[i + 2] * 0.114));
  const sim = 100 - (sum / (A.length / 4) / 255) * 100;
  refs.seamScore.textContent = `The frame after the end is ${sim.toFixed(1)}% like the first frame` + (sim >= 98.8 ? ' (looks seamless).' : sim >= 97.5 ? ' (a small jump).' : ' (a visible jump).') + hint;
}

async function searchLoops() {
  const d = S.detail;
  const minLen = Number(refs.lsMin.value) || 3, maxLen = Number(refs.lsMax.value) || null;
  if (maxLen && maxLen < minLen) { refs.lsStatus.textContent = 'The longest loop must be at least the shortest.'; return; }
  refs.lsBtn.disabled = true;
  refs.lsResults.textContent = '';
  refs.lsStatus.textContent = 'Starting…';
  try {
    const { jobId } = await call('loop_search', { originalId: d.id, minLen, maxLen });
    const r = await pollJob(jobId, j => { refs.lsStatus.textContent = j.status === 'waiting' ? plainWait(j.message) : `${j.message || 'Searching'}… ${Math.round((j.progress || 0) * 100)}%`; });
    if (S.detail !== d && (!S.detail || S.detail.id !== d.id)) return;
    if (r.suggestedMax && !refs.lsMax.value) refs.lsMax.placeholder = `auto (${r.searched.maxLen})`;
    refs.lsStatus.textContent = `Searched ${r.frames} frames for loops of ${fmtSecs(r.searched.minLen)} to ${fmtSecs(r.searched.maxLen)}${r.searched.truncated ? ' (first part of a long clip)' : ''}. Best first:`;
    renderLoopResults(r.candidates);
  } catch (e) { refs.lsStatus.textContent = e.message; }
  refs.lsBtn.disabled = false;
}
const pollJob = (jobId, onUpdate) => backend.job(jobId, onUpdate);
const seamWords = c => ({ seamless: 'seamless', fade: 'seamless with a cross-fade', soft: 'a soft cross-fade with a brief double image', nearly: 'nearly seamless', jump: 'a visible jump' })[seamKind(c)];
function renderLoopResults(list) {
  const ul = refs.lsResults;
  ul.textContent = '';
  if (!list.length) { refs.lsStatus.textContent = 'No loop found in that length range.'; return; }
  const tw = 120, th = Math.round(tw * (natural.h / natural.w || 9 / 16));
  for (const c of list) {
    const a = el('canvas', { width: tw, height: th, class: 'ls-thumb', 'aria-hidden': 'true' });
    const b = el('canvas', { width: tw, height: th, class: 'ls-thumb', 'aria-hidden': 'true' });
    const text = `Start ${fmtT2(c.start)} → end ${fmtT2(c.end)} · ${fmtSecs(c.length)} (${c.frames} frames) · the join looks like ${c.ratio.toFixed(1)}× a normal frame step${(c.blend || 0) > 0 ? ` with a ${c.blend.toFixed(2)} s cross-fade` : ``} (${seamWords(c)})`;
    const use = el('button', { class: 'btn secondary', text: 'Use', 'aria-label': `Use: ${text}`, onclick: () => {
      setTrim(c.startFrame / srcFps(), c.frames / OUT_FPS(), c.startFrame / srcFps());
      if (view.zoomed) { fitView(); positionTrim(); drawFilmstrip(); }
      draftChanged(); updateSeamSoon();
      refs.lsStatus.textContent = `Using ${fmtT2(c.start)} → ${fmtT2(c.end)} (${fmtSecs(c.length)}).`;
    } });
    ul.append(el('li', { class: 'ls-item' }, el('div', { class: 'ls-thumbs' }, a, el('span', { 'aria-hidden': 'true', text: '↺' }), b),
      el('div', { class: 'small ls-text', text }), use));
    // end thumb = the last frame of the loop; start thumb = its first frame
    grabFrame(c.endFrame / srcFps(), tw, th).then(f => f && b.getContext('2d').drawImage(f, 0, 0));
    grabFrame(c.startFrame / srcFps(), tw, th).then(f => f && a.getContext('2d').drawImage(f, 0, 0));
  }
}

async function findLoopPoint() {
  const d = S.detail, dr = S.draft;
  refs.findBtn.disabled = true;
  refs.findResult.textContent = 'Searching ±2 s around the end…';
  try {
    const r = await call('loop_point', { originalId: d.id, start: dr.start, length: dr.length });
    const use = c => el('button', { class: 'btn ' + (c === r.best ? 'primary' : 'ghost'), text: `${c === r.best ? 'Use ' : ''}${fmtT2(c.end)} (${c.similarity}%)`,
      onclick: () => { setTrim(dr.start, c.length); draftChanged(); updateSeamSoon(); refs.findResult.textContent = `Loop end set to ${fmtT2(dr.start + dr.length)}.`; } });
    refs.findResult.textContent = '';
    refs.findResult.append(el('span', { text: `Best end: ${fmtT2(r.best.end)}, ${r.best.similarity}% like the start (now ${r.current.similarity}%). ` }), use(r.best),
      r.alternatives.length ? el('span', { class: 'muted', text: ' Others: ' }) : null, ...r.alternatives.map(use));
  } catch (e) { refs.findResult.textContent = e.message; }
  refs.findBtn.disabled = false;
}

let draftSaveT, draftKeyT, pendingDraftSave=null, draftSaving=Promise.resolve();
function savePendingDraft(){if(!pendingDraftSave)return draftSaving;const value=pendingDraftSave;pendingDraftSave=null;draftSaving=draftSaving.catch(()=>{}).then(()=>call('original_patch',value)).catch(error=>{pendingDraftSave||=value;throw error;});return draftSaving;}
function draftChangedSoon() { clearTimeout(draftKeyT); draftKeyT = setTimeout(()=>{draftKeyT=0;draftChanged();}, 300); }
function draftChanged() {
  updateFrameControls();
  updateTimeline();
  updateSeamSoon();
  S.est = { ...S.est, state: 'checking' };
  updateTuneState();
  scheduleEstimate();
  clearTimeout(draftSaveT);
  const id = S.detail.id, draft = JSON.parse(JSON.stringify(S.draft));
  pendingDraftSave={originalId:id,draft};
  draftSaveT = setTimeout(() => {draftSaveT=0;savePendingDraft().catch(()=>{});}, 800);
}
function resetDraft() {
  const d = S.detail;
  S.draft = initialDraft({ ...d, draft: null });
  syncTuneControls(); applyGeometry();
  if (view.zoomed) { fitView(); drawFilmstrip(); }
  if (media && media.tagName === 'VIDEO') media.currentTime = S.draft.start;
  draftChanged();
}

// ----- size estimate -----
let estT, estSeq = 0;
function scheduleEstimate(ms = 600) {
  clearTimeout(estT);
  if (!S.detail || !S.detail.canTune) return;
  S.est = { ...S.est, state: 'checking' };
  updateTuneState();
  estT = setTimeout(runEstimate, ms);
}
async function runEstimate() {
  const seq = ++estSeq, id = S.detail.id;
  const body = JSON.parse(JSON.stringify(S.draft));
  try {
    const r = await call('estimate', { originalId: id, settings: body });
    if (r.stale || seq !== estSeq || !S.detail || S.detail.id !== id) return;
    S.est = { state: 'done', r };
    applyGeometry();
  } catch (e) {
    if (seq !== estSeq) return;
    S.est = { ...S.est, state: e.code === 'busy' ? 'paused' : 'error', error: e.message };
  }
  updateTuneState();
}

function duplicateOf() {
  const d = S.detail, dr = S.draft;
  if (!d || !S.app) return null;
  return d.versions.find(v => {
    const s = v.settings;
    return v.status !== 'error' && String(v.notchKey || '').split('|')[0] === S.app.notchKey && (s.fps || S.app.deviceFps) === S.app.deviceFps &&
      s.start === (d.isImage ? 0 : dr.start) && s.length === dr.length && String(s.quality) === String(dr.quality) &&
      s.frame && s.frame.zoom === dr.frame.zoom && s.frame.x === dr.frame.x && s.frame.y === dr.frame.y &&
      !!s.corners.on === !!dr.corners.on && (!dr.corners.on || s.corners.r === dr.corners.r) &&
      (s.detail || 0) === (dr.quality === 'auto' ? 0 : dr.detail || 0) && (s.blend || 0) === (d.isImage ? 0 : (dr.blend || 0));
  });
}

function updateTuneState() {
  if (!refs.meter || !S.app) return;
  const est = S.est, budget = S.app.budget;
  const r = est.r;
  const fill = refs.meter.querySelector('.fill'), over = refs.meter.querySelector('.over');
  refs.meter.className = 'meter';
  refs.meterRetry.hidden = true;
  refs.meterNote.textContent = '';
  let stateText = '', stateCls = '';
  if (r) {
    const frac = r.bytes / budget;
    fill.style.width = Math.min(100, frac * 100) + '%';
    over.hidden = frac <= 1;
    over.style.width = Math.min(50, (frac - 1) * 100) / 0.75 * 0.75 + '%';
    refs.meterText.textContent = `${mb(r.bytes)} of ${mb(budget)}` + (S.app.budgetRoomLimited ? ' (all the free space your screen reports)' : S.app.budgetMeasured ? ' (measured on your screen)' : ' (estimated; check the screen to measure)');
    refs.meter.setAttribute('aria-label', `Uses about ${mb(r.bytes)} of ${mb(budget)}`);
    if (!r.fits) { refs.meter.classList.add('danger'); stateText = `Too big by ${mb(r.bytes - budget)}`; stateCls = 'danger'; }
    else if (frac > 0.97) { refs.meter.classList.add('warn'); stateText = 'Close to the limit'; stateCls = 'warn'; }
    else { stateText = 'Fits'; stateCls = 'ok'; }
    refs.meterNote.textContent = (r.mode === 'auto'
      ? (r.fits ? `Auto chose: quality ${r.quality}, ${r.detail ? `simplify ${r.detail}` : 'no simplify'}` : 'At the lowest quality, even fully simplified')
      : `Quality ${S.draft.quality}${S.draft.detail ? `, simplify ${S.draft.detail}` : ''}`) + ' · estimate, the final size may differ slightly';
  } else { fill.style.width = '0'; over.hidden = true; refs.meterText.textContent = `– of ${mb(budget)}`; }
  if (est.state === 'checking') { refs.meter.classList.add('checking'); stateText = 'Checking size…'; stateCls = ''; }
  if (est.state === 'paused') { refs.meter.classList.add('checking'); stateText = 'Size check paused while the screen is updating'; stateCls = ''; }
  if (est.state === 'error') { stateText = 'Couldn\'t check the size'; stateCls = 'danger'; refs.meterRetry.hidden = false; refs.meterNote.textContent = est.error || ''; }
  refs.meterState.textContent = stateText;
  refs.meterState.className = 'meter-state ' + stateCls;
  // save button (+ one suggested fix when it doesn't fit)
  const dup = duplicateOf();
  let reason = '', disabled = false, danger = false;
  refs.saveFix.hidden = true;
  if (dup) { disabled = true; reason = `Already saved as ${dup.label || 'v' + dup.n}.`; }
  else if (est.state === 'done' && r && !r.fits) {
    disabled = true; danger = true;
    if (r.mode === 'auto') {
      reason = 'This part is too long to fit, even at the lowest quality with fine detail simplified.' +
        (r.maxFitLength ? ` Shorten it to about ${fmtSecs(r.maxFitLength)}.` : '');
      if (r.maxFitLength && !S.detail.isImage) {
        refs.saveFix.hidden = false;
        refs.saveFix.textContent = `Shorten to ${fmtSecs(r.maxFitLength)}`;
        refs.saveFix.onclick = () => { setTrim(S.draft.start, r.maxFitLength); draftChanged(); };
      }
    } else reason = `Too big.${r.maxFitLength ? ` Shorten to about ${fmtSecs(r.maxFitLength)} or` : ''} choose Best that fits.`;
  }
  refs.saveBtn.disabled = disabled;
  // Push sends the saved twin of this tuning if there is one, else makes a version first.
  refs.pushBtn.disabled = !S.baseFor || (dup ? dup.status === 'error' || (dup.status === 'ready' && dup.tooBig) : disabled);
  refs.pushBtn.textContent = S.baseFor ? `Use for “${S.baseFor.name}”` : 'Use';
  refs.saveReason.textContent = dup && !danger ? '' : reason;
  refs.saveReason.className = 'reason' + (danger ? ' danger' : '');
}

async function saveVersion() {
  const d = S.detail;
  refs.saveBtn.disabled = true;
  try {
    await call('version_create', { originalId: d.id, settings: JSON.parse(JSON.stringify(S.draft)) });
    await loadDetail();
    loadOriginals();
    loadApp();
    pollSoon();
  } catch (e) {
    toast(e.message, true);
  }
  updateTuneState();
}

// ---------- versions ----------
function jobFor(vid, kind) { return S.jobs.find(j => j.versionId === vid && j.kind === kind && (j.status === 'running' || j.status === 'waiting')); }

function plainWait(msg) {
  if (!msg) return 'Waiting…';
  if (/screen operation/.test(msg)) return 'Waiting for the screen update to finish';
  if (/conversions/.test(msg)) return 'Waiting for the current preparation to finish';
  if (/previous conversion/.test(msg)) return 'Waiting for the previous version to finish';
  return msg.replace(/…$/, '');
}

function versionSummary(v) {
  const s = v.settings || {};
  const parts = [];
  if (!S.detail.isImage) parts.push(`${fmtTime(s.start)}–${fmtTime(s.start + s.length)}`);
  parts.push(fmtSecs((v.result && v.result.length) || s.length));
  parts.push(s.quality === 'auto' ? 'Best that fits' : `Quality ${s.quality}`);
  const f = s.frame;
  if (f && (f.zoom !== 1 || f.x !== 0.5 || f.y !== 0.5)) parts.push(f.zoom < 1 ? 'whole picture' : `zoom ${Math.round(f.zoom * 100)}%`);
  if (s.blend > 0) parts.push(`blend ${s.blend.toFixed(2)} s`);
  const det = s.quality === 'auto' ? (v.result && v.result.detail) || 0 : s.detail || 0;
  if (det > 0) parts.push(`simplified ${det}`);
  if (s.corners && s.corners.on) parts.push(`rounded ${s.corners.r}`);
  return parts.join(' · ');
}

function renderVersions(d) {
  const box = refs.versions;
  box.textContent = '';
  box.append(el('h3', { id: 'hVersions', text: `Versions (${d.versions.length})` }));
  if (!d.versions.length) { box.append(el('p', { class: 'muted small', text: 'No versions yet.' })); return; }
  const ul = el('ul', { class: 'vlist' });
  for (const v of d.versions) {
    const name = v.label || `v${v.n}`;
    const chips = [];
    if (v.onScreen) chips.push(el('span', { class: 'chip accent', text: v.onScreenIncomplete ? 'Embedded (didn\'t finish)' : 'Embedded' }));
    const isBase = !!(S.baseFor && S.baseFor.versionId === v.id);
    if (isBase) chips.push(el('span', { class: 'chip accent', text: `Picture of “${S.baseFor.name}”` }));
    const vjob = jobFor(v.id, 'version');
    if (v.status === 'ready') {
      if (v.tooBig) chips.push(el('span', { class: 'chip danger', text: 'Too big for the screen' }));
      if (v.speed) chips.push(el('span', { class: 'chip warn', text: `Plays too ${v.speed.plays} – re-make`, title: `Made at ${v.speed.fps} frames per second; the screen always plays ${S.app.deviceFps}.` }));
      if (v.oldNotch) chips.push(el('span', { class: 'chip warn', text: 'Made with older screen settings – re-make', title: 'The notch or rounded-corner setting has changed since this version was made.' }));
    }
    if (v.status === 'error') chips.push(el('span', { class: 'chip danger', text: 'Failed' }));
    const main = el('div', { class: 'vmain' }, el('div', { class: 'vtitle' }, el('b', { text: name }), el('span', { class: 'small muted', text: versionSummary(v) }), chips));
    if (v.status === 'queued' || v.status === 'converting') {
      const p = vjob ? (vjob.status === 'waiting' ? plainWait(vjob.message) : `Preparing… ${Math.round(vjob.progress * 100)}%`) : 'Waiting…';
      main.append(el('div', { class: 'progress', role: 'progressbar', 'aria-valuemin': '0', 'aria-valuemax': '100', 'aria-valuenow': String(Math.round((vjob ? vjob.progress : 0) * 100)), 'aria-label': `Preparing ${name}` },
        el('div', { style: `width:${Math.round((vjob ? vjob.progress : 0) * 100)}%` })), el('div', { class: 'vsub', text: p }));
    } else if (v.status === 'error') {
      main.append(el('div', { class: 'vsub', text: 'Couldn\'t prepare this version.' + (v.errorCode === 'tooLong' && v.maxFitLength ? ` Try about ${fmtSecs(v.maxFitLength)} or less.` : '') }),
        el('details', {}, el('summary', { text: 'Show details' }), el('pre', { text: v.error || '' })));
    } else {
      main.append(el('div', { class: 'vsub num', text: `${mb(v.result.size)} · made ${fmtWhen(v.created)}` }));
    }
    const btns = el('div', { class: 'vbtns' });
    if (v.status === 'ready') {
      btns.append(el('button', { class: 'btn secondary', text: '▶ Preview', 'aria-label': `Preview ${name}`, onclick: e => { e.stopPropagation(); closeVersions(false); showPreview(v.id); refs.stage.scrollIntoView({ block: 'nearest' }); } }));
      btns.append(el('button', { class: 'btn ' + (isBase ? 'secondary' : 'primary'), text: isBase ? 'In use' : (S.baseFor ? `Use for “${S.baseFor.name}”` : 'Use'), disabled: v.tooBig || isBase || !S.baseFor,
        title: v.tooBig ? 'Too big for the screen. Make a shorter version.' : 'Nothing is written to the screen.',
        onclick: e => { e.stopPropagation(); useAsBase(v.id); } }));
    }
    if (v.status === 'error') btns.append(el('button', { class: 'btn secondary', text: 'Try again', onclick: e => { e.stopPropagation(); retryVersion(v.id); } }));
    btns.append(el('span', { class: 'grow' }));
    btns.append(menuButton(`More actions for ${name}`, () => {
      const items = [{ label: 'Rename…', run: () => renameVersion(v) }];
      if (v.status === 'ready' && (v.oldNotch || v.speed)) items.push({ label: v.speed ? 'Re-make at the right speed' : 'Make updated copy', run: () => remakeVersion(v) });
      if (v.status !== 'queued' && v.status !== 'converting') items.push({ label: 'Delete version…', danger: true, run: () => deleteVersion(v) });
      return items;
    }));
    if (v.status === 'ready' && (v.oldNotch || v.speed)) {
      btns.insertBefore(el('button', { class: 'btn ghost', text: v.speed ? 'Re-make' : 'Make updated copy', onclick: e => { e.stopPropagation(); remakeVersion(v); } }), btns.querySelector('.grow'));
    }
    main.append(btns);
    const thumb = v.status === 'ready' ? el('img', { class: 'vthumb', src: backend.mediaUrl(v.id, 'thumb.jpg', 'v'), alt: '' }) : el('div', { class: 'vthumb' });
    const li = el('li', { class: 'vrow', 'data-vid': v.id }, thumb, main);
    li.onclick = () => { if (v.status === 'ready') { closeVersions(false); showPreview(v.id); } };
    ul.append(li);
  }
  box.append(ul);
  renderVersionsHighlight();
}
function renderVersionsHighlight() {
  if (!refs.versions) return;
  refs.versions.querySelectorAll('.vrow').forEach(r => r.classList.toggle('showing', r.dataset.vid === S.showing));
}

async function retryVersion(vid) {
  try { await call('version_retry', { versionId: vid }); await loadDetail(); pollSoon(); } catch (e) { toast(e.message, true); }
}
async function remakeVersion(v) {
  try { await call('version_remake', { versionId: v.id }); await loadDetail(); loadOriginals(); pollSoon(); } catch (e) { toast(e.message, true); }
}
async function renameVersion(v) {
  const input = el('input', { type: 'text', value: v.label || `v${v.n}`, id: 'vLabel', autofocus: true, maxLength: 40 });
  const val = await openDialog({ title: 'Rename version', body: el('div', { class: 'field' }, el('label', { for: 'vLabel', text: 'Name' }), input),
    actions: [{ label: 'Cancel', value: null, kind: 'secondary' }, { label: 'Rename', value: 'save', kind: 'primary' }],
    onOpen: ({ close }) => input.addEventListener('keydown', e => { if (e.key === 'Enter') close('save'); }) });
  if (val !== 'save') return;
  try { await call('version_patch', { versionId: v.id, label: input.value }); await loadDetail(); loadApp(); } catch (e) { toast(e.message, true); }
}
async function deleteVersion(v) {
  const name = v.label || `v${v.n}`;
  const body = el('div', {}, el('p', { text: `“${S.detail.name} · ${name}” will be deleted. The original clip is kept.` }),
    v.onScreen ? el('p', { text: 'This version is Embedded. Deleting it here does not remove it from the screen.' }) : null);
  const ok = await openDialog({ title: `Delete ${name}?`, body, actions: [{ label: 'Cancel', value: false, kind: 'secondary' }, { label: 'Delete', value: true, kind: 'danger' }] });
  if (!ok) return;
  try {
    await call('version_delete', { versionId: v.id });
    if (S.showing === v.id) showPreview(null);
    await loadDetail(); loadOriginals(); loadApp();
  } catch (e) { toast(e.message, true); }
}

// ---------- send ----------
async function startSend(vid) {
  try {
    const { jobId } = await call('version_send', { versionId: vid });
    sendJobs.add(jobId);
    backend.refresh();
  } catch (e) { toast(e.message, true); }
}

/**
 * "Use as this theme's base": takes the current tuning. If it is already saved as a version, that version is used; otherwise a
 * version is made first and used as soon as it is ready. Nothing is sent to the screen from here.
 */
let pendingPush = null; // { oid, vid }
async function pushToScreen() {
  const d = S.detail;
  if (!S.baseFor) return;
  const dup = duplicateOf();
  if (dup && dup.status === 'ready') return useAsBase(dup.id);
  try {
    let vid = dup && dup.id;
    if (!vid) vid = (await call('version_create', { originalId: d.id, settings: JSON.parse(JSON.stringify(S.draft)) })).versionId;
    pendingPush = { oid: d.id, vid };
    toast('Preparing; it is used as soon as it is ready.');
    await loadDetail();
    pollSoon();
  } catch (e) { toast(e.message, true); }
}

// The theme whose base is being chosen (set when Base settings opens): { oid, name, versionId, guidesOffset, home }
async function openBase(oid, info) {
  let v = info;
  try { v = await call('theme_get', { originalId: oid }); } catch { /* use what the editor had */ }
  S.baseFor = { oid, name: v.name, versionId: v.baseVersionId, guidesOffset: (v.theme && v.theme.guidesOffset) || null, home: S.view };
  await select((v.baseOwner && v.baseOwner.originalId) || oid);
}
async function useAsBase(vid) {
  const b = S.baseFor;
  if (!b) return;
  try {
    const v = await call('theme_set_base', { originalId: b.oid, versionId: vid });
    S.baseFor = { ...b, versionId: v.baseVersionId, guidesOffset: (v.theme && v.theme.guidesOffset) || null };
    await loadOriginals();
    const cur = S.currentTheme === b.oid;
    if (cur) { goHome('screen'); toast(`New picture for “${b.name}”.`, false, 6000); }
    else { S.selTheme = b.oid; goHome('mine'); toast(`“${b.name}” has a new picture.`, false, 6000); }
  } catch (e) { toast(e.message, true); }
}
function renderCtxBar() {
  const bar = refs.ctxBar;
  if (!bar) return;
  bar.textContent = '';
  const b = S.baseFor;
  bar.hidden = !b;
  if (!b) return;
  const cur = S.detail && S.detail.versions.find(v => v.id === b.versionId);
  const back = b.home === 'screen' || S.currentTheme === b.oid ? 'On screen' : 'My themes';
  bar.append(el('p', {}, el('b', { text: `Picture for “${b.name}”. ` }),
    'Pick any clip, make a version, then ', el('b', { text: `Use for “${b.name}”` }), '.' + (cur ? ` Now: ${S.detail.name} · ${cur.label || 'v' + cur.n}.` : '')),
    el('button', { class: 'btn secondary', text: `‹ Back to ${back}`, onclick: showGallery }));
}
function paintDetailGuides() {
  if (!refs.guides || !S.app) return;
  fillGuides(refs.guides, Ly.effectiveGuides(S.app.screenGuides, S.baseFor && S.baseFor.guidesOffset));
}

async function checkPendingPush() {
  if (!pendingPush) return;
  const p = pendingPush;
  let v = S.detail && S.detail.id === p.oid ? S.detail.versions.find(x => x.id === p.vid) : null;
  if (!v) { try { v = (await call('original_get', { originalId: p.oid })).versions.find(x => x.id === p.vid); } catch { v = null; } }
  if (!v) { pendingPush = null; return; }
  if (v.status === 'ready') { pendingPush = null; if (v.tooBig) { toast('The version came out too big for the screen, so it was not used.', true, 10000); return; } await useAsBase(v.id); }
  else if (v.status === 'error') { pendingPush = null; toast('Couldn\'t prepare the version: ' + (v.error || ''), true, 12000); }
}

// ---------- activity strip ----------
const sendJobs = new Set(); // send jobs started or seen running in this window
function renderActivity() {
  const box = $('activity');
  box.textContent = '';
  box.className = 'activity';
  const active = S.jobs.filter(j => j.status === 'running' || j.status === 'waiting');
  const now = Date.now();
  const finishedSends = S.jobs.filter(j => j.kind === 'send' && sendJobs.has(j.id) && (j.status === 'done' || j.status === 'error') && !S.dismissed.has(j.id));
  const outcome = finishedSends[finishedSends.length - 1];
  const rows = [];
  // One line, always the same height (the bar is always there, so nothing above it moves when jobs come and go).
  if (active.length) {
    const row = activityRow(active[0]);
    if (active.length > 1) row.insertBefore(el('span', { class: 'act-more muted', text: `+${active.length - 1} waiting` }), row.querySelector('.bar'));
    rows.push(row);
  }
  if (!active.length && outcome) {
    if (outcome.status === 'error') {
      box.classList.add('danger');
      const act = sendErrorAction(outcome);
      rows.push(el('div', { class: 'act-row' }, el('span', { class: 'txt wrap', text: `Couldn't send “${outcome.label}”. ${plainSendError(outcome)}` }),
        act, el('button', { class: 'btn ghost', text: 'Dismiss', onclick: () => { S.dismissed.add(outcome.id); renderActivity(); } })));
    } else if (outcome.result && outcome.result.restartFailed) {
      box.classList.add('warn');
      const rf = outcome.result.restartFailed;
      rows.push(el('div', { class: 'act-row' }, el('span', { class: 'txt wrap', text: `“${outcome.label}” was written, but the screen didn't answer after restarting. ${rf.failedUsb ? 'Windows lost its USB connection.' : ''}` }),
        rf.failedUsb ? el('button', { class: 'btn primary', text: 'Reconnect', onclick: runReconnect }) : el('button', { class: 'btn primary', text: 'Check again', onclick: checkScreen }),
        el('button', { class: 'btn ghost', text: 'Dismiss', onclick: () => { S.dismissed.add(outcome.id); renderActivity(); } })));
    } else if (outcome.result && outcome.result.serialWarning) {
      box.classList.add('warn');
      rows.push(el('div', { class: 'act-row' }, el('span', { class: 'txt wrap', text: `Embedded: “${outcome.label}”. The screen is not set up yet, so it switches off about 30 seconds after this app closes.` }),
        el('button', { class: 'btn primary', text: 'Set up screen', onclick: runSetup }),
        el('button', { class: 'btn ghost', text: 'Dismiss', onclick: () => { S.dismissed.add(outcome.id); renderActivity(); } })));
    } else if (now - outcome.finishedAt < 8000) {
      box.classList.add('ok');
      rows.push(el('div', { class: 'act-row' }, el('span', { class: 'txt', text: `Embedded: “${outcome.label}”.` +
        (outcome.result && outcome.result.restarted ? ' (The screen wasn\'t answering, so it was restarted first.)' : '') })));
      setTimeout(renderActivity, 8000 - (now - outcome.finishedAt) + 50);
    }
  }
  box.hidden = !rows.length; // shown only while it has something to say (no empty strip at the bottom)
  box.append(...rows);
  for (const t of box.querySelectorAll('.txt')) t.title = t.textContent;
}

function activityRow(j) {
  let text, pct = Math.round((j.progress || 0) * 100);
  if (j.status === 'waiting') text = `${j.label}: ${plainWait(j.message)}`;
  else if (j.kind === 'send') {
    const m = j.message || '';
    if (/Connecting/.test(m)) text = `Storing “${j.label}”: connecting`;
    else if (/intro/.test(m)) text = `Storing “${j.label}”: start-up logo · ${pct}%`;
    else if (/Starting playback/.test(m)) text = `Storing “${j.label}”: starting playback`;
    else if (/Restarting the screen to show/.test(m)) text = `“${j.label}”: restarting the screen`;
    else if (/Waiting for screen/.test(m)) text = `Storing “${j.label}”: waiting for the screen`;
    else {
      text = `Storing “${j.label}” as Embedded · ${pct}%`;
      const elapsed = (Date.now() - (j.runningSince || j.startedAt)) / 1000;
      if (elapsed > 10 && j.progress > 0.02) text += ` · ${fmtLeft(elapsed / j.progress - elapsed)}`;
      else if (j.bytesTotal) text += ` · ${fmtLeft(j.bytesTotal * (1 - j.progress) / (S.app.uploadRate || 50000))}`;
    }
  } else if (j.kind === 'version') text = `Preparing “${j.label}” · ${pct}%`;
  else text = `${j.label}${j.message ? ': ' + j.message : ''}`;
  return el('div', { class: 'act-row' }, j.status === 'running' ? el('span', { class: 'spinner', 'aria-hidden': 'true' }) : null,
    el('span', { class: 'txt', text }),
    j.status === 'running' ? el('div', { class: 'bar', role: 'progressbar', 'aria-label': text, 'aria-valuemin': '0', 'aria-valuemax': '100', 'aria-valuenow': String(pct) }, el('div', { style: `width:${pct}%` })) : null);
}

function plainSendError(j) {
  if (j.failedUsb || j.code === 'failedUsb') return 'Windows lost the screen\'s USB connection.';
  if (j.code === 'busy') return j.error || 'Something else is using the screen.';
  if (j.code === 'notFound') return 'Lost contact with the screen during the upload.';
  if (j.code === 'stalled') return 'The screen stopped responding. Unplugging isn\'t needed: press Reconnect, then send again.';
  if (j.code === 'noReply') return 'The screen is not responding, even after a restart. Restart it again, then send again.';
  return j.error || '';
}
function sendErrorAction(j) {
  if (j.failedUsb || j.code === 'failedUsb' || j.code === 'stalled') return el('button', { class: 'btn primary', text: 'Reconnect', onclick: runReconnect });
  if (j.code === 'notFound') return el('button', { class: 'btn primary', text: 'Check again', onclick: checkScreen });
  if (j.code === 'noReply') return el('button', { class: 'btn primary', text: 'Restart screen', onclick: runRestart });
  return el('button', { class: 'btn primary', text: 'Try again', onclick: () => { S.dismissed.add(j.id); startSend(j.versionId); } });
}

window.addEventListener('beforeunload', e => {
  if (S.jobs.some(j => j.kind === 'send' && j.status === 'running')) { e.preventDefault(); e.returnValue = ''; }
});

// ---------- activity (the backend pushes snapshots: events in the app, polling inside backend.js over HTTP) ----------
const pollSoon = () => backend.refresh();
const poll = () => backend.refresh();
let activityBusy = false, activityNext = null;
async function onActivity(a) {
  if (activityBusy) { activityNext = a; return; } // keep only the newest while one is being applied
  activityBusy = true;
  try {
    const before = new Map(S.jobs.map(j => [j.id, j.status]));
    S.jobs = a.jobs; S.deviceBusy = a.deviceBusy;
    const busy = a.jobs.some(j => j.status === 'running' || j.status === 'waiting');
    let reloadApp = false;
    for (const j of a.jobs) {
      if (j.kind === 'send' && (j.status === 'running' || j.status === 'waiting')) sendJobs.add(j.id);
      const was = before.get(j.id);
      if (was && was !== j.status && (j.status === 'done' || j.status === 'error')) {
        if (j.kind === 'send') {
          reloadApp = true;
          if (j.status === 'done') S.screen = { ...S.screen, state: j.result && j.result.serialWarning ? 'setup' : 'ready' };
        }
        if (j.kind === 'setup' || j.kind === 'apply') { reloadApp = true; }
      }
    }
    if (a.rev !== S.rev || reloadApp) await loadOriginals();
    else if (S.detail) {
      // progress only: refresh the versions list in place
      if (busy || before.size !== a.jobs.length) renderVersions(S.detail);
    }
    await checkPendingPush();
    if (reloadApp) await loadApp();
    if (S.est.state === 'paused' && !a.deviceBusy) scheduleEstimate(0);
    if (S.screen.retry && !a.deviceBusy) { S.screen.retry = false; checkScreen(); }
    renderTop(); renderBanners(); renderActivity();
  } catch {}
  activityBusy = false;
  if (activityNext) { const n = activityNext; activityNext = null; onActivity(n); }
}
backend.on('activity', onActivity);
backend.on('app', () => loadApp().catch(() => {}));

// ---------- device dialogs (setup / reconnect / restore) ----------
async function runDeviceJob(command, { title, text, confirmLabel, danger }) {
  const ok = await openDialog({ title, body: el('div', {}, text.map(t => el('p', { text: t }))),
    actions: [{ label: 'Cancel', value: false, kind: 'secondary' }, { label: confirmLabel, value: true, kind: danger ? 'danger' : 'primary' }] });
  if (!ok) return false;
  const status = el('p', { role: 'status', text: 'Starting…' });
  const bar = el('div', { class: 'progress' }, el('div'));
  let result = false;
  await openDialog({
    title, body: el('div', {}, status, bar), actions: [{ label: 'Close', value: true, kind: 'secondary' }],
    onOpen: async ({ close, setLocked }) => {
      setLocked(true);
      try {
        const { jobId } = await call(command);
        try {
          const res = await backend.job(jobId, j => {
            status.textContent = j.status === 'waiting' ? plainWait(j.message) : /administrator/.test(j.message) ? 'Waiting for your approval in the Windows prompt…' : j.message || 'Working…';
            bar.firstChild.style.width = Math.round((j.progress || 0) * 100) + '%';
          });
          status.textContent = (res && res.warning) || 'Done.'; result = true;
        } catch (e) { status.textContent = e.message; status.setAttribute('role', 'alert'); }
      } catch (e) { status.textContent = e.message; }
      setLocked(false);
      if (result) setTimeout(() => close(true), 2000);
    },
  });
  if (result) checkScreen();
  return result;
}
function runSetup() {
  return runDeviceJob('device_setup', {
    title: 'Set up the screen (one time)', confirmLabel: 'Set up screen',
    text: ['This switches the screen to Microsoft\'s built-in WinUSB driver so it keeps its display on without any program running.',
      'Windows will ask for permission. Only the screen\'s own USB entry is changed.',
      'The vendor app (LOVINGCOOL MONITOR) won\'t see the screen until you restore the original driver in Settings.'],
  });
}
function runRestart() {
  return runDeviceJob('device_restart', {
    title: 'Restart the screen', confirmLabel: 'Restart screen',
    text: ['The screen restarts (about 10 seconds), then plays its clip again. Nothing on it is changed.',
      'If it doesn\'t come back, CoolerScreen offers Reconnect.'],
  });
}
function runReconnect() {
  return runDeviceJob('device_reconnect', {
    title: 'Reconnect the screen', confirmLabel: 'Reconnect',
    text: ['This resets the cooler screen’s USB port so Windows can try to recognise it again.', 'Windows will ask for permission. Other USB ports are left alone.'],
  });
}
function runRestore() {
  return runDeviceJob('device_restore', {
    title: 'Restore the original driver?', confirmLabel: 'Restore original driver', danger: true,
    text: ['Windows installs the standard COM-port driver again, so LOVINGCOOL MONITOR can see the screen.',
      'Afterwards the screen switches its display off 30 seconds after any program closes its connection (this is how the vendor software works).',
      'Windows will ask for permission.'],
  });
}

// ---------- settings drawer ----------
let drawerReturn = null;
function openDrawer(section) {
  drawerReturn = document.activeElement;
  $('drawerBack').hidden = false;
  fillSettings();
  const d = $('drawer');
  d.focus();
  if (section) { const s = $(section); s.scrollIntoView({ block: 'start' }); const f = s.querySelector('button:not([hidden]), select, input'); if (f) f.focus(); }
}
function closeDrawer() {
  if (notchT) { clearTimeout(notchT); saveNotch(); }
  if (fitT) { clearTimeout(fitT); fitPersist(); }
  if (guidesT) { clearTimeout(guidesT); saveGuides(); }
  $('drawerBack').hidden = true;
  calibrate(null); // the guide lines leave the real screen
  fitEnd(); // the rulers leave the real screen
  draftMask = null; // unsaved notch/corner changes are dropped
  applyLiveMask();
  if (drawerReturn && document.contains(drawerReturn)) drawerReturn.focus();
}
$('drawerClose').onclick = closeDrawer;
$('drawerBack').addEventListener('mousedown', e => { if (e.target === $('drawerBack')) closeDrawer(); });
$('drawerBack').addEventListener('keydown', e => {
  if (e.key === 'Escape' && !document.querySelector('.backdrop')) { e.preventDefault(); closeDrawer(); }
  if (e.key === 'Tab') {
    const f = [...$('drawer').querySelectorAll('button, input, select, summary, a[href]')].filter(x => !x.disabled && x.offsetParent !== null);
    const first = f[0], last = f[f.length - 1];
    if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last.focus(); }
    else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first.focus(); }
  }
});

function renderScreenSettings() {
  const st = currentScreen();
  const text = {
    checking: 'Checking…', ready: 'Connected · set up (no background program needed)', setup: 'Connected, but the one-time setup is needed: the screen switches off 30 seconds after this app closes.',
    failedUsb: connectionMessage(st), notFound: connectionMessage(st),
    busy: connectionMessage(st), updating: 'Busy updating the screen.', error: connectionMessage(st),
    noReply: connectionMessage(st), connecting: connectionMessage(st),
  }[pillState()];
  $('scrStatus').textContent = text;
  $('scrSetup').hidden = st.state !== 'setup';
  $('scrSetupAgain').hidden = st.state !== 'ready';
  $('scrCheck').disabled = st.state === 'checking' || st.state === 'updating';
  $('scrRestart').disabled = st.state === 'checking' || st.state === 'updating';
  $('scrInfo').textContent = st.info ? Object.entries(st.info).filter(([k, v]) => typeof v !== 'object').map(([k, v]) => `${k}: ${v}`).join('\n') : (st.error || '–');
}
$('scrCheck').onclick = checkScreen;
$('scrRestart').onclick = () => runRestart();
// Settings > Restart CoolerScreen: the app quits like the tray's Quit (live hands back to Embedded) and opens again by itself.
$('appRestart').onclick = async () => {
  const b = $('appRestart'), m = $('appRestartMsg');
  b.disabled = true; m.textContent = 'Restarting…';
  try { await call('app_restart'); } catch (e) { b.disabled = false; m.textContent = e.message; }
};
$('scrSetup').onclick = runSetup;
$('scrSetupAgain').onclick = runSetup;
$('aRestore').onclick = runRestore;

// ---------- Settings > Live screen ----------
// Read current Live status without opening USB. It also drives the top status
// when Settings is closed. Change only status text, preserving focused inputs.
let connectionRefreshBusy = false, nextPresenceWatch = 0, lastPresenceKey = '';
async function refreshConnection() {
  if (connectionRefreshBusy || document.hidden || !S.app) return;
  connectionRefreshBusy = true;
  try {
    const previousApp = S.app, previousLive = S.app.live, before = currentScreen();
    const app = await call('app_state');
    // A setting or app event may have returned while this read was in flight.
    if (S.app !== previousApp || S.app.live !== previousLive) return;
    S.app.live = app.live;
    const now = currentScreen();
    if (now.source === 'live' && now.state === 'ready') {
      // Keep the last real recovery when Live is then turned off: an old failed
      // manual check must not become the banner again.
      S.screen = rememberedScreen(S.screen, S.app.live, { checking, deviceBusy: S.deviceBusy, jobs: S.jobs });
      clearTimeout(busyTimer);
    }
    renderTop(); renderScreenSettings(); renderLiveMessage();
    if (before.state !== now.state || before.source !== now.source || before.error !== now.error) renderBanners();
    else {
      // Retry countdown text can change without replacing a focused button.
      const text = $('banners').querySelector('[data-connection-state] p');
      if (text) text.textContent = connectionMessage(now);
    }
    if (Date.now() >= nextPresenceWatch && shouldWatchPresence(S.screen, S.app.live, { checking, deviceBusy: S.deviceBusy, jobs: S.jobs })) {
      nextPresenceWatch = Date.now() + 10000;
      const presence = await call('device_presence'); // enumeration only: never claims an interface
      const { key, arrived } = presenceArrival(lastPresenceKey, presence);
      lastPresenceKey = key;
      if (arrived && shouldWatchPresence(S.screen, S.app.live, { checking, deviceBusy: S.deviceBusy, jobs: S.jobs })) await checkScreen({ quiet: true });
    } else if (!shouldWatchPresence(S.screen, S.app.live, { checking, deviceBusy: S.deviceBusy, jobs: S.jobs })) lastPresenceKey = '';
  } catch (e) { /* the next tick retries status; never infer a USB fault from an IPC error */ }
  finally { connectionRefreshBusy = false; }
}
setInterval(refreshConnection, 2000);
document.addEventListener('visibilitychange', () => { if (!document.hidden) refreshConnection(); });
function renderLiveMessage() {
  const l = S.app?.live || {};
  $('lvMsg').textContent = liveMessage(l, currentScreen());
}
function renderLiveSettings() {
  const l = (S.app && S.app.live) || {};
  const c = l.settings || {};
  const st = l.status || {};
  $('lvOn').checked = !!c.inApp;
  $('lvFps').value = String(st.fps >= 45 || !st.fps ? 60 : 30);
  $('lvDestroy').checked = c.destroyWindow !== false;
  $('lvSleep').value = c.onSleep || 'loop';
  $('lvLock').value = c.onLock || 'loop';
  $('lvShut').checked = !!c.screenOffOnShutdown;
  $('lvAuto').checked = !!l.autostart;
  $('lvAuto').disabled = !c.inApp && !l.autostart;
  renderLiveSwitch();
  for (const id of ['lvFps', 'lvDestroy', 'lvSleep', 'lvLock', 'lvShut']) $(id).disabled = !c.inApp;
  $('lvFpsOv').checked = !!c.fpsOverlay;
  $('lvFpsOv').disabled = !c.inApp;
  renderLiveMessage();
}
async function liveSet(patch) {
  try { S.app = await call('live_set', { patch }); }
  catch (e) { toast(e.message, true); }
  S.screen = rememberedScreen(S.screen, S.app?.live, { checking, deviceBusy: S.deviceBusy, jobs: S.jobs });
  renderLiveSettings();
  renderTop(); renderBanners(); renderScreenSettings();
  if (typeof themes !== 'undefined') themes.appChanged();
  renderSelBar();
}
const liveBind = (id, key, get) => { $(id).onchange = () => liveSet({ [key]: get($(id)) }); };
liveBind('lvOn', 'inApp', e => e.checked);
liveBind('lvFps', 'fps', e => Number(e.value));
liveBind('lvDestroy', 'destroyWindow', e => e.checked);
liveBind('lvFpsOv', 'fpsOverlay', e => e.checked);
liveBind('lvSleep', 'onSleep', e => e.value);
liveBind('lvLock', 'onLock', e => e.value);
liveBind('lvShut', 'screenOffOnShutdown', e => e.checked);
liveBind('lvAuto', 'autostart', e => e.checked);

function fillSettings() {
  const a = S.app;
  if (!a) return;
  $('appVer').textContent = a.appVersion;
  if (a.build) {
    const t = a.build.builtAt ? new Date(a.build.builtAt * 1000) : null;
    const when = t ? t.toLocaleString([], { year: 'numeric', month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' }) : 'unknown';
    $('buildLine').textContent = `CoolerScreen ${a.appVersion} · build ${a.build.hash} · ${when}`;
  }
  maskToForm(a.mask);
  $('gDir').value = a.galleryDir; $('gMsg').textContent = '';
  $('cornerR').value = a.cornerRadius || 0; $('edSnap').checked = a.editorSnap !== false; $('thMsg').textContent = '';
  loadGuidesForm();
  loadFitForm();
  renderWeather();
  $('nMsg').textContent = '';
  const bg = $('notchBg');
  if (S.detail && S.detail.assets === 'ready') { bg.src = backend.mediaUrl(S.detail.id, 'frame.jpg'); bg.hidden = false; } else { bg.removeAttribute('src'); bg.hidden = true; }
  renderScreenSettings();
  drawNotch();
  renderSourcesSettings();
  renderLiveSettings();
}
function formMask() {
  return {
    mode: $('nMode').value,
    band: { edge: $('nEdge').value, px: Number($('nPx').value) || 0 },
    circle: { cx: Number($('nCx').value) || 0, cy: Number($('nCy').value) || 0, r: Number($('nR').value) || 0, feather: Number($('nF').value) || 0 },
  };
}
function maskToForm(m) {
  $('nMode').value = m.mode; $('nEdge').value = m.band.edge; $('nPx').value = $('nPxR').value = m.band.px;
  $('nCx').value = m.circle.cx; $('nCy').value = m.circle.cy; $('nR').value = m.circle.r; $('nF').value = m.circle.feather;
}
// While the Settings drawer is open, the main preview shows the unsaved notch setting too.
let draftMask = null;
const liveMask = () => draftMask || (S.app && S.app.mask);
function applyLiveMask() {
  if (!refs.notch || S.showing) return;
  styleNotch(refs.notch, liveMask());
  applyCorners(); // the corners follow the notch band's inner edge
}
let notchT = 0;
async function saveNotch(strict=false) {
  notchT = 0;
  try {
    S.app = await call('settings_update', { mask: formMask() });
    draftMask = formMask();
    applyLiveMask();
    await loadDetail();
    $('nMsg').textContent = '';
  } catch (e) { $('nMsg').textContent = e.message;if(strict)throw e; }
}
function drawNotch(edited) {
  const m = formMask();
  $('nBand').hidden = m.mode !== 'band';
  $('nCircle').hidden = m.mode !== 'circle';
  styleNotch($('notchShape'), m);
  $('notchShape').classList.add('edit');
  draftMask = m;
  applyLiveMask();
  if (edited === true) { clearTimeout(notchT); notchT = setTimeout(saveNotch, 400); } // every change is kept as it is made
  const eff = bandPx(m.band);
  $('nPxNote').textContent = m.mode === 'band' && eff && eff !== m.band.px ? `Applied as ${eff} px (rounded up to the screen's 16 px compression grid so the black area stays clean).` : '';
}
$('nPxR').oninput = () => { $('nPx').value = $('nPxR').value; drawNotch(true); };
$('nPx').oninput = () => { $('nPxR').value = $('nPx').value; drawNotch(true); };
for (const id of ['nMode', 'nEdge', 'nCx', 'nCy', 'nR', 'nF']) $(id).addEventListener('input', () => drawNotch(true));
$('gDir').onchange = async () => {
  try { S.app = await call('gallery_set_dir', { dir: $('gDir').value }); $('gDir').value = S.app.galleryDir; $('gMsg').textContent = ''; await loadOriginals(); }
  catch (e) { $('gMsg').textContent = e.message; }
};
$('gOpen').onclick = () => $('openFolder').click();
$('cornerR').onchange = async () => {
  try { S.app = await call('settings_update', { cornerRadius: Number($('cornerR').value) || 0 }); $('thMsg').textContent = ''; await loadOriginals(); if (typeof themes !== 'undefined') themes.appChanged(); }
  catch (e) { $('thMsg').textContent = e.message; }
};
$('edSnap').onchange = async () => {
  try { S.app = await call('settings_update', { editor: { snap: $('edSnap').checked } }); $('thMsg').textContent = ''; } catch (e) { $('thMsg').textContent = e.message; }
};

// Settings > Weather: the user types a city, picks one; nothing is looked up or stored before that.
function renderWeather() {
  const w = (S.app && S.app.weather) || { place: null, unit: 'C' };
  const box = $('wxPlace');
  box.textContent = '';
  if (w.place) box.append(el('span', { text: w.place.name }), ' ', el('button', { type: 'button', class: 'linkbtn', text: 'Clear', onclick: () => setPlace(null) }));
  else box.textContent = 'No city set.';
  $('wxUnit').value = w.unit || 'C';
  $('wxResults').textContent = '';
  $('wxMsg').textContent = '';
}
async function setPlace(p) {
  try {
    S.app = await call('weather_set_place', { place: p });
    renderWeather();
    if (typeof themes !== 'undefined') themes.appChanged();
  } catch (e) { $('wxMsg').textContent = e.message; }
}
async function searchPlace() {
  const q = $('wxQuery').value.trim();
  const list = $('wxResults');
  list.textContent = '';
  if (q.length < 2) { $('wxMsg').textContent = 'Type a city.'; return; }
  $('wxMsg').textContent = 'Searching…';
  $('wxGo').disabled = true;
  try {
    const { results } = await call('weather_search', { query: q });
    $('wxMsg').textContent = results.length ? '' : 'No match.';
    for (const r of results) {
      const text = [r.name, r.admin1, r.country].filter(Boolean).join(', ');
      list.append(el('li', {}, el('button', { type: 'button', class: 'btn ghost wx-pick', text, title: text, onclick: () => setPlace({ name: r.name, lat: r.lat, lon: r.lon, tz: r.tz }) })));
    }
  } catch (e) { $('wxMsg').textContent = e.message; }
  $('wxGo').disabled = false;
}
$('wxGo').onclick = searchPlace;
$('wxQuery').onkeydown = e => { if (e.key === 'Enter') { e.preventDefault(); searchPlace(); } };
$('wxUnit').onchange = async () => {
  try { S.app = await call('settings_update', { weather: { unit: $('wxUnit').value } }); if (typeof themes !== 'undefined') themes.appChanged(); } catch (e) { $('wxMsg').textContent = e.message; }
};

// Settings > Screen guides: where the screen curves and what hides part of it. While Live is on, the lines are drawn on the
// real screen as you change them (calibration_set), and they go away when the settings close.
const GC = { g: null, live: false, t: 0 };
const clone = o => JSON.parse(JSON.stringify(o));
function loadGuidesForm() {
  GC.g = Ly.clampGuides(clone((S.app && S.app.screenGuides) || {}));
  renderGuidesForm();
}
// The lines appear on the real screen once the user works in this section, and go when the settings close.
$('setGuides').addEventListener('focusin', () => calibrate(GC.g));
function calibrate(g) {
  clearTimeout(GC.t);
  const send = () => call('calibration_set', { guides: g }).then(r => { const n = $('gcLive'); if (n && g) n.textContent = r.live ? 'The lines are showing on the real screen now.' : 'Turn Live on (top bar) to see the lines on the real screen too.'; }).catch(() => {});
  if (g) GC.t = setTimeout(send, 60); else send();
}
let guidesT = 0;
async function saveGuides(strict=false) {
  guidesT = 0;
  try {
    S.app = await call('settings_update', { screenGuides: GC.g });
    if (typeof themes !== 'undefined') themes.appChanged();
    paintDetailGuides();
  } catch (e) { $('gcMsg').textContent = e.message;if(strict)throw e; }
}
function guidesChanged() { renderGuidePreview(); calibrate(GC.g); $('gcMsg').textContent = ''; clearTimeout(guidesT); guidesT = setTimeout(saveGuides, 400); }
function renderGuidePreview() {
  const o = S.originals.find(x => x.id === S.currentTheme) || S.originals[0];
  const img = $('gcImg');
  if (o && o.frameUrl) { img.src = o.frameUrl; img.hidden = false; } else { img.removeAttribute('src'); img.hidden = true; }
  fillGuides($('gcLayer'), Ly.clampGuides(GC.g));
}
let gcBand = null;
function renderGuidesForm() {
  const rows = $('gcRows');
  rows.textContent = '';
  const upd = [];
  // a label, a slider and a number box for one value
  const row = (label, get, set, min, max) => {
    const num = el('input', { type: 'number', min, max, step: 5, 'aria-label': label });
    const rng = el('input', { type: 'range', min, max, step: 5, 'aria-label': label });
    const show = () => { num.value = rng.value = get(); };
    const apply = n => { set(clamp(Number.isFinite(n) ? n : 0, min, max)); GC.g = Ly.clampGuides(GC.g); upd.forEach(f => f()); guidesChanged(); };
    rng.oninput = () => apply(Number(rng.value));
    num.oninput = () => apply(Number(num.value));
    upd.push(show); show();
    return el('div', { class: 'gcal-row' }, el('span', { class: 'small', text: label }), rng, num);
  };
  rows.append(row('Left', () => GC.g.left.size, v => { GC.g.left.size = v; }, 0, 800));
  rows.append(row('Right', () => GC.g.right.size, v => { GC.g.right.size = v; }, 0, 800));
  gcBand = createBandSlider({ span: 600, step: 5, onchange: ({ left, right }) => { GC.g.curve.left = left; GC.g.curve.right = right; GC.g = Ly.clampGuides(GC.g); upd.forEach(f => f()); guidesChanged(); } });
  const syncBand = () => { const c = 1200 + GC.g.curve.offset; gcBand.set(GC.g.curve.left, GC.g.curve.right, { left: c, right: 2400 - c }); };
  upd.push(syncBand);
  const pos = row('Position', () => GC.g.curve.offset, v => { GC.g.curve.offset = v; }, -600, 600);
  rows.append(el('div', { class: 'gcal-curve' }, el('h4', { class: 'group-label', text: 'Curve' }), pos,
    el('div', { class: 'gcal-row two' }, el('span', { class: 'small', text: 'Edges' }), gcBand.root)));
  syncBand();
  renderGuidePreview();
}
$('gcReset').onclick = () => { GC.g = Ly.clampGuides({ left: { size: 0 }, right: { size: 0 }, curve: { offset: 0, left: 0, right: 0 } }); renderGuidesForm(); guidesChanged(); };
// Settings > Screen fit: how many pixels the panel hides at each edge. While this section is in use the screen shows rulers
// (fit_cal); one press of - or + moves an edge by one sent pixel (about 1.4 screen pixels), the numbers are screen pixels.
// Leaving the section, or closing the settings, gives the screen back.
const FIT_KEYS = ['top', 'right', 'bottom', 'left', 'shiftX', 'shiftY'];
const FIT0 = { top: 0, right: 0, bottom: 0, left: 0, shiftX: 0, shiftY: 0 };
const FC = { saved: { ...FIT0 }, cur: { ...FIT0 }, sel: 0, on: false, t: 0, mode: lsGet('cs.fitMode') === 'theme' ? 'theme' : 'rulers' };
// [key, label, hint, shift?]: the four hidden edges, then the whole-picture shift (right and down are positive)
const FIT_ROWS = [['top', 'Top'], ['right', 'Right'], ['bottom', 'Bottom'], ['left', 'Left'],
  ['shiftY', 'Up / down', 'Moves the whole picture: − up, + down', true], ['shiftX', 'Left / right', 'Moves the whole picture: − left, + right', true]];
const fitSent = v => Math.sign(v) * Math.floor((Math.abs(v) * 7 + 5) / 10);   // screen px -> sent px (x 0.7, rounded)
const fitScreen = p => Math.sign(p) * Math.round(Math.abs(p) * 10 / 7);        // sent px -> screen px
// The current theme's stored base was made with another fit: say so here, with the way to fix it.
async function refreshFitStale() {
  const note = $('fitStale');
  note.hidden = true;
  if (!S.embeddedTheme) return;
  try {
    const t = await call('theme_get', { originalId: S.embeddedTheme });
    note.hidden = !(t.baseStale || t.baseUnstored);
    note.dataset.oid = S.embeddedTheme; note.dataset.name = t.name || '';
  } catch { /* the note stays hidden */ }
}
$('fitStore').onclick = () => { closeDrawer(); storeTheme($('fitStale').dataset.oid, $('fitStale').dataset.name); };
function loadFitForm() {
  refreshFitStale();
  FC.saved = { ...FIT0, ...((S.app && S.app.screenFit) || {}) };
  FC.cur = { ...FC.saved };
  renderFitRows();
  paintFitMode();
}
function renderFitRows() {
  const rows = $('fitRows');
  rows.textContent = '';
  FIT_ROWS.forEach(([key, label, hint, isShift], i) => {
    const step = d => {
      FC.sel = Math.min(i, 3);
      const lim = isShift ? [-14, 14] : [0, 140]; // sent px
      FC.cur[key] = clamp(fitScreen(clamp(fitSent(FC.cur[key]) + d, lim[0], lim[1])), isShift ? -20 : 0, isShift ? 20 : 200);
      $('fitMsg').textContent = '';
      paintFit(); fitSend(); fitKeep();
    };
    const minus = el('button', { type: 'button', class: 'iconbtn', 'aria-label': `${label} less`, text: '−' });
    const plus = el('button', { type: 'button', class: 'iconbtn', 'aria-label': `${label} more`, text: '+' });
    minus.onclick = () => step(-1);
    plus.onclick = () => step(1);
    const row = el('div', { class: 'fit-row', title: hint || '' }, el('span', { class: 'small', text: label }), minus, el('output', { class: 'fit-val' }), plus);
    row.addEventListener('focusin', () => { if (!isShift && FC.sel !== i) { FC.sel = i; paintFit(); fitSend(); } });
    rows.append(row);
  });
  paintFit();
}
// The numbers and the marked row, in place (so the focused button stays).
function paintFit() {
  [...$('fitRows').children].forEach((row, i) => {
    row.classList.toggle('sel', FC.mode === 'rulers' && FC.sel === i);
    row.querySelector('output').textContent = String(FC.cur[FIT_ROWS[i][0]]);
  });
}
function paintFitMode() {
  $('fitModeRulers').classList.toggle('on', FC.mode === 'rulers');
  $('fitModeTheme').classList.toggle('on', FC.mode === 'theme');
  $('fitModeRulers').setAttribute('aria-pressed', String(FC.mode === 'rulers'));
  $('fitModeTheme').setAttribute('aria-pressed', String(FC.mode === 'theme'));
  $('fitNote').textContent = FC.mode === 'rulers' ? 'The screen shows rulers. Move each red edge in until its first mark just shows.' : 'The screen shows your theme with these values.';
}
function fitMode(m) {
  FC.mode = m;
  lsSet('cs.fitMode', m);
  paintFitMode(); paintFit();
  FC.on = true;
  fitSend(0);
}
$('fitModeRulers').onclick = () => fitMode('rulers');
$('fitModeTheme').onclick = () => fitMode('theme');
function fitSend(delay = 25) {
  clearTimeout(FC.t);
  FC.t = setTimeout(() => {
    call('fit_cal', { action: 'start', mode: FC.mode, fit: FC.cur, sel: FC.sel }).then(r => { FC.on = !!(r.calibrating || r.tuning); }).catch(e => { $('fitMsg').textContent = e.message; });
  }, delay);
}
function fitEnd() {
  clearTimeout(FC.t);
  if (!FC.on) return;
  FC.on = false;
  call('fit_cal', { action: 'end' }).catch(() => {});
}
document.querySelector('#drawer .drawer-body').addEventListener('focusin', e => {
  if ($('setFit').contains(e.target)) { if (!FC.on) { FC.on = true; fitSend(); } }
  else fitEnd();
});
// Every press is kept: saved about 300 ms after the last one (the tuning picture on the screen carries on meanwhile).
let fitT = 0;
function fitKeep() { clearTimeout(fitT); fitT = setTimeout(fitPersist, 300); }
async function fitPersist(strict=false) {
  fitT = 0;
  try {
    S.app = await call('settings_update', { screenFit: FC.cur });
    FC.saved = { ...FIT0, ...S.app.screenFit };
    await loadOriginals().catch(() => {});
    if (typeof themes !== 'undefined') themes.appChanged();
    refreshFitStale();
  } catch (e) { $('fitMsg').textContent = e.message;if(strict)throw e; }
}
$('fitReset').onclick = () => { FC.cur = { ...FIT0 }; paintFit(); fitSend(); fitKeep(); };
// ---------- themes: the On screen editor and the Store dialog ----------
async function storeTheme(oid, name) {
  await themes.flush(); // unsaved edits first, so the picture is made from the saved theme
  const r = await runApply(oid, name);
  await loadApp().catch(() => {});
  await loadOriginals().catch(() => {});
  if (r === 'done') { toast(`Embedded: “${name}”.`); if (S.view === 'mine' && S.currentTheme === oid) { S.selTheme = null; goHome('screen'); } }
  else if (r === 'stopped') toast('Stopped. Embedded is unchanged.');
  return r;
}
// "Use this theme": live switches at once; Embedded is not touched.
async function useTheme(oid, name) {
  try { await themes.flush(); await call('theme_use', { originalId: oid }); } catch (e) { toast(e.message, true); return 'failed'; }
  await SP.end(); // the screen goes back to what live shows: now this theme
  await loadApp().catch(() => {});
  await loadOriginals().catch(() => {});
  S.selTheme = null; goHome('screen');
  const emb = S.embeddedTheme && S.embeddedTheme !== oid ? S.originals.find(o => o.id === S.embeddedTheme) : null;
  toast(liveOn() ? `Live: “${name}”.` + (emb ? ` Embedded: “${emb.name}”.` : '') : `Theme: “${name}”. Store it as Embedded here when you are happy.`, false, 6000);
  return 'done';
}
const themes = createThemes({ el, $, call, backend, S, toast, goHome, loadOriginals, openBase, storeTheme, openSettings: openDrawer });
// UC waits for autosaved editor changes before removing the embedded page.
window.__UC_COOLER_FLUSH__ = async ({navigation=false}={}) => {
  await backend.flushImports();await themes.flush();
  if(draftKeyT){clearTimeout(draftKeyT);draftKeyT=0;draftChanged();}
  clearTimeout(draftSaveT);draftSaveT=0;await draftSaving.catch(()=>{});await savePendingDraft();
  if(notchT){clearTimeout(notchT);await saveNotch(true);}
  if(guidesT){clearTimeout(guidesT);await saveGuides(true);}
  if(fitT){clearTimeout(fitT);await fitPersist(true);}
  // Submit transient-view cleanup before acknowledging navigation. The UC
  // parent owns these requests after the frame is removed; an unavailable USB
  // screen must not hold the editor here. Disable/Quit still await cleanup.
  if(navigation){clearTimeout(FC.t);clearTimeout(GC.t);SP.end().catch(()=>{});backend.endView().catch(()=>{});}
  else{await SP.end();await call('twin_watch',{on:false});await call('fit_cal',{action:'end'});}
};
initApply({ call, backend, plainWait });

// ---------- start ----------
upgradeGlyphs();
initSettingsNav();
(async function init() {
  // Opens on the view chosen last time (On screen by default); with nothing on the screen yet, on My themes (or Browse).
  const stored = lsGet(HOME_KEY);
  setView(['screen', 'mine', 'browse'].includes(stored) ? stored : 'screen');
  await loadApp();
  const sourcesLoaded = loadSources();
  await loadOriginals();
  await sourcesLoaded;
  if (!S.userNavigated && S.view === 'screen' && !S.currentTheme) setView(S.originals.length || !enabledSources().length ? 'mine' : 'browse');
  else if (!S.userNavigated && S.view === 'mine' && !S.originals.length && !S.adding.length && enabledSources().length) setView('browse');
  renderActivity();
  backend.start();
  checkScreen(); // once at launch; presence watches only resume a failed check when the device returns
})().catch(e => toast(e.message, true, 10000));
