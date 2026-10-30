// Combined editor: native profile files in UC, local drafts in browser previews.
import {toNative, fromNative, ProfileWriter} from './js/profile-model.js';
const T = window.__TAURI_INTERNALS__;
const invoke = (cmd, args = {}) => T ? T.invoke(cmd, args) : Promise.reject(new Error('Browser preview'));
const esc = s => String(s ?? '').replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
const STORE = 'uc.interface-prototype.v1';
const NAV = {home:'Home', cooler:'Cooler screen', dock:'Stream dock', dial:'Dial', general:'General', about:'About'};
const FACES = {text:'Large text', icon:'Icon and label', clock:'Clock', claude:'Claude allowance', codex:'Codex allowance', reset:'Reset countdown', trend:'Quota trend', cpu:'CPU', gpu:'GPU', ram:'RAM'};
const ACTIONS = {none:'No action', hotkey:'Hotkey', media:'Media key', launch:'Open app or file', url:'Open URL', text:'Type text', page:'Switch page', folder:'Open folder page', back:'Back', monitor:'Monitor Toggle'};
const ICONS = {
  home:'<path d="m3 10 9-7 9 7v10H3Z"/><path d="M9 20v-7h6v7"/>',
  cooler:'<rect x="3" y="5" width="18" height="13" rx="2"/><path d="M8 21h8m-4-3v3"/>',
  dock:'<rect x="3" y="3" width="18" height="18" rx="3"/><path d="M8 3v18M3 9h18M3 15h18m10-12v18"/>',
  dial:'<circle cx="12" cy="12" r="9"/><path d="m12 12 4-4"/>',
  general:'<path d="M4 6h16M4 12h16M4 18h16"/><circle cx="8" cy="6" r="2"/><circle cx="16" cy="12" r="2"/><circle cx="10" cy="18" r="2"/>',
  about:'<circle cx="12" cy="12" r="9"/><path d="M12 11v6m0-10v1"/>',
  launch:'<path d="M14 3h7v7m0-7-11 11"/><path d="M10 3H3v18h18v-7"/>',
  media:'<path d="m9 5 10 7-10 7Z"/>',
  monitor:'<rect x="3" y="4" width="18" height="13" rx="2"/><path d="M8 21h8m-4-4v4"/>',
};
const icon = name => `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${ICONS[name] || ICONS.launch}</svg>`;
const face = (kind = 'text', text = '') => ({kind, text});
const key = (a = face(), b = a, action = 'none', value = '') => ({a:{...a}, b:{...b}, action, value});
const newPage = name => ({name, keys:Array.from({length:14}, () => key())});
function initial() {
  const page = newPage('Main');
  page.keys[0] = key(face('claude'), face('trend'));
  page.keys[1] = key(face('codex'), face('reset'));
  page.keys[2] = key(face('clock'), face('clock'));
  page.keys[3] = key(face('icon','Monitor'), face('text','Display'), 'monitor');
  page.keys[4] = key(face('icon','Music'), face('text','Play'), 'media','Play / pause');
  page.keys[5] = key(face('icon','Browser'), face('text','Web'), 'url','https://example.com');
  page.keys[13] = key(face('text','Your workspace'), face('clock'));
  return {version:1, current:'home', profile:0, page:0, profiles:[{name:'Everyday', pages:[page]}]};
}
// Reject malformed drafts; stored action text is always inert data.
function valid(d) {
  return d?.version === 1 && Array.isArray(d.profiles) && d.profiles.length > 0 && d.profiles.length <= 50 &&
    d.profiles.every(p => typeof p.name === 'string' && Array.isArray(p.pages) && p.pages.length > 0 && p.pages.length <= 50 &&
      p.pages.every(pg => typeof pg.name === 'string' && Array.isArray(pg.keys) && pg.keys.length === 14 && pg.keys.every(k =>
        k && Object.hasOwn(ACTIONS,k.action) && typeof k.value === 'string' && [k.a,k.b].every(f =>
          f && Object.hasOwn(FACES,f.kind) && typeof f.text === 'string'))));
}
let draft = initial(), storageNote = '', state = null, selected = 0, selectedFace = 'a', previewFace = 'a', hasLocalPage = false;
let profilesReady = !T, exitGuardReady = !T, guardError = '', loadRunning = false, nativeSaving = false, recoveryNote = '', previewNote = '', previewGeneration = 0;
const writer = new ProfileWriter(invoke, (status) => {
  nativeSaving=status==='saving';
  storageNote=status==='failed' ? 'Changes could not be saved. Keep this window open and retry.' : '';
  showStorage();
});
try {
  const raw = T ? null : localStorage.getItem(STORE);
  if (raw) { const saved = JSON.parse(raw); if (!valid(saved)) throw new Error('Invalid draft'); draft = saved; hasLocalPage = true; }
} catch { storageNote = 'The saved prototype could not be loaded. A fresh preview is open.'; }
draft.profile = Number.isInteger(draft.profile) && draft.profiles[draft.profile] ? draft.profile : 0;
draft.page = Number.isInteger(draft.page) && draft.profiles[draft.profile].pages[draft.page] ? draft.page : 0;
draft.current = Object.hasOwn(NAV,draft.current) ? draft.current : 'home';
const pane = document.getElementById('pane');
const profile = () => draft.profiles[draft.profile];
const page = () => profile().pages[draft.page];
const activeKey = () => page().keys[selected];
function persist() {
  if (T) {
    if (profilesReady && exitGuardReady) writer.enqueue(toNative(draft));
    return;
  }
  try { localStorage.setItem(STORE,JSON.stringify(draft)); storageNote = ''; }
  catch { storageNote = 'Local storage is unavailable. Changes last only while this preview is open.'; }
  showStorage();
}
function showStorage() {
  const el = document.getElementById('storage-status');
  el.textContent = storageNote || guardError || (T ? !profilesReady ? 'Saved layouts are unavailable' : !exitGuardReady ? 'Preparing the editor…' : nativeSaving ? 'Saving changes…' : previewNote || recoveryNote || 'Changes saved in Unified Control' : 'Prototype changes saved on this device');
  el.classList.toggle('attention', !!storageNote);
  document.getElementById('storage-retry').hidden=!(T && writer.failed);
}
const options = (items, value) => Object.entries(items).map(([v,label]) => `<option value="${esc(v)}"${v===String(value)?' selected':''}>${esc(label)}</option>`).join('');
const field = (label, id, control) => `<div class="field"><label for="${id}">${label}</label>${control}</div>`;
function deviceCard(id, title, description, status) {
  return `<button class="device-card" data-go="${id}"><span class="device-icon">${icon(id)}</span><span class="device-info"><strong>${title}</strong><span>${description}</span></span><span class="chip">${status}</span><span aria-hidden="true">›</span></button>`;
}
function faceMarkup(f, k) {
  const label = f.text || (f.kind === 'icon' ? ACTIONS[k.action] : '');
  if (f.kind === 'claude' || f.kind === 'codex') return `<span class="allowance"><svg viewBox="0 0 100 100" aria-hidden="true"><circle class="ring outer" cx="50" cy="50" r="41"/><circle class="ring inner" cx="50" cy="50" r="29"/></svg><span class="allowance-values">—<br>—</span></span><span class="key-caption">${f.kind === 'claude' ? 'Claude' : 'Codex'} · No data</span>`;
  if (f.kind === 'clock') return `<span class="key-main clock-value">${new Date().toLocaleTimeString('en-AU',{hour:'2-digit',minute:'2-digit',hour12:false})}</span><span class="key-caption">Local time</span>`;
  if (f.kind === 'reset') return '<span class="key-main">—</span><span class="key-caption">Reset · No data</span>';
  if (f.kind === 'trend') return '<span class="trend-placeholder" aria-hidden="true"><i></i><i></i><i></i><i></i><i></i></span><span class="key-caption">Trend · No data</span>';
  if (f.kind === 'icon') return `<span class="key-icon">${icon(k.action)}</span><span class="key-caption">${esc(label)}</span>`;
  if (['cpu','gpu','ram'].includes(f.kind)) return `<span class="key-main">—</span><span class="key-caption">${f.kind.toUpperCase()} · No data</span>`;
  return `<span class="key-main">${esc(label || '+')}</span>${label?'':'<span class="key-caption">Empty key</span>'}`;
}
function dockMarkup() {
  return `<div class="dock-body"><div class="dock-brand">ULANZI <span>D200H · layout preview</span></div><div class="dock-grid">${page().keys.map((k,i) => `<button class="dock-key${i===13?' wide-key':''}${i===selected?' selected':''}" data-key="${i}" aria-label="${i===13?'Wide display':`Key ${i+1}`}, ${FACES[k[previewFace].kind]}" aria-pressed="${i===selected}">${faceMarkup(k[previewFace],k)}<span class="key-index">${i===13?'Wide':String(i+1).padStart(2,'0')}</span></button>`).join('')}</div></div>`;
}
function actionControl(k) {
  if (k.action === 'none') return '<p class="hint">This key has no action.</p>';
  if (k.action === 'monitor') return '<p class="hint">Monitor Toggle will use the existing monitor control behavior when connected.</p>';
  if (k.action === 'back') return '<p class="hint">Return from the current folder page.</p>';
  if (['page','folder'].includes(k.action)) return field('Target page','action-value',`<select id="action-value">${options(Object.fromEntries(profile().pages.map((p,i)=>[String(i),p.name])), k.value)}</select>`);
  if (k.action === 'media') return field('Media control','action-value',`<select id="action-value">${options(Object.fromEntries(['Play / pause','Next track','Previous track','Volume up','Volume down','Mute'].map(v=>[v,v])),k.value)}</select>`);
  const labels = {hotkey:'Key combination', launch:'App or file path', url:'URL', text:'Text'};
  const placeholders = {hotkey:'Ctrl + Shift + M',launch:'C:\\Path\\App.exe',url:'https://example.com',text:'Text to type'};
  return field(labels[k.action],'action-value',`<input id="action-value" maxlength="500" value="${esc(k.value)}" placeholder="${esc(placeholders[k.action])}">`);
}
function editorMarkup() {
  const k = activeKey(), f = k[selectedFace];
  return `<div class="editor-head"><div><span class="eyebrow">Selected display</span><h2>${selected===13?'Wide display':`Key ${selected+1}`}</h2></div><span class="chip">Draft</span></div>
    <div class="seg" role="group" aria-label="Edit face"><button data-edit-face="a" aria-pressed="${selectedFace==='a'}">Face A</button><button data-edit-face="b" aria-pressed="${selectedFace==='b'}">Face B</button></div>
    ${field('Presentation','face-kind',`<select id="face-kind">${options(FACES,f.kind)}</select>`)}
    ${['text','icon'].includes(f.kind)?field('Display text','face-text',`<input id="face-text" maxlength="40" value="${esc(f.text)}" placeholder="Add a label">`):''}
    ${['claude','codex','reset','trend'].includes(f.kind)?'<p class="hint">Account readers are not connected. No allowance or reset values are shown.</p>':''}
    <div class="section-divider"></div><h3>On press</h3>${field('Action','action-kind',`<select id="action-kind">${options(ACTIONS,k.action)}</select>`)}
    ${actionControl(k)}<p class="hint">Actions are edited here only. Preview presses never run them.</p>
    <div class="section-divider"></div><button class="btn full" data-do="preview-flip">Preview flip to Face ${previewFace==='a'?'B':'A'}</button><p class="hint">Exactly two faces. Choose the same presentation twice for a fixed display.</p>`;
}
function dockPage() {
  return `<div class="page-heading"><div><span class="eyebrow">Ulanzi D200H</span><h1>Stream dock</h1><p>Your keys, pages and actions in one place.</p></div><span class="chip accent">Interface preview</span></div>
    <div class="device-header"><span class="device-icon">${icon('dock')}</span><div><strong>Stream dock</strong><p>Hardware connection is not implemented yet.</p></div><span class="chip">Not connected</span></div>
    <div class="workspace-toolbar"><div class="profile-picker">${field('Profile','profile-picker',`<select id="profile-picker">${options(Object.fromEntries(draft.profiles.map((p,i)=>[String(i),p.name])),draft.profile)}</select>`)}</div><button class="btn" data-do="add-profile">Add profile</button><span class="toolbar-spacer"></span><span class="hint">Previewing Face ${previewFace.toUpperCase()}</span><button class="btn" data-do="preview-flip">Flip A / B</button></div>
    <div class="dock-workspace"><section class="preview-column" aria-label="Dock preview"><div id="dock-preview">${dockMarkup()}</div><div class="page-strip" role="group" aria-label="Dock pages">${profile().pages.map((p,i)=>`<button class="btn${draft.page===i?' active':''}" data-dock-page="${i}" aria-pressed="${draft.page===i}">${esc(p.name)}</button>`).join('')}<button class="btn ghost" data-do="add-page">+ Add page</button></div><div class="names-row">${field('Profile name','profile-name',`<input id="profile-name" maxlength="40" value="${esc(profile().name)}">`)}${field('Page name','page-name',`<input id="page-name" maxlength="40" value="${esc(page().name)}">`)}</div><p class="hint">Select a display to edit it. This preview sends nothing to your dock.</p></section><aside class="key-editor" aria-label="Key editor">${editorMarkup()}</aside></div>`;
}
function plannedPage(id) {
  const cooler = id === 'cooler';
  return `<div class="page-heading"><div><span class="eyebrow">${cooler?'Lovingcool SC70':'Ulanzi Dial'}</span><h1>${NAV[id]}</h1><p>${cooler?'Your cooler display, inside Unified Control.':'A place for your dial controls.'}</p></div><span class="chip">Planned</span></div>
    <div class="device-header"><span class="device-icon">${icon(id)}</span><div><strong>${NAV[id]}</strong><p>Engine integration is planned.</p></div><span class="chip">Not connected</span></div>
    <section class="empty-state"><span class="empty-icon">${icon(id)}</span><h2>${cooler?'The cooler page comes next':'Dial support follows the cooler'}</h2><p>${cooler?'Themes, the library and live controls will be brought into this page after the dock passes acceptance. Keep using the current CoolerScreen app.':'The dial will get its own controls here once the dock and cooler are working in the combined app.'}</p><p class="hint">This prototype has not checked or opened your device.</p><button class="btn" data-go="dock">Explore the dock editor</button></section>`;
}
function render() {
  document.querySelectorAll('.nav-item').forEach(b => {
    if (b.dataset.page===draft.current) b.setAttribute('aria-current','page'); else b.removeAttribute('aria-current');
  });
  let html = '';
  if (draft.current === 'dock') html = profilesReady && exitGuardReady ? dockPage() : '<div class="page-heading"><h1>Stream dock</h1></div><p>The editor is not ready. Existing layouts have been kept.</p><button class="btn" data-do="load-profiles">Try loading again</button>';
  else if (['cooler','dial'].includes(draft.current)) html = plannedPage(draft.current);
  else if (draft.current === 'home') html = `<div class="page-heading"><div><span class="eyebrow">Unified Control</span><h1>Your devices, together</h1><p>One place to shape your desktop controls.</p></div></div><div class="home-intro"><div><h2>Start with your stream dock</h2><p>Arrange keys, choose two faces and draft actions. CoolerScreen joins this frame after dock acceptance, followed by the dial.</p><button class="btn primary" data-go="dock">Open dock editor</button></div><span class="intro-icon">${icon('dock')}</span></div><h2 class="section-title">Devices</h2><div class="device-list">${deviceCard('dock','Stream dock','Ulanzi D200H · editable layout preview','Preview')}${deviceCard('cooler','Cooler screen','Lovingcool SC70 · engine integration next','Planned')}${deviceCard('dial','Dial','Ulanzi Dial · controls in a later step','Planned')}</div><p class="hint">Device states describe this prototype. Connected hardware has not been detected.</p>`;
  else if (draft.current === 'general') html = `<div class="page-heading"><div><span class="eyebrow">App settings</span><h1>General</h1><p>Common settings for all your devices.</p></div></div><section class="settings-list"><div class="setting-row"><div><strong>Start with Windows</strong><p>The existing app setting is shown below; editing it is not part of this prototype.</p></div><span class="chip">${state ? state.autostart?'On':'Off' : 'Not available in browser'}</span></div><div class="setting-row"><div><strong>Appearance</strong><p>Uses the current CoolerScreen dark theme.</p></div><span class="chip">Dark</span></div><div class="setting-row"><div><strong>Layout storage</strong><p>${T ? 'Profiles and pages are saved in the app data folder, with a startup backup.' : 'Preview drafts are stored in this browser. They are separate from app profile files.'}</p></div><span class="chip">Local only</span></div><div class="setting-row"><div><strong>Background behavior</strong><p>The installed shell uses a visible tray icon. Closing its window keeps it running; Quit stops it.</p></div></div></section>`;
  else html = `<div class="page-heading"><div><span class="eyebrow">About this app</span><h1>Unified Control</h1><p>One Windows app for your stream dock, cooler screen and dial.</p></div></div><section class="about-card"><h2>Combined interface prototype</h2><p>The app shell and simulated dock are built. This interface lets you explore the combined layout and draft key settings. Hardware control, action execution and usage readers are not connected.</p>${state?`<dl><dt>Version</dt><dd>${esc(state.appVersion)}</dd><dt>Build</dt><dd>${esc(state.build?.hash)} · ${esc(state.build?.commitDate)}</dd></dl>`:'<p class="hint">Browser preview · app build information is unavailable.</p>'}</section>`;
  pane.innerHTML = html; showStorage(); updateNativePreviews();
}
async function updateNativePreviews() {
  const generation=++previewGeneration;
  if (!T || !profilesReady || !exitGuardReady || draft.current!=='dock') return;
  const faces=page().keys.map(k=>({...k[previewFace]}));
  const clock=new Date().toLocaleTimeString('en-AU',{hour:'2-digit',minute:'2-digit',hour12:false});
  try {
    const frames=await invoke('key_previews',{faces,clock});
    if (generation!==previewGeneration || draft.current!=='dock') return;
    for (const frame of frames) {
      const el=document.querySelector(`[data-key="${frame.index}"]`);
      if (!el) continue;
      const img=document.createElement('img');
      img.className='rendered-key'; img.alt=FACES[faces[frame.index].kind];
      const url=URL.createObjectURL(new Blob([new Uint8Array(frame.png)],{type:'image/png'}));
      img.onload=img.onerror=()=>URL.revokeObjectURL(url);
      img.src=url;
      el.replaceChildren(img);
    }
    previewNote=''; showStorage();
  } catch {
    if (generation===previewGeneration) { previewNote='Key image preview is unavailable.'; showStorage(); }
  }
}
async function loadNativeProfiles() {
  if (loadRunning) return;
  loadRunning=true;
  try {
    const result=await invoke('profiles_state');
    draft=fromNative(result.state,draft.current);
    recoveryNote=result.notice || '';
    profilesReady=true; storageNote=''; selected=0;
  } catch {
    profilesReady=false; storageNote='Saved layouts could not be loaded. Existing files have been kept.';
  }
  loadRunning=false;
  render();
}
function navigate(id) {
  if (!Object.hasOwn(NAV,id)) return;
  hasLocalPage = true; draft.current = id; persist(); render(); pane.scrollTop = 0;
  invoke('settings_update',{patch:{page:id}}).then(s=>{state=s;}).catch(()=>{});
}
document.addEventListener('click', e => {
  const nav = e.target.closest('[data-page],[data-go]');
  if (nav) return navigate(nav.dataset.page || nav.dataset.go);
  const keyEl = e.target.closest('[data-key]');
  if (keyEl) { selected = Number(keyEl.dataset.key); render(); return; }
  const editFace = e.target.closest('[data-edit-face]');
  if (editFace) { selectedFace = editFace.dataset.editFace; previewFace = selectedFace; render(); return; }
  const pg = e.target.closest('[data-dock-page]');
  if (pg) { draft.page=Number(pg.dataset.dockPage); selected=0; persist(); render(); return; }
  const action = e.target.closest('[data-do]')?.dataset.do;
  if (action === 'load-profiles') { loadNativeProfiles(); return; }
  if (action === 'retry-save') { writer.completion=writer.drain(); return; }
  if (action === 'preview-flip') { previewFace = previewFace==='a'?'b':'a'; render(); }
  if (action === 'add-profile' && draft.profiles.length < 50) {
    draft.profiles.push({name:`Profile ${draft.profiles.length+1}`,pages:[newPage('Main')]});
    draft.profile=draft.profiles.length-1; draft.page=0; selected=0; persist(); render(); document.getElementById('profile-name')?.focus();
  }
  if (action === 'add-page' && profile().pages.length < 50) {
    profile().pages.push(newPage(`Page ${profile().pages.length+1}`)); draft.page=profile().pages.length-1; selected=0; persist(); render(); document.getElementById('page-name')?.focus();
  }
});
function refreshNames() {
  const option = document.querySelector(`#profile-picker option[value="${draft.profile}"]`);
  if (option) option.textContent = profile().name || 'Untitled profile';
  const pageButton = document.querySelector(`[data-dock-page="${draft.page}"]`);
  if (pageButton) pageButton.textContent = page().name || 'Untitled page';
  if (['page','folder'].includes(activeKey().action)) {
    document.querySelectorAll('#action-value option').forEach(o => { o.textContent = profile().pages[Number(o.value)]?.name || 'Untitled page'; });
  }
}
pane.addEventListener('change', e => {
  const {id,value} = e.target;
  // A blur-triggered change must not replace the next button before its click.
  if (id === 'action-value') { activeKey().value=value; persist(); return; }
  if (id === 'profile-name' || id === 'page-name') {
    const target = id === 'profile-name' ? profile() : page();
    target.name=value.trim() || (id === 'profile-name' ? 'Untitled profile' : 'Untitled page');
    e.target.value=target.name; refreshNames(); persist(); return;
  }
  if (!['profile-picker','face-kind','action-kind'].includes(id)) return;
  const k=activeKey();
  if (id === 'profile-picker') { draft.profile=Number(value); draft.page=0; selected=0; }
  else if (id === 'face-kind') { k[selectedFace].kind=value; previewFace=selectedFace; }
  else if (id === 'action-kind') { k.action=value; k.value=['page','folder'].includes(value)?'0':value==='media'?'Play / pause':''; }
  else return;
  persist(); render();
});
pane.addEventListener('input', e => {
  const {id,value} = e.target;
  if (id === 'face-text') { activeKey()[selectedFace].text=value; previewFace=selectedFace; document.getElementById('dock-preview').innerHTML=dockMarkup(); updateNativePreviews(); }
  else if (id === 'action-value') activeKey().value=value;
  else if (id === 'profile-name') { profile().name=value; refreshNames(); }
  else if (id === 'page-name') { page().name=value; refreshNames(); }
  else return;
  persist();
});
render();
if (T) {
  loadNativeProfiles();
  invoke('plugin:event|listen',{event:'uc:prepare-quit',target:{kind:'Any'},handler:T.transformCallback(async()=>{
    document.querySelector('.app').inert=true;
    const saved=await writer.flush();
    try {
      await invoke('editor_quit_ready',{saved});
      if (saved) return; // Remain inert until native exit; no last-moment edits.
      if (!saved) {storageNote='Changes could not be saved. Unified Control has stayed open so you can retry.';showStorage();}
    } catch {storageNote='Work is still running. Unified Control has stayed open.';showStorage();}
    document.querySelector('.app').inert=false;
  })}).then(()=>invoke('editor_session_ready',{ready:true})).then(()=>{exitGuardReady=true;guardError='';render();}).catch(()=>{exitGuardReady=false;guardError='The editor exit guard is unavailable. Layout editing is disabled.';render();});
  window.addEventListener('beforeunload',()=>invoke('editor_session_ready',{ready:false}).catch(()=>{}));
}
invoke('app_state').then(s => {state=s; if (!hasLocalPage && Object.hasOwn(NAV,s.page)) draft.current=s.page; render();}).catch(()=>{});
setInterval(() => {
  if (draft.current === 'dock') document.querySelectorAll('.clock-value').forEach(el => { el.textContent=new Date().toLocaleTimeString('en-AU',{hour:'2-digit',minute:'2-digit',hour12:false}); });
  if (T && profilesReady && draft.current==='dock' && page().keys.some(k=>k[previewFace].kind==='clock')) updateNativePreviews();
},1000);
