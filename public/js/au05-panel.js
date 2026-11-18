// Ulanzi AU05 Vibe Key: the General switch, the Home summary and the device page.
// Settings stored on the key are sent as they are changed (no Save button). UC's own settings
// (the switch and a UC action per key) are saved in settings.json at once.
import {ICON as IC,actionIcon,SHORT,SECTIONS} from './au05-icons.js';
const esc=s=>String(s??'').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
const ICON='<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true"><rect x="3" y="7" width="18" height="10" rx="3"/><circle cx="7.5" cy="12" r="1.5"/><circle cx="12" cy="12" r="1.5"/><circle cx="16.5" cy="12" r="1.5"/><path d="M12 4v3"/></svg>';
export const AU05_ICON=ICON;
const STATES={
  // "Receiver connected": the USB receiver; the rest is the key itself (2026-11-17).
  disabled:'Disabled',starting:'Starting',waiting:'Receiver connected · waiting for the key',connected:'Receiver connected · Vibe Key awake',asleep:'Receiver connected · Vibe Key asleep',
  paused:'Paused',suspended:'Stopped',off:'Receiver connected · Vibe Key probably off',not_found:'Not plugged in',error:'Error',
};

const STANDBY=[[60,'1 minute'],[120,'2 minutes'],[300,'5 minutes'],[600,'10 minutes'],[900,'15 minutes'],[1800,'30 minutes'],[3600,'1 hour']];
const SLEEP={3600:'1 hour',7200:'2 hours',10800:'3 hours',14400:'4 hours',0:'Never'};
const errorText=e=>typeof e==='string'?e:e?.error||e?.message||'The Vibe Key request failed';
// The diagram's zones, in the order of the real key (Ulanzi's product image, docs.ulanzistudio.com):
// logo and microphone grille on top, a large round knob with a light ring, then K1 (microphone icon),
// K2 (tick) and K3 (cross) in one column.
const ZONES={knobTurn:'Knob turn',knobPress:'Knob press',k1:'K1 · voice',k2:'K2 · confirm',k3:'K3 · cancel'};
const EVENT_ZONE={'K1':'k1','K2':'k2','K3':'k3','knob press':'knobPress','knob right':'knobTurn','knob left':'knobTurn'};

// "updated 3 s ago" (the Claude sessions card).
const ago=ms=>{const s=Math.max(0,Math.round((Date.now()-ms)/1000));return s<60?`updated ${s} s ago`:`updated ${Math.round(s/60)} min ago`;};
const clip=(t,n=24)=>t.length>n?t.slice(0,n-1)+'…':t;
const minutes=s=>s%60===0?`${s/60} minute${s===60?'':'s'}`:`${s} seconds`;

export function au05Summary(state,{native=true,readError=false}={}){
  if(!native)return {status:'Preview',description:'Vibe Key controls are available in the Windows app.'};
  if(readError)return {status:'Status unavailable',description:'Vibe Key status could not be refreshed.'};
  if(!state||state.enabled!==true)return {status:'Disabled',description:'Vibe Key is disabled.'};
  const battery=state.battery?` · battery ${state.battery.percent} %`:'';
  const label=STATES[state.state]||'Status unknown';
  return {status:state.battery?`${label} · ${state.battery.percent} %`:label,description:`${state.message||''}${battery}`};
}

export class Au05Panel{
  constructor(invoke,native,onState=()=>{}){
    this.invoke=invoke;this.native=native;this.onState=onState;
    this.state=native?null:{enabled:false,state:'disabled',message:'Vibe Key is disabled'};
    this.readError=false;this.container=null;this.settingsContainer=null;
    this.note='';this.noteAttention=false;this.sending=0;this.polling=false;this.epoch=0;
    this.selected='k1';this.lastFlashAt=null;this.lastChange={};
    // View-only state: which mode tab a panel shows, the menu card picker, profile renaming.
    this.modeView={};this.turnView=null;this.menuPick=null;this.renaming=false;this.dragPath=null;
    // Which layer the page edits ("live": with Unified Control, "offline": on its own) and offline tab views.
    this.layerView='live';this.offlineView={};
    // The palette: open or folded (remembered on this PC), the card being dragged, the last assignment to undo.
    this.paletteOpen=true;try{this.paletteOpen=localStorage.getItem('uc.au05.palette')!=='closed';}catch{}
    this.dragAction=null;this.undo=null;this.toastTimer=null;
    this.timer=setInterval(()=>this.refresh(),1000);
  }
  get enabled(){return this.state?.enabled===true;}
  get known(){return typeof this.state?.enabled==='boolean';}
  summary(){return au05Summary(this.state,{native:this.native,readError:this.readError});}
  acceptState(state){
    if(!state)return;
    const changed=JSON.stringify(this.state)!==JSON.stringify(state);
    this.state=state;this.readError=false;
    if(changed){this.paint();this.onState();}
  }
  async refresh(){
    if(!this.native||this.polling)return;
    this.polling=true;const epoch=this.epoch;
    try{const s=await this.invoke('au05_state');if(epoch===this.epoch)this.acceptState(s);}
    catch{if(epoch===this.epoch&&!this.readError){this.readError=true;this.paint();this.onState();}}
    finally{this.polling=false;}
  }
  mountSettings(container){
    this.settingsContainer=container;if(!container)return;
    container.innerHTML=`<div class="device-setting"><span class="device-icon">${ICON}</span><div class="device-setting-copy"><strong>Vibe Key</strong><p>Ulanzi AU05 · three keys, a knob and a microphone</p><p id="au05-message" class="setting-status"></p></div><span id="au05-state" class="chip"></span><label class="device-setting-control"><input id="au05-enabled" type="checkbox" role="switch" aria-label="Enable Vibe Key" ${this.native?'':'disabled'}></label></div>`;
    container.querySelector('#au05-enabled').addEventListener('change',e=>this.configure({enabled:e.target.checked}));
    this.paintSettings();
  }
  paintSettings(){
    const c=this.settingsContainer;if(!c||!c.isConnected)return;
    const s=this.summary();
    c.querySelector('#au05-message').textContent=s.description;
    c.querySelector('#au05-state').textContent=s.status;
    const sw=c.querySelector('#au05-enabled');if(sw&&!this.configuring)sw.checked=this.enabled;
  }
  async configure(patch){
    this.configuring=true;this.epoch++;
    try{this.state=await this.invoke('au05_configure',{patch});this.setNote('');}
    catch(error){this.setNote(errorText(error),true);}
    finally{this.configuring=false;this.paint();this.onState();}
  }
  markup(){return '<div id="au05-page" aria-label="Vibe Key workspace"></div>';}
  mount(container){
    this.container=container;
    // Enter or Space on a diagram zone selects it, like a click.
    if(!container.dataset.au05Keys){
      container.dataset.au05Keys='1';
      container.addEventListener('keydown',e=>{
        const z=e.target.closest?.('[data-au05-zone]');
        if(z&&(e.key==='Enter'||e.key===' ')){e.preventDefault();this.select(z.dataset.au05Zone);}
        if(e.key==='Escape'&&this.menuPick){this.menuPick=null;this.paint(true);}
        if(e.key==='Enter'&&e.target.matches?.('[data-au05-profile-name]'))e.target.blur();
      });
      container.addEventListener('focusout',e=>{if(e.target.matches?.('[data-au05-profile-name]')&&this.renaming){this.renaming=false;setTimeout(()=>this.paint(true),0);}});
      const parent=p=>String(p).split('.').slice(0,-1).join('.');
      const rowOf=e=>e.target.closest?.('[data-qm-row]');
      // Palette cards are dragged with pointer events (mouse or pen; touch taps to assign), not HTML drag and drop: a copy of
      // the card follows the pointer, the part under it lights up when it accepts the card, and the drop
      // assigns it. Escape cancels.
      const valid=(zone,v)=>zone==='knobTurn'?v.startsWith('knob:'):['k1','k2','k3','knobPress'].includes(zone)&&v.startsWith('uc:');
      const menuOk=v=>/^uc:/.test(v)&&!['uc:quickMenu','uc:hotkey','uc:none'].includes(v);
      const clear=()=>{for(const x of container.querySelectorAll('.drop-hover,.drop-target'))x.classList.remove('drop-hover','drop-target');};
      const targetAt=(x,y,v)=>{
        const el=document.elementFromPoint(x,y);if(!el||!container.contains(el))return null;
        const z=el.closest('[data-au05-zone]');if(z)return {zone:z.dataset.au05Zone,ok:valid(z.dataset.au05Zone,v)};
        const r=el.closest('[data-qm-row],[data-qm-op="add-top"]');if(r)return {row:r,ok:menuOk(v)};
        return null;
      };
      let pd=null;
      const end=()=>{if(pd?.ghost)pd.ghost.remove();pd=null;clear();document.body.classList.remove('au05-dragging');delete document.body.dataset.au05Drop;
        this.container?.querySelector('#au05-page')?.classList.remove('drag-key','drag-knob');
        if(this.dragAction!=null){this.dragAction=null;this.paint();}};
      container.addEventListener('pointerdown',e=>{const c=e.target.closest?.('[data-au05-palette]');if(this.layerView==='offline')return;
        if(!c||e.button!==0||c.disabled||!this.native||e.pointerType==='touch')return;pd={value:c.dataset.value,x:e.clientX,y:e.clientY,card:c,started:false};});
      document.addEventListener('pointermove',e=>{
        if(!pd)return;
        if(!pd.started){if(Math.hypot(e.clientX-pd.x,e.clientY-pd.y)<6)return;
          pd.started=true;this.dragAction=pd.value;
          const g=pd.card.cloneNode(true);g.classList.add('au05-ghost');g.removeAttribute('data-au05-palette');g.style.width=`${pd.card.offsetWidth}px`;document.body.appendChild(g);pd.ghost=g;
          document.body.classList.add('au05-dragging');
          this.container?.querySelector('#au05-page')?.classList.add(pd.value.startsWith('knob:')?'drag-knob':'drag-key');}
        e.preventDefault();
        pd.ghost.style.transform=`translate(${e.clientX+10}px,${e.clientY+10}px)`;
        pd.ghost.hidden=true;const t=targetAt(e.clientX,e.clientY,pd.value);pd.ghost.hidden=false;
        clear();document.body.dataset.au05Drop=t?(t.ok?'ok':'no'):'';
        if(t?.ok&&t.zone)for(const el of container.querySelectorAll(`.au05-diagram [data-au05-zone="${t.zone}"]`))el.classList.add('drop-hover');
        if(t?.ok&&t.row)t.row.classList.add('drop-target');
        pd.target=t;});
      document.addEventListener('pointerup',e=>{
        if(!pd)return;const {started,value,target}=pd;end();if(!started)return;
        this.justDragged=true;setTimeout(()=>{this.justDragged=false;},0);
        if(!target?.ok)return;
        if(target.zone){this.assign(target.zone,value);return;}
        const what=`a:${value.slice(3)}`,r=target.row;
        if(r.dataset.qmRow!=null){const path=r.dataset.qmRow;this.editMenu(en=>{const [l,i]=Au05Panel.locate(en,path);if(l[i])l[i]=this.entryFor(what,l[i]);});}
        else this.editMenu(en=>{en.push(this.entryFor(what));});});
      document.addEventListener('pointercancel',()=>end());
      document.addEventListener('keydown',e=>{if(e.key==='Escape'&&pd?.started)end();});
      container.addEventListener('dragstart',e=>{const g=e.target.closest?.('[data-qm-drag]');if(!g)return;this.dragPath=g.dataset.qmDrag;e.dataTransfer.effectAllowed='move';e.dataTransfer.setData('text/plain',this.dragPath);rowOf(e)?.classList.add('is-dragging');});
      container.addEventListener('dragover',e=>{if(this.dragAction!=null)return;const r=rowOf(e);if(!r||this.dragPath==null||parent(r.dataset.qmRow)!==parent(this.dragPath))return;e.preventDefault();
        for(const x of container.querySelectorAll('.au05-menu-row.drop-target'))if(x!==r)x.classList.remove('drop-target');r.classList.add('drop-target');});
      container.addEventListener('drop',e=>{if(this.dragAction!=null)return;const r=rowOf(e);const from=this.dragPath;this.dragPath=null;if(!r||from==null||parent(r.dataset.qmRow)!==parent(from))return;e.preventDefault();
        const to=Number(r.dataset.qmRow.split('.').pop());this.editMenu(en=>{const [l,i]=Au05Panel.locate(en,from);if(i===to||!l[i])return;const [x]=l.splice(i,1);l.splice(to,0,x);});});
      container.addEventListener('dragend',()=>{this.dragPath=null;for(const x of container.querySelectorAll('.is-dragging,.drop-target'))x.classList.remove('is-dragging','drop-target');});
    }
    this.paint(true);this.refresh();this.claudeRefresh();
  }
  async claudeRefresh(){
    if(!this.native)return;
    try{this.claudeFull=await this.invoke('claude_sessions_state');this.paint(true);}catch{}
  }
  async claudeOp(op){
    try{const r=await this.invoke(op==='connect'?'claude_sessions_connect':'claude_sessions_disconnect');this.claudeFull=r.claude;
      if(r.warning)this.setNote(r.warning,true);else this.setNote(op==='connect'?(r.backup?`Connected. Your Claude settings were backed up to ${r.backup} first.`:'Connected.'):'Disconnected. Unified Control removed only its own hook entries.');}
    catch(error){this.setNote(errorText(error),true);}
    await this.refresh();this.paint(true);
  }
  select(zone){
    if(!Object.hasOwn(ZONES,zone))return;
    this.selected=zone;this.modeView={};this.turnView=null;this.offlineView={};this.paint(true);
    this.container?.querySelector(`.au05-diagram [data-au05-zone="${zone}"][tabindex]`)?.focus();
  }
  // A short flash on the diagram for each new key or knob message, and for each action UC ran on a hotkey.
  flash(){
    const ev=this.state?.events||[];
    const newest=ev.length?ev[0].at:0;
    if(this.lastFlashAt===null){this.lastFlashAt=newest;return;}
    for(const e of ev){
      if(e.at<=this.lastFlashAt)break;
      if(!e.pressed&&!e.action)continue;
      const zone=EVENT_ZONE[e.control];
      // An action that did not run (for example "Dictation didn't start") flashes red and says why.
      const bad=typeof e.action==='string'&&!e.action.startsWith('ran ');
      const cls=bad?'au05-flash-bad':'au05-flash';
      for(const el of this.container?.querySelectorAll(`.au05-diagram [data-au05-zone="${zone}"]`)||[]){
        el.classList.remove(cls);void el.getBoundingClientRect();el.classList.add(cls);
        setTimeout(()=>el.classList.remove(cls),bad?1600:700);
      }
      if(bad)this.toast(`${ZONES[zone]||e.control}: ${e.action.replace(/^[^:]+: [^:]+: /,'').replace(/\s*\(.*\)$/,'')}`,null);
    }
    this.lastFlashAt=Math.max(this.lastFlashAt,newest);
  }
  unmount(){this.container=null;}
  active(container){return this.container===container&&!!container.querySelector('#au05-page');}
  setNote(text,attention=false){this.note=text;this.noteAttention=attention;for(const el of this.container?.querySelectorAll('#au05-note,.au05-panel-note')||[]){el.textContent=text;el.classList.toggle('attention',attention);}}
  paint(force=false){
    this.paintSettings();
    const root=this.container?.querySelector('#au05-page');if(!root)return;
    const s=this.state||{};
    if(!root.firstChild)root.innerHTML=this.skeleton();
    // How long ago Claude's session list was read (updated on every paint, without re-rendering the page).
    const age=root.querySelector('#au05-cs-age'),readAt=s.menu?.sessions?.readAt;
    if(age&&readAt)age.textContent=ago(readAt);
    
    root.querySelector('#au05-events').innerHTML=this.eventsMarkup();
    const controls=root.querySelector('#au05-controls');
    const menuSig=s.menu?{...s.menu,view:undefined,sessions:s.menu.sessions?{...s.menu.sessions,readAt:undefined}:null}:null;
    // Recent events: the card shows only when there is something in it.
    const evBox=root.querySelector('.au05-events-box');if(evBox)evBox.hidden=!(s.events||[]).length;
    const sig=JSON.stringify([s.changes,this.advancedOpen,s.battery,s.state,s.claude,this.claudeFull,s.turns,s.keys,s.standbyS,s.sleepS,s.light,s.micNr,s.hooksMode,s.standbyStatus,s.controlCodes,s.actions,s.fake,s.enabled,this.sending>0,this.selected,menuSig,s.menuHotkeys,s.layer,s.offlinePending,s.keySync,this.layerView,this.offlineView,this.modeView,this.turnView,this.menuPick,this.renaming,s.catalogue,this.paletteOpen]);
    // Never re-render while text is typed or a list is in use (1.0: a re-render replaced an open list, so the
    // choice was lost: noise reduction levels 0, 2 and 3 never reached the key). The page catches up when the
    // list loses focus.
    const active=document.activeElement,typing=controls.contains(active)&&active.matches('input:not([type="range"]):not([type="checkbox"]),select');
    if(typing&&active.matches('select')&&!active.dataset.au05Wait){active.dataset.au05Wait='1';active.addEventListener('blur',()=>this.paint(),{once:true});}
    if(this.dragAction!=null&&!force)return this.flash();
    if(force||(sig!==this.lastSig&&!typing)){
      const refocus=controls.contains(active)&&active.id?active.id:null;
      this.lastSig=sig;controls.innerHTML=this.controlsMarkup();
      if(refocus)controls.querySelector(`#${CSS.escape(refocus)}`)?.focus();
    }
    this.flash();
  }
  skeleton(){
    return `<div class="page-heading au05-heading"><div><span class="eyebrow">ULANZI AU05</span><h1>Vibe Key</h1></div></div>
    <p id="au05-note" class="au05-note${this.noteAttention?' attention':''}" role="status">${esc(this.note)}</p>
    <div id="au05-controls"></div>
    <div id="au05-toast" class="au05-toast" role="status" hidden><span class="au05-toast-text"></span><button type="button" class="au05-link" data-au05-undo>Undo</button></div>
    <details class="au05-box au05-events-box"><summary><span class="au05-box-title"><span class="au05-box-icon">${IC.keyboard}</span>Recent events</span></summary><ul id="au05-events" class="au05-events" aria-live="polite"></ul></details>`;
  }
  eventsMarkup(){
    const ev=this.state?.events||[];
    if(!ev.length)return '<li class="au05-muted">No events yet.</li>';
    return ev.slice(0,20).map(e=>`<li><span>${esc(new Date(e.at).toLocaleTimeString('en-AU',{hour12:false}))}</span> <strong>${esc(e.control)}</strong> ${e.pressed?'down':'up'} <span class="hint">${esc(e.code)}</span>${e.action?` <span class="chip">${esc(e.action)}</span>`:''}</li>`).join('');
  }
  // Every setting can be sent; the ones not yet checked on the real key carry an "Untested" label.
  /** The menus K2 cycles through (1.0): name, style (dial or list), what it shows; add, move, remove. */
  facesMarkup(faces,native){
    const o=(v,l,sel)=>`<option value="${esc(v)}"${v===sel?' selected':''}>${esc(l)}</option>`;
    const rows=faces.map((f,i)=>`<li class="au05-face-row"><input type="text" class="au05-face-name" data-qm-face="${i}:name" value="${esc(f.name)}" aria-label="Menu name"${native}>
      <select data-qm-face="${i}:style" aria-label="Style"${native}>${o('dial','Dial',f.style)}${o('list','List',f.style)}</select>
      <select data-qm-face="${i}:source" aria-label="Shows"${native}>${o('menu','This profile’s menu',f.source)}${o('recent','Recent Claude sessions',f.source)}</select>
      <span class="au05-face-tools"><button class="btn btn-icon" type="button" data-qm-face-op="up:${i}" title="Move up"${i===0?' disabled':native}>↑</button><button class="btn btn-icon" type="button" data-qm-face-op="down:${i}" title="Move down"${i===faces.length-1?' disabled':native}>↓</button><button class="btn btn-icon" type="button" data-qm-face-op="remove:${i}" title="Remove"${faces.length<2?' disabled':native}>✕</button></span></li>`).join('');
    return `<div class="au05-faces"><span class="au05-inmenu-title" title="With the menu open, the control set to &quot;Next menu&quot; (K2) shows the next one; after the last, the first again">Menus</span><ul class="au05-face-list">${rows}</ul>
      <button class="btn" type="button" data-qm-face-op="add"${faces.length>=6?' disabled':native}>${IC.plus} Add menu</button></div>`;
  }
  untested(name){return this.state?.checked?.[name]===true?'':' <span class="chip attention au05-untested">Untested</span>';}
  controlsMarkup(){
    const s=this.state||{};const off=!this.native||s.enabled!==true||this.sending>0;
    const dis=off?' disabled':'';
    const native=this.native?'':' disabled';
    const live=this.native&&s.enabled===true?'':' disabled';
    const opt=(v,label,sel)=>`<option value="${esc(v)}"${String(v)===String(sel)?' selected':''}>${esc(label)}</option>`;
    const raw=v=>v==null?'':`<span class="au05-raw">${esc(v)}</span>`;
    const cat=s.catalogue?.actions||[];
    const m=s.menu||{};
    const keyOf=id=>(s.keys||[]).find(k=>k.key===id);
    const shortName=e=>SHORT[e.id]||e.label.replace(/\s*\(.*\)$/,'');
    const compactKeys=k=>String(k||'').replace(/\s*\+\s*/g,'+');
    const actionName=a=>{
      if(!a||a.kind==='none')return 'Nothing';
      if(a.kind==='quickMenu')return 'Quick menu';
      if(a.kind==='hotkey'){const app=(s.catalogue?.apps||[]).find(x=>x.id===a.app);return a.value?`${a.value}${app?` in ${app.name}`:''}`:'Custom (not set)';}
      const e=cat.find(x=>x.id===a.kind);return e?`${e.group==='Windows / media'?'':`${e.group} `}${e.label}`:a.kind;
    };
    const mark=sec=>`<span class="au05-mark" aria-hidden="true" style="--hue:${sec.hue}">${sec.mark?esc(sec.mark):IC[sec.glyph]}</span>`;
    // One action card. `attrs` carries the data attributes of the picker it belongs to.
    const card=(attrs,value,icon,name,selected,{keys='',unverified=false,title=''}={})=>
      `<button type="button" class="au05-card${selected?' is-selected':''}" ${attrs} data-value="${esc(value)}" aria-pressed="${selected}" title="${esc(title||name)}"${native}>
        <span class="au05-card-icon">${icon}</span><span class="au05-card-name">${esc(name)}</span>
        ${keys?`<span class="au05-kbd">${esc(keys)}</span>`:''}${unverified?'<span class="au05-unv" aria-label="unverified">?</span>':''}</button>`;
    // The catalogue as card sections (Claude, ChatGPT, Target app, Windows & media), plus extra sections.
    const catalogueSections=(attrs,prefix,selectedValue,extra='')=>`<div class="au05-sections">${SECTIONS.filter(sec=>cat.some(e=>e.group===sec.group)).map(sec=>
      `<section class="au05-section"><h3 class="au05-sec">${mark(sec)}<span class="au05-sec-title">${esc(sec.title)}</span></h3><div class="au05-grid">${cat.filter(e=>e.group===sec.group).map(e=>
        card(attrs,`${prefix}${e.id}`,actionIcon(e.id),shortName(e),selectedValue===`${prefix}${e.id}`,{
          keys:e.keys&&e.keys!==e.label&&e.keys!==shortName(e)&&e.keys!=='Follows the target app'&&!/^(Play|Next|Previous|Volume|Mute)/.test(e.keys)?e.keys:'',
          unverified:e.status!=='verified',title:`${e.group}: ${e.label}${e.keys&&e.keys!==e.label?` · ${e.keys}`:''}${e.status==='verified'?'':' · unverified'} · ${e.note}`})).join('')}</div></section>`).join('')}${extra}</div>`;
    const pill=(text,kind='',cls='')=>`<span class="au05-pill ${kind} ${cls}">${esc(text)}</span>`;

    // ---- two layers: "With Unified Control" (live) and "On its own" (offline, stored on the key) ----
    const L=s.layer||{target:'live',offline:{}};
    const offlineView=this.layerView==='offline';
    const shortLabel=t=>String(t||'').replace(/^Factory default \((.*)\)$/,'$1 (factory)');
    const onKey=k=>k.mode==='unknown'?'not read':shortLabel(k.label);
    const readLink=k=>`<button class="au05-link" type="button" data-au05-read="${k.key}"${dis}>${k.mode==='unknown'?'Read from key':'Read again'}</button>`;
    const jobPill=(k)=>{const job=k.job;if(!job)return '';
      if(job.state==='check')return `<span class="au05-pill warn au05-job" title="${esc(job.message)}">Needs a check <button class="au05-link" type="button" data-au05-retry="${k.key}">Set up</button></span>`;
      if(job.read&&job.state!=='failed')return pill(job.state==='sending'?'Reading…':'Waiting to read','warn','au05-job');
      return job.state==='done'?pill('Key updated','ok','au05-job'):job.state==='failed'?`<span class="au05-pill bad au05-job" title="${esc(job.message)}">Failed <button class="au05-link" type="button" data-au05-retry="${k.key}">Try again</button></span>`:pill(job.state==='sending'?'Sending…':'Waiting for key','warn','au05-job');};
    // ---- the side panel for one key: mode, status and the current action (the palette assigns) ----
    const current=(icon,name,sub)=>`<div class="au05-current"><span class="au05-card-icon">${icon}</span><div><strong>${esc(name)}</strong>${sub?`<span class="au05-muted">${esc(sub)}</span>`:''}</div></div>`;
    const keyValue=k=>{const a=s.actions?.[k.key]||{kind:'offline'};return a.kind==='offline'?'offline':`uc:${a.kind}`;};
    const keyEditor=k=>{
      const a=s.actions?.[k.key]||{kind:'offline',value:'',app:''};
      const value=keyValue(k);
      const view=this.modeView[k.key]||(value==='offline'?'offline':'uc');
      const seg=(id,label)=>`<button type="button" data-au05-segment="${id}" aria-pressed="${view===id}"${native}>${esc(label)}</button>`;
      const pills=[
        k.checked?'':`<span class="chip attention au05-untested">Untested</span>`,
        value.startsWith('uc:')&&a.kind!=='none'&&k.hotkey?.state==='ready'?pill(`Ready · ${k.ownCode}`,'ok'):'',
        value.startsWith('uc:')&&a.kind!=='none'&&k.hotkey?.state==='failed'?pill(k.hotkey.message||'Not available','bad'):'',
        jobPill(k),
      ].join('');
      const custom=value==='uc:hotkey'?`<div class="au05-inline"><input data-au05-action-value="${k.key}" maxlength="60" placeholder="Ctrl + Enter" aria-label="Key combination for ${esc(k.name)}" value="${esc(a.value)}">
        <select data-au05-action-app="${k.key}" aria-label="Send to">${opt('','To the focused app',a.app||'')}${(s.catalogue?.apps||[]).map(x=>opt(x.id,`To ${x.name}${x.verified?'':' (unverified)'}`,a.app||'')).join('')}</select></div>`:'';
      const press=keyOf('knobPress');
      const menuLine=a.kind==='quickMenu'&&press&&!press.uc&&k.key!=='knobPress'?'<p class="au05-line attention">Give the knob press a Unified Control action (Nothing is fine) to choose with it.</p>':'';
      const entry=cat.find(e=>e.id===a.kind);
      // A plain keystroke can also be what the key types on its own.
      const alsoKeys=view==='uc'?(a.kind==='hotkey'?(a.value||'').trim():entry?.offlineKeys||''):'';
      const off=L.offline?.[k.key]||{};
      const also=alsoKeys?(!off.factory&&off.keys===alsoKeys?`<p class="au05-line">On its own it types ${esc(alsoKeys)} too.</p>`
        :`<button class="btn au05-also" type="button" data-au05-also="${esc(alsoKeys)}" data-control="${k.key}"${native}>Also use on its own (${esc(alsoKeys)})</button>`):'';
      const summary=view==='offline'?current(IC.keyboard,shortLabel(k.offline),'What it types on its own (change it under "On its own")')
        :!value.startsWith('uc:')?current(IC.dot,'No action yet','Pick one in the palette')
        :a.kind==='none'?current(actionIcon('none'),'Nothing','')
        :a.kind==='quickMenu'?current(actionIcon('quickMenu'),'Quick menu','Unified Control')
        :a.kind==='hotkey'?current(actionIcon('hotkey'),'Custom shortcut',a.value||'Type the keys')
        :entry?current(actionIcon(entry.id),shortName(entry),`${entry.group}${entry.keys&&entry.keys!==entry.label?` · ${entry.keys}`:''}`):current(IC.dot,a.kind,'');
      return `<div class="au05-panel-head"><div><h2>${esc(ZONES[k.key])}</h2>
          <span class="au05-types">On the key now: ${esc(onKey(k))} ${readLink(k)}</span></div>
        <div class="au05-pills">${pills}</div></div>
        <div class="au05-seg au05-mode" data-au05-mode="${k.key}" data-value="${esc(value)}" role="group" aria-label="What ${esc(k.name)} does with Unified Control">${seg('uc','UC action')}${seg('offline','Same as on its own')}</div>
        
        ${summary}${view==='uc'?custom+menuLine:''}${also}`;
    };
    // ---- the offline layer for one control (what the key types without Unified Control) ----
    const offlineRow=(name,label,factory,turn)=>{
      const o=L.offline?.[name]||{factory:true};
      const view=this.offlineView[name]||(o.factory?'factory':'keys');
      const seg=(id,text)=>`<button type="button" data-au05-offline-mode="${id}" data-control="${name}" aria-pressed="${view===id}"${native}>${esc(text)}</button>`;
      return `<div class="au05-offline-row">${label?`<div class="au05-set-label">${esc(label)}</div>`:''}
        <div class="au05-seg" role="group" aria-label="${esc(label||ZONES[name]||name)} on its own">${seg('keys','Types keys')}${seg('factory',turn?'Factory scroll':`Factory (${factory})`)}</div>
        ${view==='keys'?`<div class="au05-inline"><input data-au05-offline-keys="${name}" maxlength="60" placeholder="Ctrl + D" aria-label="Keys ${esc(label||name)} types on its own" value="${esc(o.keys||'')}"${native}></div>`:''}</div>`;
    };
    const offlineEditor=()=>{
      if(this.selected==='knobTurn'){
        const turns=s.turns||[];const t=n=>turns.find(x=>x.key===n)||{key:n,mode:'unknown'};
        return `<div class="au05-panel-head"><div><h2>Knob turn · on its own</h2><span class="au05-types">On the key now: right ${esc(t('knobRight').mode==='unknown'?'not read':shortLabel(t('knobRight').label))}, left ${esc(t('knobLeft').mode==='unknown'?'not read':shortLabel(t('knobLeft').label))}</span></div></div>
          ${offlineRow('knobRight','Right','',true)}${offlineRow('knobLeft','Left','',true)}`;
      }
      const k=keyOf(this.selected);if(!k)return '<p class="au05-line">Wake the key (press any key) to read its controls.</p>';
      // Actions the key can store as plain keys (they go to the app in front); the others need Unified Control.
      const o=L.offline?.[k.key]||{};
      const offCard=e=>{const keys=e.offlineKeys;const on=keys&&!o.factory&&o.keys===keys;
        return keys?`<button type="button" class="au05-card au05-ocard${on?' is-selected':''}" data-au05-offline-pick="${esc(keys)}" data-control="${k.key}" aria-pressed="${on}" title="${esc(`${e.group}: ${e.label} · types ${keys} in the app in front`)}"${native}><span class="au05-card-icon">${actionIcon(e.id)}</span><span class="au05-card-name">${esc(shortName(e))}</span><span class="au05-kbd">${esc(keys)}</span></button>`
          :`<span class="au05-card au05-ocard is-dim" aria-disabled="true" title="${esc(`${e.group}: ${e.label} · needs Unified Control`)}"><span class="au05-card-icon">${actionIcon(e.id)}</span><span class="au05-card-name">${esc(shortName(e))}</span><span class="au05-needs-uc">Needs Unified Control</span></span>`;};
      const needUc=cat.filter(e=>!e.offlineKeys);
      const offActions=`<div class="au05-set-label">Or take an action's keys</div><div class="au05-grid au05-offline-actions">${cat.filter(e=>e.offlineKeys).map(offCard).join('')}</div>
        <details class="au05-needs-uc-list"><summary>Needs Unified Control (${needUc.length+1})</summary><div class="au05-grid au05-offline-actions">${needUc.map(offCard).join('')}
        <span class="au05-card au05-ocard is-dim" aria-disabled="true" title="Quick menu, profiles and Claude sessions need Unified Control"><span class="au05-card-icon">${actionIcon('quickMenu')}</span><span class="au05-card-name">Quick menu</span><span class="au05-needs-uc">Needs Unified Control</span></span></div></details>`;
      return `<div class="au05-panel-head"><div><h2>${esc(ZONES[k.key])} · on its own</h2><span class="au05-types">On the key now: ${esc(onKey(k))} ${readLink(k)}</span></div><div class="au05-pills">${jobPill(k)}</div></div>
        ${offlineRow(k.key,'',k.factory,false)}${offActions}`;
    };

    // ---- the knob turn panel ----
    const anyMenu=Object.values(s.actions||{}).some(a=>a?.kind==='quickMenu');
    const turnAct=m.knobTurnAction||'scroll';
    const TURN={scroll:['scroll','Scroll','One notch per click, under the mouse'],volume:['volume','Volume','Volume up and down'],none:['none','Nothing','Only moves the quick menu']};
    const knobEditor=()=>{
      const view=this.turnView||(m.knobTurn==='uc'?'uc':'offline');
      const seg=(id,label)=>`<button type="button" data-au05-turn-mode="${id}" aria-pressed="${view===id}"${native}>${esc(label)}</button>`;
      const dir=t=>`<div class="au05-dir"><span>${t.key==='knobRight'?'Right':'Left'} · on the key now: ${esc(t.mode==='unknown'?'not read':t.mode==='uc'?t.ownCode:shortLabel(t.label))}</span>
        ${t.checked?'':'<span class="chip attention au05-untested">Untested</span>'}${jobPill(t)}
        <button class="au05-link" type="button" data-au05-read="${t.key}"${dis}>${t.mode==='unknown'?'Read':'Read again'}</button></div>`;
      const [ic,name,sub]=TURN[turnAct]||TURN.scroll;
      return `<div class="au05-panel-head"><div><h2>Knob turn</h2><span class="au05-types">${m.knobTurn==='uc'?'Types F23 / F24 with Unified Control':'Same as on its own'}${anyMenu?' · moves the quick menu':''}</span></div></div>
        <div class="au05-seg" data-au05-turn-seg role="group" aria-label="What turning the knob does with Unified Control">${seg('uc','UC action')}${seg('offline','Same as on its own')}</div>
        
        ${m.knobTurn==='uc'?current(actionIcon(ic),name,sub):current(IC.scroll,'Same as on its own','')}
        <div class="au05-dirs">${(s.turns||[]).map(dir).join('')}</div>`;
    };

    // ---- the drawing ----
    const assignment=id=>{
      if(offlineView){
        if(id==='knobTurn'){const r=L.offline?.knobRight,l=L.offline?.knobLeft;return r?.factory&&l?.factory?'Factory scroll':`R ${r?.factory?'scroll':r?.keys} · L ${l?.factory?'scroll':l?.keys}`;}
        const o=L.offline?.[id];return o?(o.factory?`Factory: ${keyOf(id)?.factory||''}`:`Types ${o.keys}`):'';
      }
      if(id==='knobTurn'){const act={scroll:'UC scroll',volume:'Volume',none:'Nothing'}[turnAct]||'UC scroll';return m.knobTurn==='uc'?`${act}${anyMenu?' · menu':''}`:`Factory scroll${anyMenu?' · menu':''}`;}
      const k=keyOf(id),a=s.actions?.[id];
      const waiting=k?.job&&(k.job.state==='waiting'||k.job.state==='sending')?' (waiting)':'';
      if(!a||a.kind==='offline'){const o=L.offline?.[id];return (o?(o.factory?`Factory: ${k?.factory||''}`:`Types ${o.keys}`):'Same as on its own')+waiting;}
      return actionName(a)+waiting;
    };
    const sel=id=>id===this.selected?' is-selected':'';
    const zone=(id,shape)=>shape.replace('<ZONE ','<').replace('/>',` data-au05-zone="${id}" class="au05-zone${sel(id)}${id==='knobTurn'?' au05-ring':''}" role="button" tabindex="0" aria-label="${ZONES[id]}: ${esc(assignment(id))}" aria-pressed="${id===this.selected}"/>`);
    // A control waiting for "Set up" (not checked on the real key) is marked on the drawing.
    const needsCheck=id=>{const ks=id==='knobTurn'?[...(s.turns||[])]:[...(s.keys||[])].filter(k=>k.key===id);return ks.some(k=>k.job?.state==='check');};
    const callout=(id,y,anchorX,anchorY)=>`<g class="au05-callout${sel(id)}" data-au05-zone="${id}"><path d="M${anchorX} ${anchorY}H164V${y+17}H176" class="au05-leader"/><rect x="176" y="${y}" width="196" height="34" rx="8"/><text x="186" y="${y+14}" class="au05-name">${esc(ZONES[id])}</text><text x="186" y="${y+28}" class="au05-assign">${esc(clip(assignment(id)))}</text>${needsCheck(id)?`<g class="au05-needs-check"><title>Needs a check: choose it and press Set up</title><circle cx="360" cy="${y+10}" r="6"/><text x="360" y="${y+13.5}" text-anchor="middle">!</text></g>`:''}</g>`;
    const diagram=`<svg class="au05-diagram" viewBox="0 0 380 500" role="group" aria-label="Vibe Key layout. Choose a control.">
      <rect x="20" y="8" width="130" height="484" rx="22" class="au05-body"/>
      <text x="85" y="34" text-anchor="middle" class="au05-logo">Ulanzi</text>
      ${[0,1,2,3,4,5].map(i=>`<rect x="${54+i*11}" y="46" width="5" height="22" rx="2.5" class="au05-grille"/>`).join('')}
      <circle cx="85" cy="140" r="56" class="au05-light"/>
      ${zone('knobTurn','<ZONE circle cx="85" cy="140" r="46"/>')}
      ${zone('knobPress','<ZONE circle cx="85" cy="140" r="33"/>')}
      <path d="M62 117l4 4M108 117l-4 4M62 163l4-4M108 163l-4-4" class="au05-mark-line"/>
      ${[['k1',215],['k2',307],['k3',399]].map(([id,y])=>`${zone(id,`<ZONE rect x="37" y="${y}" width="96" height="84" rx="12"/>`)}<circle cx="85" cy="${y+42}" r="25" class="au05-cap"/>`).join('')}
      <rect x="80" y="243" width="10" height="17" rx="5" class="au05-icon"/><path d="M76 255a9 9 0 0 0 18 0M85 264v5" class="au05-icon"/>
      <path d="M76 349l6 6 12-12" class="au05-icon"/>
      <path d="M78 434l14 14M92 434l-14 14" class="au05-icon"/>
      ${callout('knobTurn',78,131,110)}${callout('knobPress',126,118,140)}${callout('k1',240,133,257)}${callout('k2',332,133,349)}${callout('k3',424,133,441)}
    </svg>`;
    const editor=offlineView?offlineEditor():this.selected==='knobTurn'?knobEditor():keyOf(this.selected)?keyEditor(keyOf(this.selected)):'<p class="au05-line">Wake the key (press any key) to read its controls.</p>';
    // The layer switch, where the key's layer stands, and the one risk line.
    const layerBar=`<div class="au05-layerbar"><div class="au05-seg au05-layer-seg" role="group" aria-label="Which layer you edit">
        <button type="button" data-au05-layer-view="live" aria-pressed="${!offlineView}">With Unified Control</button><button type="button" data-au05-layer-view="offline" aria-pressed="${offlineView}" title="What each control types when Unified Control is not running (another PC, or after Quit). Stored on the key when Unified Control quits.">On its own</button></div>
      ${L.target==='offline'?`<span class="au05-pill ${L.offlineOnKey?'ok':'warn'}">${L.offlineOnKey?'Offline layer on the key':'Storing the offline layer…'}</span><button class="btn" type="button" data-au05-layer="live"${native}>Back to Unified Control</button>`
        :`<button class="btn" type="button" data-au05-layer="offline"${live}>Store offline layer on the key now</button>`}
      ${s.keySync?.blocked?'<span class="au05-pill bad" title="A key change got no answer. Nothing more is sent to the key until you press Reconnect or Try again">Key changes paused: use Reconnect</span>':''}
      ${s.keySync?.waiting?'<span class="au05-pill warn" title="The last run left a key change unanswered; the key gets 20 seconds to finish it">Waiting for the key to finish an earlier change</span>':''}
      ${s.offlinePending&&!(L.target==='offline'&&L.offlineOnKey)?'<span class="au05-pill warn" title="At the last quit the key was asleep or there was no time">Offline layer not stored yet</span>':''}</div>
`;

    // ---- the action palette: always on the page; drag a card onto the drawing, or click to set the selected part ----
    const sel0=this.selected;const selKey=keyOf(sel0);
    const selValue=sel0==='knobTurn'?(m.knobTurn==='uc'?`knob:${turnAct}`:''):selKey?keyValue(selKey):(s.actions?.[sel0]?`uc:${s.actions[sel0].kind}`:'');
    const forKnob=sel0==='knobTurn';
    const pcard=(value,icon,name,{keys='',unverified=false,title='',fresh=false}={})=>{
      const knob=value.startsWith('knob:');const dim=knob!==forKnob;
      return `<button type="button" class="au05-card au05-pcard${selValue===value?' is-selected':''}${dim?' is-dim':''}" data-au05-palette="" data-value="${esc(value)}" aria-pressed="${selValue===value}"${dim?' aria-disabled="true"':''} title="${esc(title||name)}"${native}>
        <span class="au05-card-icon">${icon}</span><span class="au05-card-name">${esc(name)}</span>${keys?`<span class="au05-kbd">${esc(keys)}</span>`:''}${fresh?'<span class="au05-new">New</span>':unverified?'<span class="au05-unv" aria-label="unverified">?</span>':''}</button>`;
    };
    const paletteOpen=this.paletteOpen;
    const paletteSections=`${SECTIONS.filter(sec=>cat.some(e=>e.group===sec.group)).map(sec=>
      `<section class="au05-section"><h3 class="au05-sec">${mark(sec)}<span class="au05-sec-title">${esc(sec.title)}</span></h3><div class="au05-grid">${cat.filter(e=>e.group===sec.group).map(e=>
        pcard(`uc:${e.id}`,actionIcon(e.id),shortName(e),{keys:e.keys&&e.keys!==e.label&&e.keys!==shortName(e)&&e.keys!=='Follows the target app'&&!/^(Play|Next|Previous|Volume|Mute)/.test(e.keys)?e.keys:'',
          unverified:e.status!=='verified',fresh:/^New\b/.test(e.note||''),title:`${e.group}: ${e.label}${e.keys&&e.keys!==e.label?` · ${e.keys}`:''}${e.status==='verified'?'':' · unverified'} · ${e.note}`})).join('')}</div></section>`).join('')}
      <section class="au05-section"><h3 class="au05-sec"><span class="au05-mark" aria-hidden="true" style="--hue:var(--accent)">${IC.dot}</span><span class="au05-sec-title">Unified Control</span></h3><div class="au05-grid">
        ${pcard('uc:quickMenu',actionIcon('quickMenu'),'Quick menu',{title:'Open the quick menu'})}${pcard('uc:hotkey',actionIcon('hotkey'),'Custom',{title:'Any key combination'})}${pcard('uc:none',actionIcon('none'),'Nothing',{title:'The key does nothing'})}</div></section>
      <section class="au05-section"><h3 class="au05-sec"><span class="au05-mark" aria-hidden="true" style="--hue:var(--hue-5)">${IC.scroll}</span><span class="au05-sec-title">Knob turn</span></h3><div class="au05-grid">
        ${Object.entries(TURN).map(([v,[ic,name,title]])=>pcard(`knob:${v}`,actionIcon(ic),name,{title:`Knob turn: ${title}`})).join('')}</div></section>`;
    const palette=`<section class="au05-box au05-palette${paletteOpen?'':' is-collapsed'}" aria-label="Actions">
      <header class="au05-palette-head"><button type="button" class="au05-palette-toggle" data-au05-palette-toggle aria-expanded="${paletteOpen}">${paletteOpen?IC.down:IC.next}<span>Actions</span></button>
        <span class="au05-muted au05-palette-hint">Drag onto a key, or click to set ${esc(ZONES[sel0]||'the selected part')}</span></header>
      ${paletteOpen?`<div class="au05-sections">${paletteSections}</div>`:''}</section>`;

    // ---- status (0.36.0: one block): receiver, Vibe Key and battery, each with a dot; the action, if any, in
    // one short line. Battery: icon with fill level, percent, low warning, last known while asleep, details on hover.
    const b=s.battery,pct=b?.percent,asleep=s.state==='asleep'||s.state==='waiting'||s.state==='off';
    const low=pct!=null&&pct<=15;
    const fill=pct==null?0:Math.max(3,Math.min(100,pct));
    const bTime=b?.at?new Date(b.at).toLocaleTimeString('en-AU',{hour12:false}):'';
    const bTitle=b?`Battery ${pct} % · ${(b.millivolts/1000).toFixed(2)} V${bTime?` · updated ${bTime}`:''}${b.charging===true?' · probably charging':''}${b.flags!=null?` · status flags 0x${b.flags.toString(16).padStart(2,'0')}${b.chargingConfirmed?'':' (charging bit not confirmed yet)'}`:''}`:'Battery: not read yet. Press a key to wake the Vibe Key.';
    const bText=pct==null?'—':b.full?'Full':`${pct} %`;
    const st=s.enabled?s.state:'disabled';
    const RX={not_found:['bad','No receiver','Plug in the Vibe Key receiver'],error:['bad','Receiver error','Use Reconnect'],disabled:['off','Disabled','Turn the Vibe Key on in General']};
    const KEY={waiting:['off','Vibe Key waiting','Press a key or turn the knob to wake it'],connected:['ok','Vibe Key awake',''],asleep:['off','Vibe Key asleep','Press a key or turn the knob to wake it'],
      off:['warn','Vibe Key probably off','Hold the power button 2–3 s'],paused:['warn','Sending paused','A reply was slow; it resumes in 30 s'],suspended:['bad','Sending stopped','Use Reconnect'],starting:['off','Starting','']};
    const rx=RX[st]||['ok','Receiver connected',''],key=RX[st]?null:KEY[st]||['off','Vibe Key',''];
    const action=key&&key[0]!=='ok'&&['off','suspended','paused'].includes(st)?key[2]:RX[st]?rx[2]:'';
    const dot=(c,text,title)=>`<span class="au05-st" title="${esc(title||text)}"><span class="au05-status-dot ${c}" aria-hidden="true"></span>${esc(text)}</span>`;
    const batteryMarkup=`<div class="au05-statusbar" id="au05-status" data-state="${esc(st)}" role="status">
      <div class="au05-st-row">${dot(rx[0],rx[1],rx[2])}${key?dot(key[0],key[1],key[2]):''}
      <span class="au05-battery${low?' low':''}${asleep?' asleep':''}${pct==null?' unknown':''}" title="${esc(bTitle)}">
      <svg class="au05-batt-icon" viewBox="0 0 44 22" aria-hidden="true"><rect x="1.5" y="2.5" width="36" height="17" rx="4" class="shell"/><rect x="39" y="8" width="3.5" height="6" rx="1.2" class="tip"/>
        <rect x="4.5" y="5.5" width="${(30*fill/100).toFixed(1)}" height="11" rx="2" class="level"/>${b?.charging===true?'<path d="M21 4.5 15.5 12h4.5l-1.5 5.5 6-8h-4.5l1.5-5z" class="bolt"/>':''}</svg>
      <span id="au05-battery" class="au05-batt-text" aria-label="${esc(bTitle)}">${esc(bText)}</span></span></div>
      ${action?`<div class="au05-st-action${st==='off'||st==='not_found'||st==='suspended'?' attention':''}">${esc(action)}</div>`:''}</div>`;

    // ---- profiles and target app ----
    const profiles=m.profiles||[];
    const profileCards=profiles.map((p,i)=>{
      const active=p.id===m.profile;
      const inner=active&&this.renaming?`<input id="au05-profile-name" class="au05-profile-input" data-au05-profile-name maxlength="60" value="${esc(p.name)}" aria-label="Profile name"${native}>`
        :`<button type="button" class="au05-profile-btn" data-au05-profile-pick="${esc(p.id)}" aria-pressed="${active}"${native}><span class="au05-dot"></span>${esc(p.name)}</button>`;
      return `<div class="au05-profile${active?' is-active':''}" style="--hue:var(--hue-${i%6+1})">${inner}${active&&!this.renaming?`<button type="button" class="au05-mini" data-au05-profile-op="rename" title="Rename" aria-label="Rename ${esc(p.name)}"${native}>${IC.edit}</button>${profiles.length>1?`<button type="button" class="au05-mini" data-au05-profile-op="remove" title="Remove" aria-label="Remove ${esc(p.name)}"${native}>${IC.trash}</button>`:''}`:''}</div>`;
    }).join('');
    const targetMarks={claude:SECTIONS[0],chatgpt:SECTIONS[1]};
    const topBar=`<div class="au05-top">
      <div class="au05-profiles" role="group" aria-label="Profiles">${profileCards}<button type="button" class="au05-profile au05-profile-add" data-au05-profile-op="add" title="New profile (a copy of this one)" aria-label="New profile"${native}>${IC.plus}</button></div>
      <div class="au05-target" title="What the Target app actions use"><span class="au05-target-label">Target</span><div class="au05-seg" role="group" aria-label="Target app">${(m.targets||[]).map(t=>`<button type="button" data-au05-target-pick="${esc(t.id)}" aria-pressed="${t.id===m.target}"${native}>${targetMarks[t.id]?mark(targetMarks[t.id]):''}${esc(t.name)}</button>`).join('')}</div></div></div>`;

    // ---- the quick menu editor ----
    const typeOf=e=>({action:'Action',profile:'Profile',target:'App',submenu:'Sub-menu',claudeSessions:'Sessions',claudeSession:'Session'})[e.type]||e.type;
    const whatOf=e=>e.type==='action'?(e.action?.kind==='hotkey'?`Keys ${e.action.value}`:actionName(e.action)):e.type==='profile'?((profiles.find(p=>p.id===e.profile)||{}).name||e.profile):e.type==='target'?((m.targets||[]).find(t=>t.id===e.target)?.name||e.target):e.type==='submenu'?`${(e.entries||[]).length} entries`:e.type==='claudeSessions'?'Live list':'One session';
    const iconOf=e=>e.type==='action'?actionIcon(e.action?.kind):e.type==='profile'?IC.profile:e.type==='target'?IC.target:e.type==='submenu'?IC.folder:IC.sessions;
    const thenSelect=(e,p)=>e.type==='claudeSessions'?`<select id="qm-then-${p}" data-qm-then="${p}" aria-label="After opening the session"${native}>${opt('none','Then nothing',e.then||'none')}${cat.filter(x=>x.group==='Claude'&&x.id!=='claudeStop').map(x=>opt(x.id,`Then ${x.label}`,e.then||'none')).join('')}</select>`:'';
    const rows=[];
    const walk=(list,path,depth)=>list.forEach((e,i)=>{
      const p=[...path,i].join('.');
      rows.push(`<li class="au05-menu-row" style="--depth:${depth}" data-qm-row="${p}">
        <span class="au05-grip" draggable="true" data-qm-drag="${p}" title="Drag to move" aria-hidden="true">${IC.grip}</span>
        <span class="au05-card-icon">${iconOf(e)}</span>
        <input id="qm-label-${p}" data-qm-label="${p}" maxlength="60" value="${esc(e.label)}" aria-label="Name of menu entry ${esc(e.label)}"${native}>
        <button type="button" class="au05-what" data-qm-op="change" data-qm-path="${p}" title="Change what it does"${native}><span class="au05-type">${esc(typeOf(e))}</span><span class="au05-what-text">${esc(whatOf(e))}</span></button>
        ${thenSelect(e,p)}
        <span class="au05-menu-tools">${e.type==='submenu'&&depth<1?`<button type="button" class="au05-mini" data-qm-op="add" data-qm-path="${p}" title="Add inside" aria-label="Add inside ${esc(e.label)}"${native}>${IC.plus}</button>`:''}<button type="button" class="au05-mini" data-qm-op="up" data-qm-path="${p}" aria-label="Move ${esc(e.label)} up"${i===0?' disabled':native}>${IC.up}</button><button type="button" class="au05-mini" data-qm-op="down" data-qm-path="${p}" aria-label="Move ${esc(e.label)} down"${i===list.length-1?' disabled':native}>${IC.down}</button><button type="button" class="au05-mini" data-qm-op="remove" data-qm-path="${p}" aria-label="Remove ${esc(e.label)}"${native}>${IC.close}</button></span></li>`);
      if(e.type==='submenu')walk(e.entries||[],[...path,i],depth+1);
    });
    walk(m.entries||[],[],0);
    const pick=this.menuPick;
    const pickedEntry=pick?.mode==='change'?(()=>{const [l,i]=Au05Panel.locate(m.entries||[],pick.path);return l[i];})():null;
    const pickedValue=pickedEntry?(pickedEntry.type==='action'?`a:${pickedEntry.action?.kind}`:pickedEntry.type==='profile'?`p:${pickedEntry.profile}`:pickedEntry.type==='target'?`t:${pickedEntry.target}`:pickedEntry.type==='submenu'?'submenu':'sessions'):'';
    const qmCard=(v,icon,name,title)=>card('data-qm-pick=""',v,icon,name,pickedValue===v,{title});
    const picker=pick?`<div class="au05-picker" role="dialog" aria-label="Choose a menu entry"><div class="au05-picker-head"><h3>${pick.mode==='change'?'Change entry':'Add entry'}</h3><button type="button" class="au05-mini" data-qm-op="cancel" aria-label="Close">${IC.close}</button></div>
      ${catalogueSections('data-qm-pick=""','a:',pickedValue,`<section class="au05-section"><h3 class="au05-sec"><span class="au05-mark" style="--hue:var(--accent)">${IC.menu}</span>Menu</h3><div class="au05-grid">
        ${profiles.map(p=>qmCard(`p:${p.id}`,IC.profile,p.name,`Switch to the profile ${p.name}`)).join('')}
        ${(m.targets||[]).map(t=>qmCard(`t:${t.id}`,IC.target,`Target ${t.name}`,`Target app: ${t.name}`)).join('')}
        ${qmCard('submenu',IC.folder,'Sub-menu','A menu inside the menu')}${qmCard('sessions',IC.sessions,'Claude sessions','The live list of Claude sessions')}</div></section>`)}</div>`:'';
    // What K2, K3 and the knob press do while the menu is open.
    const ROLE={nextMenu:'Next menu',select:'Select',back:'Back',close:'Close',none:'Its own action'};
    const ctl=m.controls||{};
    const roleSel=(k,label)=>`<label class="au05-role"><span>${esc(label)}</span><select data-qm-role="${k}" aria-label="${esc(label)} while the menu is open"${native}>${Object.entries(ROLE).map(([v,t])=>opt(v,t,ctl[k]||'none')).join('')}</select></label>`;
    const usesCodes=anyMenu?['k2','k3'].filter(k=>(ctl[k]||'none')!=='none'&&(s.actions?.[k]?.kind||'offline')==='offline').map(k=>k.toUpperCase()):[];
    const codesTip=`${usesCodes.length?`${usesCodes.join(' and ')} type ${usesCodes.length>1?'their':'its'} own code${usesCodes.length>1?'s':''} (${usesCodes.map(k=>k==='K2'?'F21':'F22').join(', ')}) while Unified Control runs, so the menu can use ${usesCodes.length>1?'them':'it'}; when the menu is closed, Unified Control types ${usesCodes.length>1?'their':'its'} usual keys. `:''}Their normal actions wait while the menu is open. The knob turn moves the highlight.`;
    const controlsBox=`<div class="au05-inmenu"><span class="au05-inmenu-title" title="${esc(codesTip)}">In the menu</span>
      <span class="au05-inmenu-roles">${roleSel('k2','K2')}${roleSel('k3','K3')}${roleSel('knobPress','Knob press')}</span>
      <label class="au05-reopen" title="The menu opens at the sub-menu and entry it was closed on, for each profile"><input type="checkbox" role="switch" data-qm-reopen=""${m.reopenWhereLeft!==false?' checked':''}${native}> Reopen where I left off</label>
      <label class="au05-role" title="Claude sessions you have not spoken to for this long fold into a &quot;More&quot; entry at the end of their list; open it to see them all"><span>Fold sessions after</span><select data-qm-fold="" aria-label="Fold Claude sessions not used for"${native}>${[[1,'1 hour'],[4,'4 hours'],[8,'8 hours'],[24,'1 day'],[72,'3 days'],[0,'Never']].map(([v,t])=>opt(v,t,m.sessionFoldHours??0)).join('')}</select></label></div>`;
    const profileName=(profiles.find(p=>p.id===m.profile)||{}).name||'';
    const menuBox=`<section class="au05-box"><header class="au05-box-head"><div class="au05-box-title"><span class="au05-box-icon">${IC.menu}</span><h2>Quick menu</h2>${profileName?`<span class="au05-muted">for the profile ${esc(profileName)}</span>`:''}</div>
      <div class="au05-box-actions"><button class="btn" type="button" data-qm-op="open"${live}>Open</button><button class="btn" type="button" data-qm-op="reset"${native}>Reset</button></div></header>
      <ul class="au05-menu-list" aria-label="Quick menu entries"><li class="au05-menu-row au05-menu-fixed" style="--depth:0" title="Always the first entry; inside a sub-menu the first entry is Back"><span class="au05-card-icon">${IC.close}</span><span class="au05-fixed-label">Close</span><span class="au05-muted">Always first</span></li>${rows.join('')}</ul>
      <div class="au05-box-foot"><button class="btn" type="button" data-qm-op="add-top"${native}>${IC.plus} Add entry</button>${m.last?`<span class="au05-muted">Last: ${esc(m.last)}</span>`:''}</div>${controlsBox}${picker}${this.facesMarkup(m.faces||[],native)}</section>`;

    // ---- Claude sessions ----
    const cs=s.claude||{},full=this.claudeFull||{};
    const when=ms=>ms?new Date(ms).toLocaleTimeString('en-AU',{hour12:false,hour:'2-digit',minute:'2-digit'}):'';
    const csState=full.blocked?'bad':cs.connected?(full.hooks&&full.hooks!=='connected'?'warn':'ok'):'off';
    const csText=full.blocked?'Blocked by Claude settings':cs.connected?(full.hooks&&full.hooks!=='connected'?'Hooks missing: connect again':cs.lastEvent?`Connected · last event ${when(cs.lastEvent)}`:'Connected · no event yet'):'Not connected';
    const src=m.sessions||{};
    const plural=(n,w)=>`${n} ${w}${n===1?'':'s'}`;
    const counts=src.open!=null?`${plural(src.open|0,'open session')} · ${plural(src.projects|0,'project')} · ${src.working|0} working`:'No session list yet';
    const recent=(src.recent||[]).map(x=>`<li class="au05-cs-row${x.working?' working':''}"><span class="au05-cs-dot" aria-label="${x.working?'working':'idle'}"></span><span class="au05-cs-title">${esc(x.title)}</span><span class="au05-muted">${esc(x.project||'')}</span><span class="au05-muted au05-cs-when">${esc(when(x.at))}</span></li>`).join('');
    const fallback=src.source==='hooks'?(src.note||'Claude\'s session list was not found'):src.groupsNote||'';
    const liveTip=full.blocked||`Adds the live working dot through Claude Code hooks${full.settingsPath?` in ${full.settingsPath}`:''}; merged and backed up first. Turning it off removes only those entries. Sessions report once they start (or restart) after it is on.`;
    const claudeBox=`<section class="au05-box au05-claude" aria-label="Claude sessions"><header class="au05-box-head"><div class="au05-box-title"><span class="au05-box-icon">${IC.sessions}</span><h2>Claude sessions</h2></div>
      <label class="au05-live" title="${esc(liveTip)}"><input type="checkbox" role="switch" data-claude-live=""${cs.connected&&!(full.hooks&&full.hooks!=='connected')?' checked':''}${native||(!cs.connected&&(full.blocked||full.error))?' disabled':''}> Live working status: ${cs.connected?(full.hooks&&full.hooks!=='connected'?'needs reconnecting':'On'):'Off'}</label></header>
      <div class="au05-cs-counts" id="au05-cs-counts">${esc(counts)}</div>
      ${recent?`<ul class="au05-cs-list" aria-label="Latest sessions">${recent}</ul>`:''}
      <div class="au05-cs-source${fallback?' attention':''}">${fallback?esc(`${fallback}: ${src.source==='hooks'?'showing the sessions reported by live status':'grouped by folder'}`):"From Claude's session list"}${src.readAt?` · <span id="au05-cs-age">${esc(ago(src.readAt))}</span>`:''}</div>
      ${full.blocked?`<p class="au05-line attention">${esc(full.blocked)}</p>`:''}${full.error?`<p class="au05-line attention">${esc(full.error)}</p>`:''}
      <details class="au05-claude-preview"><summary>What live status adds</summary><pre>${esc(full.preview||'')}</pre></details></section>`;

    // ---- device settings ----
    const standby=s.standbyS;const standbyOpts=STANDBY.some(([v])=>v===standby)||standby==null?STANDBY:[...STANDBY,[standby,minutes(standby)]];
    const sleepOpts=Object.entries(SLEEP).some(([v])=>Number(v)===s.sleepS)||s.sleepS==null?Object.entries(SLEEP):[...Object.entries(SLEEP),[String(s.sleepS),`${s.sleepS} seconds`]];
    const light=s.light||{};const range=s.lightRange||{modeMax:3,brightnessMax:10};
    const fmax=s.lightFieldMax||{type:3,time:15,breatheLevel:15,breatheBrightness:10,alwaysOnBrightness:10};
    const numbers=(max,sel)=>Array.from({length:max+1},(_,i)=>opt(i,String(i),sel)).join('');
    const FIELDS=[['type','Type'],['time','Time'],['breatheLevel','Breathe'],['breatheBrightness','Breathe bright.'],['alwaysOnBrightness','Always-on']];
    const records=(light.records||[]).map((rec,r)=>`<tr><th scope="row">${r}</th>${FIELDS.map(([f,label],i)=>`<td><select data-au05-record="${r}" data-au05-field="${f}" aria-label="Record ${r} ${label}"${dis}>${numbers(Math.max(fmax[f],rec[i]??0),rec[i])}</select></td>`).join('')}</tr>`).join('');
    const codes=(s.controlCodes||[]).map((c,i)=>c==null?'':`<span class="au05-code"><span class="au05-muted">${['K1','K2','K3','Press','Right','Left'][i]}</span> ${esc(c)}</span>`).filter(Boolean).join('');
    // 0.37.0: each setting's last change: waiting for a touch, sending, applied, what the key kept, or not applied.
    const changes=s.changes||{};
    const pending=key=>['touch','sending'].includes(changes[key]?.state)?changes[key].value:undefined;
    const chg=key=>{const c=changes[key];if(!c)return '';
      const recent=Date.now()-(c.at||0)<30000;
      if(c.state==='touch')return '<span class="au05-chg warn" role="status">Touch the Vibe Key to apply</span>';
      if(c.state==='sending')return '<span class="au05-chg">Sending…</span>';
      if(c.state==='applied')return recent?'<span class="au05-chg ok">Applied</span>':'';
      if(c.state==='kept')return `<span class="au05-chg warn" title="What the key read back after the change">${esc(c.message||'The key kept another value')}</span>`;
      if(c.state==='failed')return `<span class="au05-chg bad">${esc(c.message||'Not applied')}</span>`;
      return recent&&c.message?`<span class="au05-chg">${esc(c.message)}</span>`:'';};
    const row=(label,untested,control,rawText,key)=>`<div class="au05-set-row"><div class="au05-set-label">${esc(label)}${untested}${raw(rawText)}</div><div class="au05-set-ctl">${control}${key?chg(key):''}</div></div>`;
    // The light modes, as checked on the real key (2026-11-17): a tile each with the lit parts glowing.
    const LIGHT_MODES=[['Off','Everything off',[]],['All','Dial and keys lit',['dial','keys']],['Dial','Dial only',['dial']],['Keys','Keys only (maybe just the active key)',['keys']]];
    const lightTile=(i,[name,title,lit],cur)=>{const on=p=>lit.includes(p)?' lit':'';
      return `<button type="button" class="au05-ltile" data-au05-light-mode="${i}" aria-pressed="${cur===i}" title="${esc(title)}"${dis}>
        <svg viewBox="0 0 40 56" aria-hidden="true"><rect x="6" y="2" width="28" height="52" rx="7" class="body"/><circle cx="20" cy="15" r="8" class="part${on('dial')}"/>
        <rect x="13" y="27" width="14" height="7" rx="2" class="part${on('keys')}"/><rect x="13" y="36" width="14" height="7" rx="2" class="part${on('keys')}"/><rect x="13" y="45" width="14" height="7" rx="2" class="part${on('keys')}"/></svg>
        <span>${esc(name)}${i===3?' <span class="au05-unv" aria-label="not fully known">?</span>':''}</span></button>`;};
    const modeNow=pending('lightMode')??light.mode;
    const lightPicker=`<div class="au05-lpick" role="group" aria-label="Light mode">${LIGHT_MODES.map((m,i)=>lightTile(i,m,modeNow)).join('')}</div>`;
    const brightNow=pending('lightBrightness')??light.brightness;
    const brightness=`<span class="au05-slider"><input type="range" min="0" max="${Math.max(range.brightnessMax,light.brightness??0)}" step="1" value="${brightNow??0}" data-au05-set="lightBrightness" aria-label="All-on brightness"${brightNow==null?' disabled':dis}><span class="au05-value">${brightNow??'—'}</span></span>`;
    const notRead=v=>v==null?opt('','Not read','') :'';
    const deviceCard=(icon,title,body,wide=false)=>`<section class="au05-box au05-set-card${wide?' au05-wide':''}"><h3 class="au05-box-title"><span class="au05-box-icon">${icon}</span>${esc(title)}</h3>${body}</section>`;
    const device=`<div class="au05-section-head"><h2>Device</h2><div class="au05-box-actions"><button class="btn" data-au05="read-all" type="button">Read all settings</button><button class="btn" data-au05="reconnect" type="button">Reconnect</button></div></div>
      ${s.readsFailed?`<div class="au05-reads-failed" role="status"><span>${esc(s.readsFailed)}</span><button class="btn" data-au05="read-all" type="button">Read again</button></div>`:''}
      <div class="au05-settings-grid">
      ${deviceCard(IC.power,'Power',
        row('Standby',this.untested('standbyTime'),`<select data-au05-set="standbyTime" aria-label="Standby time"${dis}>${notRead(standby)}${standbyOpts.map(([v,l])=>opt(v,l,pending('standbyTime')??standby)).join('')}</select>`,standby==null?null:`${standby} s`,'standbyTime')+
        row('Power off',this.untested('sleepTime'),`<select data-au05-set="sleepTime" aria-label="Power-off time" title="&quot;Never&quot; stops the key switching itself off, at some battery cost"${dis}>${notRead(s.sleepS)}${sleepOpts.map(([v,l])=>opt(v,l,pending('sleepTime')??s.sleepS)).join('')}</select>`,s.sleepS==null?null:`${s.sleepS} s`,'sleepTime')+
        row('Standby status','',`<span class="au05-value">${s.standbyStatus==null?'Not read':esc(s.standbyStatus)}</span>`,null))}
      ${deviceCard(IC.light,'Light',
        row('Mode',this.untested('lightMode'),light.mode==null&&pending('lightMode')==null?'<span class="au05-value">Not read</span>':lightPicker,light.mode==null?null:`mode ${light.mode}`,'lightMode')+
        row('Brightness',this.untested('lightBrightness'),brightness,null,'lightBrightness')+
        row('Hooks mode',this.untested('hooksMode'),`<select data-au05-set="hooksMode" aria-label="Hooks mode"${dis}>${notRead(s.hooksMode)}${opt(0,'Off',pending('hooksMode')??s.hooksMode)}${opt(1,'On',pending('hooksMode')??s.hooksMode)}</select>`,null,'hooksMode')+
        `<details class="au05-advanced"${this.advancedOpen?' open':''}><summary data-au05-advanced="">Advanced: mode records${this.untested('lightRecord')}</summary>${records?`<div class="au05-table"><table><thead><tr><th></th>${FIELDS.map(([,l])=>`<th scope="col">${l}</th>`).join('')}</tr></thead><tbody>${records}</tbody></table></div>`:'<p class="au05-muted">Not read yet.</p>'}${Object.keys(changes).filter(k=>k.startsWith('lightRecord:')).map(chg).join('')}</details>`,true)}
      ${deviceCard(IC.mic,'Microphone',
        row('Noise reduction',this.untested('micNr'),`<select data-au05-set="micNr" aria-label="Noise reduction"${dis}>${notRead(s.micNr)}${Array.from({length:Math.max(s.micNrMax??3,s.micNr?.level??0)+1},(_,i)=>opt(i,i===0?'Off':`Level ${i}`,pending('micNr')??s.micNr?.level)).join('')}</select>`,s.micNr?`${s.micNr.level}, ${s.micNr.a}, ${s.micNr.b}`:null,'micNr'))}
      ${codes?deviceCard(IC.sliders,'Controls',`<div class="au05-codes">${codes}</div>`):''}
      </div>`;

    return `${topBar}${layerBar}<div class="au05-workspace"><div class="au05-drawing">${batteryMarkup}${diagram}</div>
    <div class="au05-side"><aside class="au05-editor key-editor" aria-label="${esc(ZONES[this.selected])} settings">${editor}</aside>${offlineView?'':palette}</div></div>
    <div class="au05-boxes">${menuBox}${claudeBox}</div>
    ${device}`;
  }
  async change(change){
    this.sending++;this.setNote('Sending to the Vibe Key…');this.paint(true);
    try{const st=await this.invoke('au05_change',{change});this.state=st;
      const key=change.setting==='lightRecord'?`lightRecord:${change.record}:${change.field}`:change.setting;
      const c=st?.changes?.[key];
      this.setNote(c?.state==='touch'?'Touch the Vibe Key to apply.':c?.state==='failed'?(c.message||'Not applied.'):c?.state==='kept'?c.message:'Saved on the Vibe Key.',c?.state==='failed'||c?.state==='kept');return true;}
    catch(error){this.setNote(errorText(error),true);return false;}
    finally{this.sending--;this.paint(true);this.onState();}
  }
  // Choosing a UC action (or "Same as on its own") only saves it; Unified Control's layer sync then makes
  // the key type its own code (or its offline typing), one key command at a time, and the panel shows it.
  async chooseAction(key,kind){
    await this.action(key,kind,kind==='hotkey'?(this.state?.actions?.[key]?.value||''):'',kind==='hotkey'?(this.state?.actions?.[key]?.app||''):'');
  }
  // The knob turn with Unified Control: its own codes F23/F24 (scroll, volume or nothing) or the same as on
  // its own.
  async chooseTurn(value){
    const uc=value.startsWith('uc:');
    await this.configure(uc?{knobTurn:'uc',knobTurnAction:value.slice(3)}:{knobTurn:'factory'});
  }
  // The offline layer: what one control types on its own.
  async setOffline(control,mode,keys){
    await this.configure(mode==='keys'?{offline:{control,mode,keys}}:{offline:{control,mode}});
  }
  // What a part has now, so an assignment can be undone.
  snapshot(part){
    const s=this.state||{};
    if(part==='knobTurn')return {part,knobTurn:s.menu?.knobTurn||'factory',act:s.menu?.knobTurnAction||'scroll'};
    return {part,action:{...(s.actions?.[part]||{kind:'offline',value:'',app:''})}};
  }
  async restore(snap){
    if(snap.part==='knobTurn'){await this.chooseTurn(snap.knobTurn==='uc'?`uc:${snap.act}`:'factory');return;}
    this.modeView[snap.part]=null;
    await this.action(snap.part,snap.action.kind||'offline',snap.action.value||'',snap.action.app||'');
  }
  // Give a part an action (a dropped or clicked palette card). A key in "Types keys" or "Factory" becomes a UC
  // action (the usual key set-up runs, with its status pill). Shows Undo for about 6 seconds.
  async assign(part,value,name){
    if(part==='knobTurn'?!value.startsWith('knob:'):!value.startsWith('uc:'))return false;
    const snap=this.snapshot(part);
    const label=name||this.container?.querySelector(`[data-au05-palette][data-value="${CSS.escape(value)}"] .au05-card-name`)?.textContent||value;
    this.selected=part;
    if(part==='knobTurn'){this.turnView='uc';await this.chooseTurn(`uc:${value.slice(5)}`);}
    else{this.modeView[part]='uc';await this.chooseAction(part,value.slice(3));}
    this.paint(true);
    for(const el of this.container?.querySelectorAll(`.au05-diagram [data-au05-zone="${part}"]`)||[]){el.classList.remove('au05-flash');void el.getBoundingClientRect();el.classList.add('au05-flash');setTimeout(()=>el.classList.remove('au05-flash'),900);}
    this.toast(`${ZONES[part]}: ${label}`,()=>this.restore(snap));
    return true;
  }
  toast(text,undo){
    const t=this.container?.querySelector('#au05-toast');if(!t)return;
    this.undo=undo;t.querySelector('.au05-toast-text').textContent=text;t.hidden=false;
    t.querySelector('[data-au05-undo]').hidden=!undo;t.classList.toggle('bad',!undo);
    clearTimeout(this.toastTimer);this.toastTimer=setTimeout(()=>{t.hidden=true;this.undo=null;},6000);
  }
  // Key typing is read only on request, one key at a time (the key can take up to 20 s to answer).
  async readKey(key){
    this.sending++;this.setNote('Reading from the Vibe Key… This can take up to 20 seconds.');this.paint(true);
    try{this.state=await this.invoke('au05_read_key',{key});this.setNote('');}
    catch(error){this.setNote(errorText(error),true);}
    finally{this.sending--;this.paint(true);}
  }
  async action(control,kind,value,app=''){
    try{this.state=await this.invoke('au05_configure',{patch:{action:{control,kind,value,app}}});this.setNote('Saved.');}
    catch(error){this.setNote(errorText(error),true);}
    finally{this.paint(true);}
  }
  // The quick menu is saved whole after each edit (add, remove, move, rename, change what an entry does).
  editMenu(fn){
    const entries=structuredClone(this.state?.menu?.entries||[]);
    fn(entries);this.configure({menu:entries});
  }
  static locate(entries,path){
    const parts=String(path).split('.').map(Number);let list=entries;
    for(const i of parts.slice(0,-1))list=list[i]?.entries||[];
    return [list,parts[parts.length-1]];
  }
  entryFor(what,old){
    const s=this.state||{},m=s.menu||{};
    if(what.startsWith('a:')){const id=what.slice(2),x=(s.catalogue?.actions||[]).find(e=>e.id===id);
      return {type:'action',label:x?`${x.group==='Windows / media'?'':`${x.group} `}${x.label}`:id,action:{kind:id,value:'',app:''}};}
    if(what.startsWith('p:')){const id=what.slice(2);return {type:'profile',label:(m.profiles||[]).find(p=>p.id===id)?.name||id,profile:id};}
    if(what.startsWith('t:')){const id=what.slice(2);return {type:'target',label:(m.targets||[]).find(t=>t.id===id)?.name||id,target:id};}
    if(what==='submenu')return {type:'submenu',label:old?.type==='submenu'?old.label:'Sub-menu',entries:old?.entries||[]};
    if(what==='sessions')return {type:'claudeSessions',label:'Claude sessions',then:'none'};
    return old?.type==='claudeSession'?old:{type:'claudeSessions',label:'Claude sessions',then:'none'};
  }
  menuOp(op,path){
    if(op==='open'){this.invoke('au05_menu',{input:{op:'toggle'}}).catch(error=>this.setNote(errorText(error),true));return;}
    if(op==='reset'){this.configure({menu:'default'});return;}
    if(op==='add-top'||op==='add'||op==='change'){this.menuPick={mode:op,path};this.paint(true);
      this.container?.querySelector('.au05-picker')?.scrollIntoView({block:'nearest'});return;}
    if(op==='cancel'){this.menuPick=null;this.paint(true);return;}
    this.editMenu(entries=>{
      const [list,i]=Au05Panel.locate(entries,path);
      if(op==='up'&&i>0)[list[i-1],list[i]]=[list[i],list[i-1]];
      if(op==='down'&&i<list.length-1)[list[i+1],list[i]]=[list[i],list[i+1]];
      if(op==='remove')list.splice(i,1);
    });
  }
  // A card picked in the menu picker: add it (at the end, or inside a sub-menu) or replace the entry.
  menuPicked(what){
    const pick=this.menuPick;if(!pick)return;this.menuPick=null;
    this.editMenu(entries=>{
      if(pick.mode==='add-top'){entries.push(this.entryFor(what));return;}
      const [list,i]=Au05Panel.locate(entries,pick.path);if(!list[i])return;
      if(pick.mode==='add'&&list[i].type==='submenu')(list[i].entries||=[]).push(this.entryFor(what));
      if(pick.mode==='change')list[i]=this.entryFor(what,list[i]);
    });
  }
  // Delegated events from the page.
  handle(e){
    const t=e.target;
    // Live working status (Connect / Disconnect of the Claude Code hooks).
    if(e.type==='change'&&t.matches?.('[data-claude-live]')){this.claudeOp(t.checked?'connect':'disconnect');return true;}
    const cop=e.type==='click'&&t.closest?.('[data-claude-op]');
    if(cop){this.claudeOp(cop.dataset.claudeOp);return true;}
    const click=sel=>e.type==='click'&&t.closest?.(sel);
    const pal=click('[data-au05-palette]');
    if(pal){if(!this.justDragged&&pal.getAttribute('aria-disabled')!=='true')this.assign(this.selected,pal.dataset.value,pal.querySelector('.au05-card-name')?.textContent);return true;}
    if(click('[data-au05-palette-toggle]')){this.paletteOpen=!this.paletteOpen;try{localStorage.setItem('uc.au05.palette',this.paletteOpen?'open':'closed');}catch{}this.paint(true);return true;}
    if(click('[data-au05-undo]')){const u=this.undo;this.undo=null;const t=this.container?.querySelector('#au05-toast');if(t)t.hidden=true;clearTimeout(this.toastTimer);if(u)u();return true;}
    const qmPick=click('[data-qm-pick]');
    if(qmPick){this.menuPicked(qmPick.dataset.value);return true;}
    const cardPick=click('[data-au05-pick]');
    if(cardPick){const key=cardPick.dataset.au05Pick;this.chooseAction(key,cardPick.dataset.value.slice(3));return true;}
    const segment=click('[data-au05-segment]');
    if(segment){const key=segment.closest('[data-au05-mode]').dataset.au05Mode,v=segment.dataset.au05Segment;
      this.modeView[key]=v;
      if(v==='offline')this.chooseAction(key,'offline');else this.paint(true);
      return true;}
    const turnSeg=click('[data-au05-turn-mode]');
    if(turnSeg){this.turnView=turnSeg.dataset.au05TurnMode;this.chooseTurn(this.turnView==='offline'?'factory':`uc:${this.state?.menu?.knobTurnAction||'scroll'}`);return true;}
    const lv=click('[data-au05-layer-view]');
    if(lv){this.layerView=lv.dataset.au05LayerView;this.paint(true);return true;}
    const ly=click('[data-au05-layer]');
    if(ly){this.configure({layer:ly.dataset.au05Layer});return true;}
    const op=click('[data-au05-offline-pick]');
    if(op){const c=op.dataset.control;this.offlineView[c]='keys';this.setOffline(c,'keys',op.dataset.au05OfflinePick);return true;}
    const al=click('[data-au05-also]');
    if(al){this.setOffline(al.dataset.control,'keys',al.dataset.au05Also);return true;}
    const om=click('[data-au05-offline-mode]');
    if(om){const c=om.dataset.control,v=om.dataset.au05OfflineMode;this.offlineView[c]=v;
      if(v==='factory')this.setOffline(c,'factory');else{this.paint(true);this.container.querySelector(`[data-au05-offline-keys="${c}"]`)?.focus();}
      return true;}
    const turnPick=click('[data-au05-turn-pick]');
    if(turnPick){this.chooseTurn(turnPick.dataset.au05TurnPick);return true;}
    const profPick=click('[data-au05-profile-pick]');
    if(profPick){if(profPick.getAttribute('aria-pressed')!=='true')this.configure({profile:profPick.dataset.au05ProfilePick});return true;}
    const tgtPick=click('[data-au05-target-pick]');
    if(tgtPick){this.configure({target:tgtPick.dataset.au05TargetPick});return true;}
    const qm=e.type==='click'&&t.closest?.('[data-qm-op]');
    if(qm){this.menuOp(qm.dataset.qmOp,qm.dataset.qmPath);return true;}
    const pop=e.type==='click'&&t.closest?.('[data-au05-profile-op]');
    if(pop){
      const m=this.state?.menu||{};
      if(pop.dataset.au05ProfileOp==='rename'){this.renaming=true;this.paint(true);const i=this.container.querySelector('[data-au05-profile-name]');i?.focus();i?.select();return true;}
      if(pop.dataset.au05ProfileOp==='add')this.configure({profileEdit:{op:'add',name:`Profile ${(m.profiles||[]).length+1}`}});
      else if(confirm(`Remove the profile "${(m.profiles||[]).find(p=>p.id===m.profile)?.name||''}"? Its key actions are deleted.`))this.configure({profileEdit:{op:'remove',id:m.profile}});
      return true;
    }
    if(e.type==='click'&&t.closest?.('[data-au05="reconnect"]')){
      this.invoke('au05_reconnect').then(s=>{this.state=s;this.setNote('Reconnecting.');this.paint(true);}).catch(error=>this.setNote(errorText(error),true));
      return true;
    }
    if(e.type==='click'&&t.closest?.('[data-au05="read-all"]')){
      this.invoke('au05_read_all').then(s=>{this.state=s;this.setNote('Reading all settings, one at a time. Key settings are read per key.');this.paint(true);}).catch(error=>this.setNote(errorText(error),true));
      return true;
    }
    // The light mode tiles (0.37.0) and the Advanced detail (its open state survives a repaint).
    const tile=e.type==='click'&&t.closest?.('[data-au05-light-mode]');
    if(tile){if(!tile.disabled)this.change({setting:'lightMode',value:Number(tile.dataset.au05LightMode)});return true;}
    if(e.type==='click'&&t.closest?.('[data-au05-advanced]')){this.advancedOpen=!t.closest('details').open;return false;}
    const zone=e.type==='click'&&t.closest?.('[data-au05-zone]');
    if(zone){this.select(zone.dataset.au05Zone);return true;}
    const retry=e.type==='click'&&t.closest?.('[data-au05-retry]');
    if(retry){this.change({setting:'retryJob',key:retry.dataset.au05Retry});return true;}
    const fop=e.type==='click'&&t.closest?.('[data-qm-face-op]');
    if(fop){const [op,i0]=fop.dataset.qmFaceOp.split(':'),i=Number(i0);const f=(this.state?.menu?.faces||[]).map(x=>({...x}));
      if(op==='add')f.push({name:'Recent sessions',style:'list',source:'recent'});
      else if(op==='remove')f.splice(i,1);
      else{const j=op==='up'?i-1:i+1;if(f[i]&&f[j])[f[i],f[j]]=[f[j],f[i]];}
      this.configure({menuFaces:f});return true;}
    const read=e.type==='click'&&t.closest?.('[data-au05-read]');
    if(read){this.readKey(read.dataset.au05Read);return true;}
    if(e.type!=='change')return false;
    if(t.dataset.qmLabel!==undefined){const path=t.dataset.qmLabel;this.editMenu(en=>{const [l,i]=Au05Panel.locate(en,path);if(l[i])l[i].label=t.value;});return true;}
    if(t.dataset.qmReopen!==undefined){this.configure({reopenWhereLeft:t.checked});return true;}
    if(t.dataset.qmFold!==undefined){this.configure({sessionFoldHours:Number(t.value)});return true;}
    if(t.dataset.qmFace!==undefined){const faces=(this.state?.menu?.faces||[]).map(f=>({...f}));const [i,k]=t.dataset.qmFace.split(':');if(faces[i]){faces[i][k]=t.value;this.configure({menuFaces:faces});}return true;}
    if(t.dataset.qmRole!==undefined){this.configure({menuControls:{[t.dataset.qmRole]:t.value}});return true;}
    if(t.dataset.qmThen!==undefined){const path=t.dataset.qmThen;this.editMenu(en=>{const [l,i]=Au05Panel.locate(en,path);if(l[i])l[i].then=t.value;});return true;}
    if(t.dataset.qmWhat!==undefined){const path=t.dataset.qmWhat;this.editMenu(en=>{const [l,i]=Au05Panel.locate(en,path);if(l[i])l[i]=this.entryFor(t.value,l[i]);});return true;}
    if(t.dataset.au05Profile!==undefined){this.configure({profile:t.value});return true;}
    if(t.dataset.au05Target!==undefined){this.configure({target:t.value});return true;}
    if(t.dataset.au05ProfileName!==undefined){this.renaming=false;this.configure({profileEdit:{op:'rename',id:this.state?.menu?.profile,name:t.value}});return true;}
    if(t.dataset.au05Turn!==undefined){this.chooseTurn(t.value);return true;}
    if(t.dataset.au05Set){if(t.value!=='')this.change({setting:t.dataset.au05Set,value:Number(t.value)});return true;}
    if(t.dataset.au05Record){this.change({setting:'lightRecord',record:Number(t.dataset.au05Record),field:t.dataset.au05Field,value:Number(t.value)});return true;}
    if(t.dataset.au05Mode){
      const key=t.dataset.au05Mode,input=this.container.querySelector(`[data-au05-keys="${key}"]`);
      if(t.value==='keys'){input.hidden=false;input.focus();return true;}
      if(t.value==='unknown')return true;
      if(t.value.startsWith('uc:')){this.chooseAction(key,t.value.slice(3));return true;}
      this.setMode(key,t.value);return true;
    }
    if(t.dataset.au05OfflineKeys!==undefined){this.setOffline(t.dataset.au05OfflineKeys,'keys',t.value);return true;}
    if(t.dataset.au05ActionValue){const key=t.dataset.au05ActionValue;this.action(key,'hotkey',t.value,this.state?.actions?.[key]?.app||'');return true;}
    if(t.dataset.au05ActionApp){const key=t.dataset.au05ActionApp;this.action(key,'hotkey',this.state?.actions?.[key]?.value||'',t.value);return true;}
    return false;
  }
}
