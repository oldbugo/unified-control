// Combined editor: native profile files in UC, local drafts in browser previews.
import {toNative, fromNative, ProfileWriter} from './js/profile-model.js';
import {EditorHistory} from './js/editor-history.js';
import {savePreviewPicture,previewPicture} from './js/preview-assets.js';
import {KeyPreviewRenderer} from './js/key-preview.js';
import {DevicePageFollower} from './js/device-page-follow.js';
import {CoolerPanel} from './js/cooler-panel.js';
import {AgentEditorLease} from './js/agent-editor.js';
import {installNavigationSearch} from './js/navigation-search.js';
// Diagnostic only: a rejected promise nobody awaited is logged, never shown.
window.addEventListener('unhandledrejection',event=>{console.warn('Unhandled promise rejection',event.reason);});
const T = window.__TAURI_INTERNALS__;
const invoke = (cmd, args = {}) => T ? T.invoke(cmd, args) : Promise.reject(new Error('Browser preview'));
const coolerPanel=new CoolerPanel(invoke,!!T,()=>{syncDeviceVisibility();updateHomeDeviceStatus();});
let refreshNavigationSearch=()=>{};
const esc = s => String(s ?? '').replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
const STORE = 'uc.interface-prototype.v1';
const NAV = {home:'Home', cooler:'Cooler screen', dock:'Stream dock', dial:'Dial', general:'General', about:'About'};
const FACES = {
  text:'Large text', icon:'Icon and label', picture:'Picture and label', clock:'Clock', date:'Date', claude:'Claude usage', codex:'Codex usage', reset:'Reset countdown', trend:'Quota trend', cpu:'CPU', gpu:'GPU', ram:'RAM'
};
const USAGE_VIEWS={
  rings:'Combined rings',bars:'Combined bars',trend:'Combined trends',tokens:'Daily tokens',resets:'Both reset countdowns','five-hour-ring':'Five-hour ring','weekly-ring':'Weekly ring','five-hour-number':'Five-hour number','weekly-number':'Weekly number','five-hour-reset':'Five-hour countdown','weekly-reset':'Weekly countdown','five-hour-time':'Five-hour reset time','weekly-time':'Weekly reset time','five-hour-trend':'Five-hour trend','weekly-trend':'Weekly trend','wide-dashboard':'Wide dashboard'
};
const FACE_ICONS={
  app:'App',folder:'Folder',back:'Back',play:'Play',pause:'Pause',next:'Next',previous:'Previous',volume:'Volume',mute:'Mute',monitor:'Monitor',globe:'Web',keyboard:'Keyboard',clock:'Clock'
};
const ACTIONS = {
  none:'No action', hotkey:'Hotkey', media:'Media key', launch:'Open app or file', url:'Open URL', text:'Type text', page:'Switch page', folder:'Open folder page', back:'Back', monitor:'Monitor Toggle', cooler:'Cooler screen'
};
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
const history=new EditorHistory();
const pendingMutations=new Set();
const pictureEpoch=new WeakMap();
let actionSearch='',deviceStatus=null;
let coolerControls=null,coolerChoicesLoaded=false,coolerChoicesLoading=false,coolerChoicesError='';
let followPhysicalPage=true,followPreferenceReady=!T,followPreferenceEpoch=0,savedFollowPhysicalPage=true,followPreferenceWrite=Promise.resolve();
let dockPollRunning=false,dockRequestTicket=0,lastDockStatusTicket=0,editorIntentEpoch=0,dockStatusUnavailable=false;
let editGesture=null;
let agentLayoutLocked=false,quitPreparing=false;
const pageFollower=new DevicePageFollower();
function remember(group=''){
  if(draft.profiles.some(p=>!p.id||p.pages.some(pg=>!pg.id)))toNative(draft);
  history.checkpoint(draft,group);
  const undo=document.querySelector('[data-do="undo"]');
  if(undo)undo.disabled=!history.available;
}
let profilesReady = !T, exitGuardReady = !T, guardError = '', loadRunning = false, nativeSaving = false, recoveryNote = '', previewNote = '', previewGeneration = 0;
const keyPreviews=new KeyPreviewRenderer();
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
  editorIntentEpoch++;
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
  el.textContent = storageNote || guardError || (T ? !profilesReady ? 'Saved layouts are unavailable' : !exitGuardReady ? 'Preparing the editor…' : agentLayoutLocked ? 'Applying agent layout changes…' : nativeSaving ? 'Saving changes…' : previewNote || recoveryNote || 'Changes saved in Unified Control' : 'Prototype changes saved on this device');
  el.classList.toggle('attention', !!storageNote);
  document.getElementById('storage-retry').hidden=!(T && writer.failed);
}
const options = (items, value) => Object.entries(items).map(([v,label]) => `<option value="${esc(v)}"${v===String(value)?' selected':''}>${esc(label)}</option>`).join('');
const field = (label, id, control) => `<div class="field"><label for="${id}">${label}</label>${control}</div>`;
function deviceCard(id, title, description, status) {
  return `<button class="device-card" data-go="${esc(id)}"><span class="device-icon">${icon(id)}</span><span class="device-info"><strong>${esc(title)}</strong><span data-device-description>${esc(description)}</span></span><span class="chip" data-device-status>${esc(status)}</span><span aria-hidden="true">›</span></button>`;
}
function homeDockSummary(){
  if(!T)return {description:'Ulanzi D200H · editable layout preview',status:'Preview'};
  if(dockStatusUnavailable)return {description:'Dock status could not be refreshed.',status:'Status unavailable'};
  return {description:deviceStatus?.message||'Waiting for dock status.',status:deviceStatus?.enabled===false?'Disabled':deviceStatus?.state||'Status unknown'};
}
function deviceEnabled(id){
  if(id==='dial')return false;
  if(!T)return true; // Browser previews expose the workspaces without enabling hardware.
  return id==='dock'?deviceStatus?.enabled===true:coolerPanel.state.enabled===true;
}
function deviceKnown(id){return !T||(id==='dock'?typeof deviceStatus?.enabled==='boolean':typeof coolerPanel.state.enabled==='boolean');}
function syncDeviceVisibility(){
  for(const id of ['dock','cooler','dial']){
    const button=document.querySelector(`.nav-item[data-page="${id}"]`);
    if(button)button.dataset.available=String(deviceEnabled(id));
  }
  refreshNavigationSearch();
  if(['dock','cooler','dial'].includes(draft.current)&&deviceKnown(draft.current)&&!deviceEnabled(draft.current))navigate('home');
}
function updateHomeDeviceStatus(){
  if(draft.current!=='home')return;
  const list=pane.querySelector('.device-list');if(!list)return;
  const cooler=coolerPanel.summary(),dock=homeDockSummary();
  for(const [id,title,description,status] of [
    ['dock','Stream dock',dock.description,dock.status],
    ['cooler','Cooler screen',cooler.description,cooler.status],
  ]){
    let card=list.querySelector(`.device-card[data-go="${id}"]`);
    if(!deviceEnabled(id)){card?.remove();continue;}
    if(!card){list.insertAdjacentHTML('beforeend',deviceCard(id,title,description,status));card=list.lastElementChild;}
    const text=card.querySelector('[data-device-description]'),chip=card.querySelector('[data-device-status]');
    if(text.textContent!==description)text.textContent=description;
    if(chip.textContent!==status)chip.textContent=status;
  }
  const empty=pane.querySelector('#devices-empty');if(empty)empty.hidden=!!list.children.length;
}
function faceMarkup(f, k) {
  const label = f.text || (f.kind === 'icon' ? ACTIONS[k.action] : '');
  if (f.kind === 'claude' || f.kind === 'codex') return `<span class="allowance"><svg viewBox="0 0 100 100" aria-hidden="true"><circle class="ring outer" cx="50" cy="50" r="41"/><circle class="ring inner" cx="50" cy="50" r="29"/></svg><span class="allowance-values">—<br>—</span></span><span class="key-caption">${f.kind === 'claude' ? 'Claude' : 'Codex'} · No data</span>`;
  if (f.kind === 'clock') return `<span class="key-main clock-value">${new Date().toLocaleTimeString('en-AU',{hour:'2-digit',minute:'2-digit',hour12:false})}</span><span class="key-caption">Local time</span>`;
  if (f.kind === 'reset') return '<span class="key-main">—</span><span class="key-caption">Reset · No data</span>';
  if (f.kind === 'trend') return '<span class="trend-placeholder" aria-hidden="true"><i></i><i></i><i></i><i></i><i></i></span><span class="key-caption">Trend · No data</span>';
  if (f.kind === 'icon') return `<span class="key-icon">${icon(k.action)}</span><span class="key-caption">${esc(label)}</span>`;
  if (f.kind === 'picture') return `${f.asset?`<img class="rendered-key" data-picture="${esc(f.asset)}" alt="${esc(f.text||'Picture')}">`:'<span class="key-main">—</span>'}<span class="key-caption">${esc(f.text||'Picture')}</span>`;
  if (f.kind === 'date') return `<span class="key-main">${new Date().toLocaleDateString('en-AU',{day:'2-digit',month:'short'})}</span>`;
  if (['cpu','gpu','ram'].includes(f.kind)) return `<span class="key-main">—</span><span class="key-caption">${f.kind.toUpperCase()} · No data</span>`;
  return `<span class="key-main">${esc(label || '+')}</span>${label?'':'<span class="key-caption">Empty key</span>'}`;
}
function dockMarkup() {
  return `<div class="dock-body"><div class="dock-brand">ULANZI <span>D200H · layout preview</span></div><div class="dock-grid">${page().keys.map((k,i) => `<button class="dock-key${i===13?' wide-key':''}${i===selected?' selected':''}" data-key="${i}" aria-label="${i===13?'Wide display':`Key ${i+1}`}, ${FACES[k[previewFace].kind]}" aria-pressed="${i===selected}">${faceMarkup(k[previewFace],k)}<span class="key-index">${i===13?'Wide':String(i+1).padStart(2,'0')}</span></button>`).join('')}</div></div>`;
}
const coolerSelectDraft=new WeakSet();
const COOLER_OPS={
  theme_next:'Next theme',theme_previous:'Previous theme',theme_select:'Select theme',live:'Live',brightness_set:'Set brightness',brightness_step:'Change brightness'
};
function coolerConfig(k=activeKey()){
  if(coolerSelectDraft.has(k))return {op:'theme_select',theme_id:''};
  try{
    const value=JSON.parse(k.value);
    if(value&&Object.hasOwn(COOLER_OPS,value.op))return value;
  }catch{}return {op:'theme_next'};
}
function coolerThemeOptions(selected){
  const themes=Array.isArray(coolerControls?.themes)?coolerControls.themes:[];
  const entries=themes.filter(t=>typeof t.id==='string'&&typeof t.name==='string').map(t=>[t.id,t.name]);
  if(selected&&!entries.some(([id])=>id===selected))entries.unshift([selected,`Unavailable theme (${selected})`]);
  return `<option value="" disabled>Choose a theme</option>`+entries.map(([id,name])=>`<option value="${esc(id)}" ${id===selected?'selected':''}>${esc(name)}</option>`).join('');
}
function coolerActionMarkup(k){
  const c=coolerConfig(k);
  let controls=field('Cooler control','cooler-operation',`<select id="cooler-operation">${options(COOLER_OPS,c.op)}</select>`);
  if(c.op==='theme_select')controls+=field('Theme','cooler-theme',`<select id="cooler-theme">${coolerThemeOptions(c.theme_id||'')}</select>`);
  if(c.op==='live')controls+=field('Live mode','cooler-live',`<select id="cooler-live">${options({on:'On',off:'Off',toggle:'Toggle'},c.mode||'toggle')}</select>`);
  if(c.op==='brightness_set')controls+=field('Brightness (%)','cooler-number',`<input id="cooler-number" type="number" min="0" max="100" step="1" value="${esc(c.value??50)}">`);
  if(c.op==='brightness_step')controls+=field('Brightness change (%)','cooler-number',`<input id="cooler-number" type="number" min="-100" max="100" step="1" value="${esc(c.delta??10)}">`);
  if(['brightness_set','brightness_step'].includes(c.op))controls+='<p id="cooler-number-error" class="hint" role="status"></p>';
  return controls+(coolerSelectDraft.has(k)?'<p class="hint">Choose a theme to save this action. The previous action stays saved until then.</p>':'')+'<p class="hint" id="cooler-action-status"></p><button class="btn" data-do="refresh-cooler-controls" type="button">Refresh cooler themes</button><p class="hint">The cooler must already be enabled. This action does not turn the device on. Selecting a theme changes the Live theme; it does not store an Embedded copy.</p>';
}
function paintCoolerChoices(){
  const status=document.getElementById('cooler-action-status');if(!status)return;
  status.textContent=!T?'Theme choices are available in the Windows app.':coolerChoicesLoading?'Loading cooler themes�':coolerChoicesError||(!coolerControls?.running?'Cooler unavailable. Saved theme choices are kept.':coolerControls.connected?'Cooler connected.':'Cooler engine running; screen disconnected.');
  const select=document.getElementById('cooler-theme');if(select){const id=coolerConfig().theme_id||'';select.innerHTML=coolerThemeOptions(id);select.value=id;}
}
async function updateCoolerControls(force=false){
  if(!document.getElementById('cooler-action-status'))return;
  if(!T||coolerChoicesLoading||coolerChoicesLoaded&&!force){paintCoolerChoices();return;}
  coolerChoicesLoading=true;coolerChoicesError='';paintCoolerChoices();
  try{coolerControls=await invoke('cooler_controls_state');}catch{
    coolerChoicesError='Cooler themes could not be read. Saved choices are kept; refresh to retry.';
  }
  finally{coolerChoicesLoaded=true;coolerChoicesLoading=false;paintCoolerChoices();}
}
function updateCoolerAction(target){
  if(!target.isConnected||document.getElementById(target.id)!==target)return;
  const k=activeKey(),c=coolerConfig(k);
  if(target.id==='cooler-operation'){
    const op=target.value;
    if(op==='theme_select'){coolerSelectDraft.add(k);render({preserveFocus:true});return;}coolerSelectDraft.delete(k);
    k.value=JSON.stringify(op==='live'?{op,mode:'toggle'}:op==='brightness_set'?{op,value:50}:op==='brightness_step'?{op,delta:10}:{op});
    persist();render({preserveFocus:true});return;
  }
  if(target.id==='cooler-theme'){if(!target.value)return;coolerSelectDraft.delete(k);c.theme_id=target.value;}
  else if(target.id==='cooler-live')c.mode=target.value;
  else if(target.id==='cooler-number'){
    if(!['brightness_set','brightness_step'].includes(c.op))return;
    const value=Number(target.value);
    if(target.value===''||!Number.isInteger(value)||value < (c.op==='brightness_step'?-100:0)||value>100||c.op==='brightness_step'&&value===0){
      target.setCustomValidity('Enter a whole number in range; a brightness change cannot be zero.');
      const error=document.getElementById('cooler-number-error');
      if(error)error.textContent='Enter a whole number in range; a brightness change cannot be zero. The previous value stays saved.';
      return;
    }target.setCustomValidity('');
    const error=document.getElementById('cooler-number-error');
    if(error)error.textContent='';
    c[c.op==='brightness_step'?'delta':'value']=value;
  }else return;
  const value=JSON.stringify(c);if(k.value!==value){k.value=value;persist();}
}
function actionControl(k) {
  if (k.action === 'none') return '<p class="hint">This key has no action.</p>';
  if (k.action === 'cooler') return coolerActionMarkup(k);
  if (k.action === 'monitor') return `<div id="monitor-targets"><p class="hint">Loading available monitors…</p></div><p class="hint">Choose the displays this key will toggle.</p>`;
  if (k.action === 'back') return '<p class="hint">Return from the current folder page.</p>';
  if (['page','folder'].includes(k.action)) return field('Target page','action-value',`<select id="action-value">${options(Object.fromEntries(profile().pages.map((p,i)=>[String(i),p.name])), k.value)}</select>`);
  if (k.action === 'media') return field('Media control','action-value',`<select id="action-value">${options(Object.fromEntries(['Play / pause','Next track','Previous track','Volume up','Volume down','Mute'].map(v=>[v,v])),k.value)}</select>`);
  const labels = {hotkey:'Key combination', launch:'App or file path', url:'URL', text:'Text'};
  const placeholders = {hotkey:'Ctrl + Shift + M',launch:'C:\\Path\\App.exe',url:'https://example.com',text:'Text to type'};
  if(k.action==='launch'){
    let config={path:k.value,args:[]};
    try{
      const parsed=JSON.parse(k.value);
      if(parsed&&typeof parsed.path==='string')config=parsed;
    }catch{}return field('App, file or folder','action-value',`<input id="action-value" maxlength="500" value="${esc(config.path)}" placeholder="${esc(placeholders.launch)}">`)+field('Arguments (one per line)','launch-args',`<textarea id="launch-args" rows="3" maxlength="3000">${esc((config.args||[]).join('\n'))}</textarea>`);
  }
  return field(labels[k.action],'action-value',`<input id="action-value" maxlength="500" value="${esc(k.value)}" placeholder="${esc(placeholders[k.action])}">`);
}
function editorMarkup() {
  const k = activeKey(), f = k[selectedFace];
  return `<div class="editor-head"><div><span class="eyebrow">Selected display</span><h2>${selected===13?'Wide display':`Key ${selected+1}`}</h2></div><span class="chip">Draft</span></div>
    <div class="seg" role="group" aria-label="Edit face"><button data-edit-face="a" aria-pressed="${selectedFace==='a'}">Face A</button><button data-edit-face="b" aria-pressed="${selectedFace==='b'}">Face B</button></div>
    ${field('Presentation','face-kind',`<select id="face-kind">${options(FACES,f.kind)}</select>`)}
    ${['text','icon','picture'].includes(f.kind)?field('Display text','face-text',`<input id="face-text" maxlength="40" value="${esc(f.text)}" placeholder="Add a label">`):''}
    ${f.kind==='icon'?field('Icon','face-icon',`<select id="face-icon">${options(FACE_ICONS,f.icon||'app')}</select>`):''}
    ${f.kind==='picture'?field('Picture','face-picture','<input id="face-picture" type="file" accept="image/png,image/jpeg">'):''}
    ${['claude','codex'].includes(f.kind)?field('Usage display','usage-view',`<select id="usage-view">${options(Object.fromEntries(Object.entries(USAGE_VIEWS).filter(([id])=>f.kind==='codex'||id!=='tokens')),f.usageView||'rings')}</select>`):''}
    ${['claude','codex','reset','trend'].includes(f.kind)?'<p class="hint">Unavailable or stale readings stay unknown. Claude needs a supporting Desktop Code session.</p>':''}
    <div class="section-divider"></div><h3>On press</h3>${field('Find an action','action-search',`<input id="action-search" type="search" value="${esc(actionSearch)}" placeholder="Search actions">`)}${field('Action','action-kind',`<select id="action-kind">${options(Object.fromEntries(Object.entries(ACTIONS).filter(([kind,label])=>kind===k.action||label.toLowerCase().includes(actionSearch.toLowerCase()))),k.action)}</select>`)}
    ${actionControl(k)}<p class="hint">Actions run from physical keys only. Preview presses never run them.</p>
    <div class="section-divider"></div><button class="btn full" data-do="preview-flip">Preview flip to Face ${previewFace==='a'?'B':'A'}</button>${T&&state?.fake?'<button class="btn full" data-do="simulate-press">Test simulated key press</button>':''}<p class="hint">Exactly two faces. Choose the same presentation twice for a fixed display.</p>`;
}
function dockPage() {
  return `<div class="page-heading"><div><span class="eyebrow">Ulanzi D200H</span><h1>Stream dock</h1><p>Your keys, pages and actions in one place.</p></div><span class="chip accent">${T?(state?.fake?'Simulated dock':'D200H'):'Browser preview'}</span></div>
    <div class="workspace-status"><span class="status-dot" aria-hidden="true"></span><span id="dock-state">${esc(deviceStatus?.state||(!T?'Preview':'Waiting'))}</span><span id="dock-message">${esc(deviceStatus?.message||(!T?'Your layout preview. Connect hardware in the Windows app.':'Waiting for dock status.'))}</span></div>
    <div class="dock-controls">${field('Brightness','dock-brightness',`<input id="dock-brightness" type="range" min="1" max="100" value="${deviceStatus?.brightness||50}" ${T?'':'disabled'}>`)}<label class="dock-switch">Follow physical page <input id="follow-physical-page" type="checkbox" ${followPhysicalPage?'checked':''} ${T?'':'disabled'}></label><span class="hint" id="dock-active-page"></span><button class="btn" data-do="undo" ${history.available?'':'disabled'}>Undo</button></div>
    <div class="workspace-toolbar"><div class="profile-picker">${field('Profile','profile-picker',`<select id="profile-picker">${options(Object.fromEntries(draft.profiles.map((p,i)=>[String(i),p.name])),draft.profile)}</select>`)}</div><button class="btn" data-do="add-profile">Add profile</button><span class="toolbar-spacer"></span><span class="hint">Previewing Face ${previewFace.toUpperCase()}</span><button class="btn" data-do="preview-flip">Flip A / B</button></div>
    <div class="dock-workspace"><section class="preview-column" aria-label="Dock preview"><div id="dock-preview">${dockMarkup()}</div><div class="page-strip" role="group" aria-label="Dock pages">${profile().pages.map((p,i)=>`<button class="btn${draft.page===i?' active':''}" data-dock-page="${i}" aria-pressed="${draft.page===i}">${esc(p.name)}</button>`).join('')}<button class="btn ghost" data-do="add-page">+ Add page</button></div><div class="names-row">${field('Profile name','profile-name',`<input id="profile-name" maxlength="40" value="${esc(profile().name)}">`)}${field('Page name','page-name',`<input id="page-name" maxlength="40" value="${esc(page().name)}">`)}</div><p class="hint">Select a display to edit it. Changes save as made and apply when the dock is enabled.</p></section><aside class="key-editor" aria-label="Key editor">${editorMarkup()}</aside></div>`;
}
function plannedPage(id) {
  const cooler = id === 'cooler';
  return `<div class="page-heading"><div><span class="eyebrow">${cooler?'Lovingcool SC70':'Ulanzi Dial'}</span><h1>${NAV[id]}</h1><p>${cooler?'Your cooler display, inside Unified Control.':'A place for your dial controls.'}</p></div><span class="chip">Planned</span></div>
    <div class="device-header"><span class="device-icon">${icon(id)}</span><div><strong>${NAV[id]}</strong><p>Engine integration is planned.</p></div><span class="chip">Not connected</span></div>
    <section class="empty-state"><span class="empty-icon">${icon(id)}</span><h2>${cooler?'The cooler page comes next':'Dial support follows the cooler'}</h2><p>${cooler?'Themes, the library and live controls will be brought into this page after the dock passes acceptance. Keep using the current CoolerScreen app.':'The dial will get its own controls here once the dock and cooler are working in the combined app.'}</p><p class="hint">${cooler?'CoolerScreen':'Dial'} controls are not integrated yet.</p><button class="btn" data-go="dock">Explore the dock editor</button></section>`;
}
function editorFocus() {
  const element=document.activeElement;
  if(!pane.contains(element))return null;
  let selector=element.id?`#${CSS.escape(element.id)}`:'';
  if(!selector)for(const name of ['data-key','data-edit-face','data-do','data-dock-page']){
    if(element.hasAttribute(name)){selector=`[${name}="${CSS.escape(element.getAttribute(name))}"]`;break;}
  }
  if(!selector)return null;
  return {selector,index:[...pane.querySelectorAll(selector)].indexOf(element)};
}
function restoreEditorFocus(focus,fallback) {
  if(!focus||document.activeElement!==document.body)return;
  let element=pane.querySelectorAll(focus.selector)[focus.index];
  if((!element||element.disabled||!element.getClientRects().length)&&fallback)element=pane.querySelector(fallback);
  if(element&&!element.disabled&&!element.closest('[inert],[hidden]')&&element.getClientRects().length)element.focus({preventScroll:true});
}
function render({preserveFocus=false,focusFallback=''}={}) {
  const focus=preserveFocus?editorFocus():null;
  document.querySelector('.preview-label').textContent=T?(state?.fake?'Simulated workspace':'Device workspace'):'Browser preview';
  pane.classList.toggle('cooler-workspace',draft.current==='cooler');
  document.querySelectorAll('.nav-item').forEach(b => {
    if (b.dataset.page===draft.current) b.setAttribute('aria-current','page'); else b.removeAttribute('aria-current');
  });
  if(draft.current==='cooler'&&coolerPanel.active(pane)){coolerPanel.refresh();showStorage();return;}
  coolerPanel.unmount();
  let html = '';
  if (draft.current === 'dock') html = profilesReady && exitGuardReady ? dockPage() : '<div class="page-heading"><h1>Stream dock</h1></div><p>The editor is not ready. Existing layouts have been kept.</p><button class="btn" data-do="load-profiles">Try loading again</button>';
  else if (draft.current==='cooler') html=coolerPanel.markup();
  else if (draft.current==='dial') html = plannedPage(draft.current);
  else if (draft.current === 'home') html = `<div class="home-intro"><div><span class="eyebrow">YOUR WORKSPACE</span><h1>Your devices, together.</h1><p>Make the desk your own. Your enabled devices keep working as you move between them.</p><button class="btn" data-go="general">Manage devices ${icon('general')}</button></div><div class="workspace-art" aria-hidden="true"><span class="art-screen">${icon('cooler')}</span><span class="art-dock">${icon('dock')}</span><span class="art-orbit"></span></div></div><div class="section-heading"><div><span class="eyebrow">DEVICE WORKSPACE</span><h2>Devices</h2></div><span class="local-badge"><i></i>Runs locally</span></div><div class="device-list"></div><section id="devices-empty" class="empty-devices"><span class="device-icon">${icon('general')}</span><div><h3>Choose your devices</h3><p>Enable a device in General to add it to your workspace.</p></div><button class="btn primary" data-go="general">Open settings</button></section><p class="workspace-footnote">${T?'Closing the window keeps enabled devices running in the tray.':'Browser previews do not connect to hardware.'}</p>`;
  else if (draft.current === 'general') html = `
    <div class="page-heading"><div><span class="eyebrow">MAKE IT YOURS</span><h1>General</h1><p>Your devices and the way Unified Control works.</p></div></div>
    <div class="settings-section-heading"><h2>Devices</h2><p>Enabled devices appear in Home and the sidebar. They keep working in the background.</p></div>
    <section class="settings-list device-settings"><div class="device-setting"><span class="device-icon">${icon('dock')}</span><div class="device-setting-copy"><strong>Stream dock</strong><p>Ulanzi D200H · keys, profiles and actions</p><p id="dock-message" class="setting-status">${esc(deviceStatus?.message||'Waiting for dock status.')}</p></div><span id="dock-state" class="chip">${esc(deviceStatus?.state||'Unknown')}</span><label class="device-setting-control"><input id="dock-enabled" type="checkbox" role="switch" aria-label="Enable Stream dock" ${deviceStatus?.enabled?'checked':''} ${T?'':'disabled'}></label></div><div id="cooler-settings"></div></section>
    <p class="settings-note">Dial support is planned. It will appear here when it is available.</p>
    <div class="settings-section-heading"><h2>Application</h2><p>Local controls, always within reach.</p></div>
    <section class="settings-list"><div class="setting-row"><div><strong>Start with Windows</strong><p>Start quietly in the tray. Off unless you enable it.</p></div>
    <input id="autostart-enabled" type="checkbox" role="switch" aria-label="Start with Windows" ${state?.autostart?'checked':''} ${!T||state?.fake?'disabled':''}></div>
    <div class="setting-row"><div><strong>Freeze diagnostics</strong><p>Save a local timestamp every minute while Unified Control runs, including in the tray. Turn this off when troubleshooting is finished.</p></div><input id="freeze-diagnostics" type="checkbox" role="switch" aria-label="Freeze diagnostics" ${state?.freezeDiagnostics?'checked':''} ${!T?'disabled':''}></div>
    <div class="setting-row"><div><strong>Local agent access</strong><p>Let connected MCP clients read device status and configure dock layouts. Changes appear here.</p></div><input id="agent-access" type="checkbox" role="switch" aria-label="Local agent access" ${state?.agentAccess?'checked':''} ${!T?'disabled':''}></div>
    <div class="setting-row"><div><strong>Layout storage</strong><p>${T?'Profiles and pages stay in the app data folder, with a startup backup.':'Preview drafts are stored in this browser, separately from app profiles.'}</p></div><span class="chip">Local only</span></div>
    <div class="setting-row"><div><strong>Background behavior</strong><p>Close the window to keep enabled devices running in the tray. Use Quit to stop them; active work finishes first.</p></div></div></section>`;
  else html = `<div class="page-heading"><div><span class="eyebrow">About this app</span><h1>Unified Control</h1><p>One Windows app for your stream dock, cooler screen and dial.</p></div></div><section class="about-card"><h2>One workspace. Your devices.</h2><p>Shape your dock, manage your cooler display and keep your controls close. Choose enabled devices in General. Dial support follows later.</p>${state?`<dl><dt>Version</dt><dd>${esc(state.appVersion)}</dd><dt>Build</dt><dd>${esc(state.build?.hash)} · ${esc(state.build?.commitDate)}</dd></dl>`:'<p class="hint">Browser preview · app build information is unavailable.</p>'}</section>`;
  pane.innerHTML = html;
  if(draft.current==='cooler')coolerPanel.mount(pane);
  if(draft.current==='general')coolerPanel.mountSettings(pane.querySelector('#cooler-settings'));
  showStorage();
  updateNativePreviews();
  updatePictures();
  updateMonitors();
  updateCoolerControls();
  updateDeviceStatus();
  syncDeviceVisibility();
  restoreEditorFocus(focus,focusFallback);
}
async function updatePictures(){
  if(T)return;
  for(const img of document.querySelectorAll('[data-picture]')){
    try{
      const blob=await previewPicture(img.dataset.picture);
      if(blob&&img.isConnected){
        const url=URL.createObjectURL(blob);
        img.onload=img.onerror=()=>URL.revokeObjectURL(url);
        img.src=url;
      }
    }catch{}
  }
}
const MONITOR_DISCOVERY_ERRORS=new Set([
  'Monitor discovery returned an invalid result','Monitor discovery returned no display list',
  'Monitor discovery exceeded its display limit','A discovered monitor has no stable key',
  'A discovered monitor has an invalid stable key','Monitor action lock is unavailable',
  'Monitor Toggle resources are unavailable','Windows system folder is unavailable',
  'Monitor Toggle helper could not be started','Monitor operation failed; check its configured targets and saved layout',
  'Monitor helper result exceeds its size limit','Monitor helper returned an invalid result',
  'Monitor information unavailable','Monitor discovery requires Windows',
]);
async function updateMonitors(){
  const container=document.getElementById('monitor-targets');if(!container)return;
  if(!T){container.innerHTML='<p class="hint">Monitor selection is available in the app.</p>';return;}
  const edited=activeKey();
  container.textContent='Loading available monitors…';
  try{
    const displays=await invoke('monitor_list');if(!container.isConnected||edited!==activeKey())return;
    let chosen=[];try{chosen=JSON.parse(edited.value).targets||[];}catch{}
    for(const key of chosen)if(!displays.some(display=>display.key===key))displays.push({key,label:'Saved display (currently unavailable)'});
    container.innerHTML=displays.map(display=>`<label class="monitor-choice"><input type="checkbox" data-monitor="${esc(display.key)}" ${chosen.includes(display.key)?'checked':''}> ${esc(display.label)}</label>`).join('')||'<p class="hint">No configurable monitors found.</p>';
  }catch(error){
    if(!container.isConnected||edited!==activeKey())return;
    const candidate=typeof error==='string'?error:error?.error||error?.message;
    const message=MONITOR_DISCOVERY_ERRORS.has(candidate)?candidate:'Monitor information is unavailable.';
    container.innerHTML=`<p class="hint" role="status">${esc(message)}</p><button class="btn" data-do="retry-monitors">Retry monitor detection</button>`;
  }
}
function updateDeviceStatus(){
  syncDeviceVisibility();
  const message=document.getElementById('dock-message'),chip=document.getElementById('dock-state'),enabled=document.getElementById('dock-enabled');
  if(message)message.textContent=deviceStatus?.message||'Dock disabled';
  if(chip)chip.textContent=deviceStatus?.state||'Disabled';
  if(enabled)enabled.checked=!!deviceStatus?.enabled;
  const active=document.getElementById('dock-active-page');
  if(active)active.textContent=deviceStatus?.currentPage?`On dock: ${deviceStatus.currentProfileName||''} / ${deviceStatus.currentPageName||deviceStatus.currentPage}`:'';
  document.querySelectorAll('[data-key]').forEach(key=>key.classList.toggle('action-error',Number(key.dataset.key)===deviceStatus?.errorKey));
  updateUsageStatus();
  updateHomeDeviceStatus();
}
function updateUsageStatus(){
  // General has a dock status row too; usage belongs to the dock workspace.
  if(!pane.querySelector('#dock-preview'))return;
  const selected=deviceStatus?.usageStatus?.providers?.filter(item=>item.selected)||[];
  let note=document.getElementById('dock-usage-status');
  if(!note&&document.getElementById('dock-message')){
    note=document.createElement('p');
    note.id='dock-usage-status';
    note.className='hint';
    note.setAttribute('role','status');
    pane.querySelector('.dock-controls').after(note);
  }
  if(!note)return;note.hidden=!selected.length;
  const lines=selected.map(item=>`${item.provider==='claude'?'Claude':'Codex'}: ${item.message||item.status}`);
  if(selected.length&&deviceStatus.usageStatus.reader?.code!=='ready')lines.unshift(deviceStatus.usageStatus.reader?.message||'AI usage reader is unavailable.');
  const text=lines.join(' ');if(note.textContent!==text)note.textContent=text;
}
function followDeliveredPage(){
  const target=pageFollower.take(draft,{enabled:followPhysicalPage,ready:profilesReady&&exitGuardReady&&followPreferenceReady,
    busy:agentLayoutLocked||writer.running||!!writer.pending||writer.failed||nativeSaving||pendingMutations.size>0||!!(editGesture?.isConnected&&document.activeElement===editGesture),visible:draft.current==='dock'});
  if(!target)return false;
  draft.profile=target.profile;draft.page=target.page;selected=0;selectedFace='a';previewFace='a';actionSearch='';
  render();return true;
}
function applyDockStatus(result,ticket,intent){
  if(intent!==editorIntentEpoch||ticket<lastDockStatusTicket)return;
  lastDockStatusTicket=ticket;deviceStatus=result;dockStatusUnavailable=false;pageFollower.observe(result);
  if(!followDeliveredPage())updateDeviceStatus();
}
async function pollDockState(){
  if(!T||dockPollRunning)return;
  dockPollRunning=true;
  const ticket=++dockRequestTicket,intent=editorIntentEpoch;
  try{applyDockStatus(await invoke('dock_state'),ticket,intent);}catch{
    if(intent===editorIntentEpoch&&ticket>=lastDockStatusTicket){dockStatusUnavailable=true;updateHomeDeviceStatus();}
  }
  finally{dockPollRunning=false;}
}
function setFollowPhysicalPage(enabled){
  const epoch=++followPreferenceEpoch;followPhysicalPage=enabled;followPreferenceReady=true;
  if(enabled)pageFollower.requestSync();else pageFollower.manualSelection();
  followDeliveredPage();
  followPreferenceWrite=followPreferenceWrite.catch(()=>{}).then(()=>invoke('settings_update',{patch:{followPhysicalPage:enabled}})).then(result=>{
    savedFollowPhysicalPage=result.followPhysicalPage!==false;
    if(epoch===followPreferenceEpoch){
      state=result;
      followPhysicalPage=savedFollowPhysicalPage;
      const control=document.getElementById('follow-physical-page');
      if(control)control.checked=followPhysicalPage;
    }
  }).catch(()=>{
    if(epoch===followPreferenceEpoch){
      followPhysicalPage=savedFollowPhysicalPage;
      previewNote='The page-follow setting could not be saved.';
      showStorage();
      const control=document.getElementById('follow-physical-page');
      if(control)control.checked=followPhysicalPage;
    }
  });
}
async function updateNativePreviews() {
  const generation=++previewGeneration;
  if (!T || !profilesReady || !exitGuardReady || draft.current!=='dock') return;
  const faces=page().keys.map(k=>({...k[previewFace]}));
  const clock=new Date().toLocaleTimeString('en-AU',{hour:'2-digit',minute:'2-digit',hour12:false});
  try {
    const frames=await invoke('key_previews',{faces,clock});
    if (generation!==previewGeneration || draft.current!=='dock') return;
    await Promise.all(frames.map(async frame => {
      const el=document.querySelector(`[data-key="${frame.index}"]`);
      if (!el) return;
      await keyPreviews.render(el,frame.png,FACES[faces[frame.index].kind],()=>generation===previewGeneration && draft.current==='dock' && document.querySelector(`[data-key="${frame.index}"]`)===el);
    }));
    if(generation!==previewGeneration || draft.current!=='dock')return;
    previewNote=''; showStorage();
  } catch {
    if (generation===previewGeneration) { previewNote='Key image preview is unavailable.'; showStorage(); }
  }
}
async function loadNativeProfiles() {
  if (loadRunning) return;
  loadRunning=true;
  try {
    // A window can open while an agent transaction started from the tray. Wait
    // for that accepted transaction before taking the editor's initial revision.
    const deadline=Date.now()+20_000;
    while((await invoke('app_state')).agentBusy===true){
      if(Date.now()>=deadline)throw new Error('A layout transaction is still running');
      await new Promise(resolve=>setTimeout(resolve,100));
    }
    const result=await invoke('profiles_state');
    adoptNativeLayout(result);
    recoveryNote=result.notice || '';
    profilesReady=true; storageNote=''; selected=0;
  } catch {
    profilesReady=false; storageNote='Saved layouts could not be loaded. Existing files have been kept.';
  }
  loadRunning=false;
  render();followDeliveredPage();
}
function adoptNativeLayout(result) {
  writer.adoptRevision(result.revision);
  draft=fromNative(result.state,draft.current);
  history.clear();editorIntentEpoch++;editGesture=null;
  profilesReady=true;selected=Math.min(selected,13);
}
function updateEditorLock(){document.querySelector('.app').inert=agentLayoutLocked||quitPreparing;showStorage();}
const agentEditor=new AgentEditorLease({
  invoke,
  busy:()=>quitPreparing||!profilesReady||!exitGuardReady||writer.failed||!!(editGesture?.isConnected&&document.activeElement===editGesture),
  flush:async()=>{await Promise.all([...pendingMutations]);await followPreferenceWrite;return writer.flush();},
  lock:locked=>{agentLayoutLocked=locked;updateEditorLock();},
  adopt:async result=>{adoptNativeLayout(result);render();},
  reload:async()=>{
    try{adoptNativeLayout(await invoke('profiles_state'));render();}
    catch(error){profilesReady=false;storageNote='The accepted layout could not be reloaded. Reload layouts before editing.';render();throw error;}
  },
  onError:()=>{previewNote='The agent layout update could not finish. Your saved layout is kept.';showStorage();},
});
async function navigate(id) {
  if (!Object.hasOwn(NAV,id)||(['dock','cooler','dial'].includes(id)&&!deviceEnabled(id))) return;
  if(draft.current==='cooler'&&id!=='cooler'){
    pane.inert=true;
    try{await coolerPanel.prepareLeave({navigation:true});}catch(error){previewNote=error.message;showStorage();return;}finally{pane.inert=false;}
  }
  hasLocalPage = true; draft.current = id;editGesture=null;
  // Sidebar navigation changes only the app section. Saving profiles here could
  // send an editor-inspected page back to a physically navigated dock.
  if(T)editorIntentEpoch++;else persist();
  render(); pane.scrollTop = 0;
  if(id==='dock'){pageFollower.requestSync();followDeliveredPage();}
  invoke('settings_update',{patch:{page:id}}).then(s=>{state=s;}).catch(()=>{});
}
document.addEventListener('click', e => {
  const nav = e.target.closest('[data-page],[data-go]');
  if (nav) return navigate(nav.dataset.page || nav.dataset.go);
  const keyEl = e.target.closest('[data-key]');
  if (keyEl) { selected = Number(keyEl.dataset.key); render({preserveFocus:true}); return; }
  const editFace = e.target.closest('[data-edit-face]');
  if (editFace) { selectedFace = editFace.dataset.editFace; previewFace = selectedFace; render({preserveFocus:true}); return; }
  const pg = e.target.closest('[data-dock-page]');
  if (pg) { editGesture=null;pageFollower.manualSelection();draft.page=Number(pg.dataset.dockPage); selected=0; persist(); render(); return; }
  const action = e.target.closest('[data-do]')?.dataset.do;
  if (action === 'load-profiles') { loadNativeProfiles(); return; }
  if (action === 'retry-save') { writer.completion=writer.drain(); return; }
  if (action === 'refresh-cooler-controls') { updateCoolerControls(true); return; }
  if (action === 'retry-monitors') { updateMonitors(); return; }
  if (action === 'undo') {
    const previous=history.undo();
    if(previous){
      const current=draft.current,profileId=profile().id,pageId=page().id;
      draft=previous;draft.current=current;
      const samePage=profileId&&pageId&&profile().id===profileId&&page().id===pageId;
      selected=samePage?Math.max(0,Math.min(selected,page().keys.length-1)):0;
      persist();render({preserveFocus:true,focusFallback:`[data-key="${selected}"]`});
    }
    return;
  }
  if (action === 'simulate-press') {
    invoke('dock_test_button',{index:selected,pressed:true}).then(()=>invoke('dock_test_button',{index:selected,pressed:false})).catch(()=>{previewNote='The simulated dock is not enabled.';showStorage();});
    return;
  }
  if (action === 'preview-flip') { previewFace = previewFace==='a'?'b':'a'; render({preserveFocus:true}); }
  if (action === 'add-profile' && draft.profiles.length < 50) {
    remember();
    draft.profiles.push({name:`Profile ${draft.profiles.length+1}`,pages:[newPage('Main')]});
    draft.profile=draft.profiles.length-1; draft.page=0; selected=0; persist(); render(); document.getElementById('profile-name')?.focus();
  }
  if (action === 'add-page' && profile().pages.length < 50) {
    remember();
    profile().pages.push(newPage(`Page ${profile().pages.length+1}`));
    draft.page=profile().pages.length-1;
    selected=0;
    persist();
    render();
    document.getElementById('page-name')?.focus();
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
function markEditGesture(element){
  // Text entry spans keystrokes; committed dropdowns are protected by the writer.
  if(element.matches('textarea,input')&&!['checkbox','file','range','button','submit'].includes(element.type))editGesture=element;
}
pane.addEventListener('focusout',e=>{if(e.target===editGesture)editGesture=null;});
pane.addEventListener('change', e => {
  const {id,value} = e.target;
  markEditGesture(e.target);
  if(id==='follow-physical-page'){setFollowPhysicalPage(e.target.checked);return;}
  if(id==='autostart-enabled'){
    invoke('autostart_set',{enabled:e.target.checked}).then(result=>{state=result;}).catch(()=>{
      e.target.checked=!!state?.autostart;previewNote='The Windows startup setting could not be changed.';showStorage();
    });
    return;
  }
  if(id==='freeze-diagnostics'){
    invoke('settings_update',{patch:{freezeDiagnostics:e.target.checked}}).then(result=>{state=result;}).catch(()=>{
      e.target.checked=!!state?.freezeDiagnostics;previewNote='Freeze diagnostics could not be saved.';showStorage();
    });
    return;
  }
  if(id==='agent-access'){
    invoke('settings_update',{patch:{agentAccess:e.target.checked}}).then(result=>{state=result;}).catch(()=>{
      e.target.checked=!!state?.agentAccess;previewNote='Local agent access could not be saved.';showStorage();
    });
    return;
  }
  if(id==='dock-enabled'){
    const ticket=++dockRequestTicket,intent=++editorIntentEpoch;
    invoke('dock_configure',{patch:{enabled:e.target.checked}}).then(result=>{applyDockStatus(result,ticket,intent);updateNativePreviews();}).catch(()=>{
      previewNote='The dock setting could not be applied.';showStorage();updateDeviceStatus();
    });
    return;
  }
  if(id==='dock-brightness'){
    const ticket=++dockRequestTicket,intent=++editorIntentEpoch;
    invoke('dock_configure',{patch:{brightness:Number(value)}}).then(result=>{applyDockStatus(result,ticket,intent);}).catch(()=>{previewNote='Brightness could not be applied.';showStorage();});
    return;
  }
  if(e.target.dataset.monitor){
    remember();
    activeKey().value=JSON.stringify({
      targets:[...document.querySelectorAll('[data-monitor]:checked')].map(el=>el.dataset.monitor),mode:'toggle'
    });
    persist();
    return;
  }
  if(['cooler-operation','cooler-theme','cooler-live','cooler-number'].includes(id)){remember();updateCoolerAction(e.target);return;}
  if(id==='face-picture'){
    const file=e.target.files?.[0];
    if(!file)return;
    editorIntentEpoch++;
    const dest=activeKey()[selectedFace],epoch=Symbol();
    pictureEpoch.set(dest,epoch);
    remember();
    const mutation=(async()=>{
      try{
        if(file.size>4*1024*1024)throw new Error();const asset=T?await invoke('asset_import',{bytes:[...new Uint8Array(await file.arrayBuffer())]}):await savePreviewPicture(file);if(pictureEpoch.get(dest)!==epoch)return;dest.asset=asset;persist();render();
      }catch{
        previewNote='Choose a valid PNG or JPEG of at most 4 MiB and 4096 pixels per side.';showStorage();
      }
    })();
    pendingMutations.add(mutation);
    mutation.finally(()=>pendingMutations.delete(mutation));
    return;
  }
  if(id==='usage-view'||id==='face-icon'){
    remember();
    activeKey()[selectedFace][id==='usage-view'?'usageView':'icon']=value;
    persist();
    render({preserveFocus:true});
    return;
  }
  // A blur-triggered change must not replace the next button before its click.
  if (id === 'action-value'||id==='launch-args') {updateActionValue();persist();return;}
  if (id === 'profile-name' || id === 'page-name') {
    const target = id === 'profile-name' ? profile() : page();
    target.name=value.trim() || (id === 'profile-name' ? 'Untitled profile' : 'Untitled page');
    e.target.value=target.name; refreshNames(); persist(); return;
  }
  if (!['profile-picker','face-kind','action-kind'].includes(id)) return;
  if(id!=='profile-picker')remember();
  const k=activeKey();
  if (id === 'profile-picker') { editGesture=null;pageFollower.manualSelection();draft.profile=Number(value); draft.page=0; selected=0; }
  else if (id === 'face-kind') {
    k[selectedFace].kind=value;
    if(!['claude','codex'].includes(value))delete k[selectedFace].usageView;
    if(value==='claude'&&k[selectedFace].usageView==='tokens')k[selectedFace].usageView='rings';
    previewFace=selectedFace;
  }
  else if (id === 'action-kind') {
    k.action=value;
    coolerSelectDraft.delete(k);
    k.value=['page','folder'].includes(value)?'0':value==='media'?'Play / pause':value==='cooler'?JSON.stringify({op:'theme_next'}):'';
  }
  else return;
  persist(); render({preserveFocus:true});
});
pane.addEventListener('input', e => {
  const {id,value} = e.target;
  markEditGesture(e.target);
  if(id==='cooler-number'){remember(`${draft.profile}/${draft.page}/${selected}/cooler-number`);updateCoolerAction(e.target);return;}
  if(id==='action-search'){
    actionSearch=value;
    const choices=Object.fromEntries(Object.entries(ACTIONS).filter(([kind,label])=>kind===activeKey().action||label.toLowerCase().includes(value.toLowerCase())));
    document.getElementById('action-kind').innerHTML=options(choices,activeKey().action);
    return;
  }
  if(!['face-text','action-value','launch-args','profile-name','page-name'].includes(id))return;
  remember(`${draft.profile}/${draft.page}/${selected}/${selectedFace}/${id}`);
  if (id === 'face-text') {
    activeKey()[selectedFace].text=value; previewFace=selectedFace;
    // Native previews keep their decoded image until the edited PNG is ready.
    // Browser-only previews still update their HTML label immediately.
    if(!T){document.getElementById('dock-preview').innerHTML=dockMarkup();updatePictures();}
    const label=document.querySelector('.workspace-toolbar .hint');
    if(label)label.textContent=`Previewing Face ${previewFace.toUpperCase()}`;
    updateNativePreviews();
  }
  else if (id === 'action-value'||id==='launch-args') updateActionValue();
  else if (id === 'profile-name') { profile().name=value; refreshNames(); }
  else if (id === 'page-name') { page().name=value; refreshNames(); }
  else return;
  persist();
});
function updateActionValue(){
  const input=document.getElementById('action-value');
  if(!input)return;
  if(activeKey().action==='launch'){
    const args=(document.getElementById('launch-args')?.value||'').split('\n').filter(v=>v.length>0);
    activeKey().value=JSON.stringify({path:input.value,args});
  }else activeKey().value=input.value;
}
document.addEventListener('keydown',event=>{
  if((event.ctrlKey||event.metaKey)&&event.key.toLowerCase()==='z'&&!['INPUT','TEXTAREA'].includes(event.target.tagName)){
    event.preventDefault();document.querySelector('[data-do="undo"]')?.click();
  }
});
render();
if (T) {
  const agentListener=invoke('plugin:event|listen',{event:'uc:agent-event',target:{kind:'Any'},handler:T.transformCallback(e=>agentEditor.event(e.payload))});
  const quitListener=invoke('plugin:event|listen',{event:'uc:prepare-quit',target:{kind:'Any'},handler:T.transformCallback(async()=>{
    quitPreparing=true;updateEditorLock();
    try{await coolerPanel.prepareLeave();}catch(error){
      quitPreparing=false;
      updateEditorLock();
      previewNote=error.message;
      showStorage();
      await invoke('editor_quit_ready',{saved:false}).catch(()=>{});
      return;
    }
    await Promise.all([...pendingMutations]);
    await followPreferenceWrite;
    const saved=await writer.flush();
    try {
      await invoke('editor_quit_ready',{saved});
      if (saved) return; // Remain inert until native exit; no last-moment edits.
      if (!saved) {storageNote='Changes could not be saved. Unified Control has stayed open so you can retry.';showStorage();}
    } catch {storageNote='Work is still running. Unified Control has stayed open.';showStorage();}
    quitPreparing=false;updateEditorLock();
  })});
  Promise.all([agentListener,quitListener]).then(()=>invoke('editor_session_ready',{ready:true})).then(async()=>{
    exitGuardReady=true;guardError='';await loadNativeProfiles();render();
  }).catch(()=>{
    exitGuardReady=false;guardError='The editor exit guard is unavailable. Layout editing is disabled.';render();
  });
  window.addEventListener('beforeunload',()=>invoke('editor_session_ready',{ready:false}).catch(()=>{}));
}
if(T)invoke('plugin:event|listen',{event:'uc:cooler-event',target:{kind:'Any'},handler:T.transformCallback(e=>coolerPanel.event(e.payload))}).catch(()=>{});
const initialPreferenceEpoch=followPreferenceEpoch,initialDockTicket=++dockRequestTicket,initialIntent=editorIntentEpoch;
invoke('app_state').then(s => {
  state=s;
  if(s.cooler&&!coolerPanel.hasState)coolerPanel.acceptState(s.cooler);
  if(followPreferenceEpoch===initialPreferenceEpoch){
    followPhysicalPage=s.followPhysicalPage!==false;
    savedFollowPhysicalPage=followPhysicalPage;
    followPreferenceReady=true;
  }
  if(s.dock)applyDockStatus(s.dock,initialDockTicket,initialIntent);
  if (!hasLocalPage && Object.hasOwn(NAV,s.page)) draft.current=s.page;
  render();followDeliveredPage();
}).catch(()=>{});
setInterval(() => {
  if (draft.current === 'dock') document.querySelectorAll('.clock-value').forEach(el => {
    el.textContent=new Date().toLocaleTimeString('en-AU',{hour:'2-digit',minute:'2-digit',hour12:false});
  });
  pollDockState();
  if (T && profilesReady && draft.current==='dock' && page().keys.some(k=>['clock','date','cpu','gpu','ram','claude','codex','reset','trend'].includes(k[previewFace].kind))) updateNativePreviews();
},1000);
refreshNavigationSearch=installNavigationSearch(document.querySelector('.nav'))||(()=>{});
syncDeviceVisibility();
