// Ulanzi AU05 Vibe Key: the General switch, the Home summary and the device page.
// Settings stored on the key are sent as they are changed (no Save button). UC's own settings
// (the switch and a UC action per key) are saved in settings.json at once.
import {ICON as IC,actionIcon,SHORT,SECTIONS} from './au05-icons.js';
const esc=s=>String(s??'').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
const ICON='<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true"><rect x="3" y="7" width="18" height="10" rx="3"/><circle cx="7.5" cy="12" r="1.5"/><circle cx="12" cy="12" r="1.5"/><circle cx="16.5" cy="12" r="1.5"/><path d="M12 4v3"/></svg>';
export const AU05_ICON=ICON;
const STATES={
  disabled:'Disabled',starting:'Starting',waiting:'Waiting for the key',connected:'Connected',asleep:'Asleep',
  paused:'Paused',suspended:'Stopped',not_found:'Not plugged in',error:'Error',
};

const STANDBY=[[60,'1 minute'],[120,'2 minutes'],[300,'5 minutes'],[600,'10 minutes'],[900,'15 minutes'],[1800,'30 minutes'],[3600,'1 hour']];
const SLEEP={3600:'1 hour',7200:'2 hours',10800:'3 hours',14400:'4 hours',0:'Never'};
const errorText=e=>typeof e==='string'?e:e?.error||e?.message||'The Vibe Key request failed';
// The diagram's zones, in the order of the real key (Ulanzi's product image, docs.ulanzistudio.com):
// logo and microphone grille on top, a large round knob with a light ring, then K1 (microphone icon),
// K2 (tick) and K3 (cross) in one column.
const ZONES={knobTurn:'Knob turn',knobPress:'Knob press',k1:'K1 · voice',k2:'K2 · confirm',k3:'K3 · cancel'};
const EVENT_ZONE={'K1':'k1','K2':'k2','K3':'k3','knob press':'knobPress','knob right':'knobTurn','knob left':'knobTurn'};

const clip=(t,n=24)=>t.length>n?t.slice(0,n-1)+'…':t;
const minutes=s=>s%60===0?`${s/60} minute${s===60?'':'s'}`:`${s} seconds`;

export function au05Summary(state,{native=true,readError=false}={}){
  if(!native)return {status:'Preview',description:'Vibe Key controls are available in the Windows app.'};
  if(readError)return {status:'Status unavailable',description:'Vibe Key status could not be refreshed.'};
  if(!state||state.enabled!==true)return {status:'Disabled',description:'Vibe Key is disabled.'};
  const battery=state.battery?` · battery ${state.battery.percent} %`:'';
  return {status:STATES[state.state]||'Status unknown',description:`${state.message||''}${battery}`};
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
      container.addEventListener('dragstart',e=>{const g=e.target.closest?.('[data-qm-drag]');if(!g)return;this.dragPath=g.dataset.qmDrag;e.dataTransfer.effectAllowed='move';e.dataTransfer.setData('text/plain',this.dragPath);rowOf(e)?.classList.add('is-dragging');});
      container.addEventListener('dragover',e=>{const r=rowOf(e);if(!r||this.dragPath==null||parent(r.dataset.qmRow)!==parent(this.dragPath))return;e.preventDefault();
        for(const x of container.querySelectorAll('.au05-menu-row.drop-target'))if(x!==r)x.classList.remove('drop-target');r.classList.add('drop-target');});
      container.addEventListener('drop',e=>{const r=rowOf(e);const from=this.dragPath;this.dragPath=null;if(!r||from==null||parent(r.dataset.qmRow)!==parent(from))return;e.preventDefault();
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
    this.selected=zone;this.modeView={};this.turnView=null;this.paint(true);
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
      for(const el of this.container?.querySelectorAll(`.au05-diagram [data-au05-zone="${zone}"]`)||[]){
        el.classList.remove('au05-flash');void el.getBoundingClientRect();el.classList.add('au05-flash');
        setTimeout(()=>el.classList.remove('au05-flash'),700);
      }
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
    const chip=root.querySelector('#au05-page-state'),msg=root.querySelector('#au05-page-message');
    const sum=this.summary();chip.textContent=sum.status;msg.textContent=s.message||sum.description;
    root.querySelector('#au05-battery').textContent=s.battery?`${s.battery.percent} %`:'—';root.querySelector('#au05-battery').parentElement.title=s.battery?`Battery ${s.battery.percent} % (${(s.battery.millivolts/1000).toFixed(2)} V)`:'Battery: not read yet';
    root.querySelector('#au05-events').innerHTML=this.eventsMarkup();
    const controls=root.querySelector('#au05-controls');
    const menuSig=s.menu?{...s.menu,view:undefined}:null;
    const sig=JSON.stringify([s.claude,this.claudeFull,s.turns,s.keys,s.standbyS,s.sleepS,s.light,s.micNr,s.hooksMode,s.standbyStatus,s.controlCodes,s.actions,s.fake,s.enabled,this.sending>0,this.selected,menuSig,s.menuHotkeys,this.modeView,this.turnView,this.menuPick,this.renaming,s.catalogue]);
    // Re-render while a select has focus (so a key job's state updates next to it), but never while text is typed.
    const active=document.activeElement,typing=controls.contains(active)&&active.matches('input');
    if(force||(sig!==this.lastSig&&!typing)){
      const refocus=controls.contains(active)&&active.id?active.id:null;
      this.lastSig=sig;controls.innerHTML=this.controlsMarkup();
      if(refocus)controls.querySelector(`#${CSS.escape(refocus)}`)?.focus();
    }
    this.flash();
  }
  skeleton(){
    return `<div class="page-heading au05-heading"><div><span class="eyebrow">ULANZI AU05</span><h1>Vibe Key</h1></div><div class="au05-head-pills"><span class="au05-pill" title="Battery">${IC.power}<span id="au05-battery"></span></span><span id="au05-page-state" class="chip"></span></div></div>
    <p class="au05-status"><span id="au05-page-message"></span></p>
    <p id="au05-note" class="au05-note${this.noteAttention?' attention':''}" role="status">${esc(this.note)}</p>
    <div id="au05-controls"></div>
    <details class="au05-box au05-events-box"><summary><span class="au05-box-title"><span class="au05-box-icon">${IC.keyboard}</span>Recent events</span></summary><ul id="au05-events" class="au05-events" aria-live="polite"></ul></details>`;
  }
  eventsMarkup(){
    const ev=this.state?.events||[];
    if(!ev.length)return '<li class="au05-muted">No events yet.</li>';
    return ev.slice(0,20).map(e=>`<li><span>${esc(new Date(e.at).toLocaleTimeString('en-AU',{hour12:false}))}</span> <strong>${esc(e.control)}</strong> ${e.pressed?'down':'up'} <span class="hint">${esc(e.code)}</span>${e.action?` <span class="chip">${esc(e.action)}</span>`:''}</li>`).join('');
  }
  // Every setting can be sent; the ones not yet checked on the real key carry an "Untested" label.
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

    // ---- the side panel for one key ----
    const keyEditor=k=>{
      const a=s.actions?.[k.key]||{kind:'none',value:'',app:''};
      const job=k.job;
      const pendingMode=job&&job.state!=='done'&&job.state!=='failed'?(job.label===k.ownCode?'uc':job.label?.startsWith('Factory')?'factory':'keys'):null;
      const effective=pendingMode||k.mode;
      const value=(effective==='uc'||(effective==='unknown'&&a.kind!=='none'))?`uc:${a.kind}`:effective;
      const view=this.modeView[k.key]||(value==='keys'?'keys':value==='factory'?'factory':'uc');
      const seg=(id,label)=>`<button type="button" data-au05-segment="${id}" aria-pressed="${view===id}"${live}>${esc(label)}</button>`;
      const pills=[
        k.checked?'':`<span class="chip attention au05-untested">Untested</span>`,
        value.startsWith('uc:')&&a.kind!=='none'&&k.hotkey?.state==='ready'?pill(`Ready · ${k.ownCode}`,'ok'):'',
        value.startsWith('uc:')&&a.kind!=='none'&&k.hotkey?.state==='failed'?pill(k.hotkey.message||'Not available','bad'):'',
        job?(job.state==='done'?pill('Key updated','ok','au05-job'):job.state==='failed'?`<span class="au05-pill bad au05-job" title="${esc(job.message)}">Failed <button class="au05-link" type="button" data-au05-retry="${k.key}">Try again</button></span>`:pill(job.state==='sending'?'Sending…':'Waiting for key','warn','au05-job')):'',
        !job&&a.kind!=='none'&&k.mode!=='uc'&&k.mode!=='unknown'?`<span class="au05-pill warn">Not set up <button class="au05-link" type="button" data-au05-retry="${k.key}">Set up</button></span>`:'',
      ].join('');
      const ucExtra=`<section class="au05-section"><h3 class="au05-sec"><span class="au05-mark" style="--hue:var(--accent)">${IC.dot}</span><span class="au05-sec-title">Unified Control</span></h3><div class="au05-grid">
        ${card(`data-au05-pick="${k.key}"`,'uc:quickMenu',actionIcon('quickMenu'),'Quick menu',value==='uc:quickMenu',{title:'Open the quick menu'})}
        ${card(`data-au05-pick="${k.key}"`,'uc:hotkey',actionIcon('hotkey'),'Custom',value==='uc:hotkey',{title:'Any key combination'})}
        ${card(`data-au05-pick="${k.key}"`,'uc:none',actionIcon('none'),'Nothing',value==='uc:none',{title:'The key does nothing'})}</div></section>`;
      const custom=value==='uc:hotkey'?`<div class="au05-inline"><input data-au05-action-value="${k.key}" maxlength="60" placeholder="Ctrl + Enter" aria-label="Key combination for ${esc(k.name)}" value="${esc(a.value)}">
        <select data-au05-action-app="${k.key}" aria-label="Send to">${opt('','To the focused app',a.app||'')}${(s.catalogue?.apps||[]).map(x=>opt(x.id,`To ${x.name}${x.verified?'':' (unverified)'}`,a.app||'')).join('')}</select></div>`:'';
      const press=keyOf('knobPress');
      const menuLine=a.kind==='quickMenu'&&press&&press.mode!=='uc'&&k.key!=='knobPress'?'<p class="au05-line attention">Set the knob press to a Unified Control action (Nothing is fine) to choose with it.</p>':'';
      const body=view==='keys'?`<div class="au05-inline"><input data-au05-keys="${k.key}" maxlength="60" placeholder="Ctrl + D" aria-label="Keys ${esc(k.name)} types" value="${esc(k.mode==='keys'?k.keys||'':'')}"></div>`
        :view==='factory'?`<p class="au05-line">${esc(k.name)} types ${esc(k.factory)}, stored on the key.</p>`
        :`${custom}${menuLine}${catalogueSections(`data-au05-pick="${k.key}"`,'uc:',value,ucExtra)}`;
      return `<div class="au05-panel-head"><div><h2>${esc(ZONES[k.key])}</h2>
          <span class="au05-types">Types ${esc(k.label?k.label.replace(/^Factory default \((.*)\)$/,'$1'):'—')} <button class="au05-link" type="button" data-au05-read="${k.key}"${dis}>${k.mode==='unknown'?'Read from key':'Read again'}</button></span></div>
        <div class="au05-pills">${pills}</div></div>
        <div class="au05-seg au05-mode" data-au05-mode="${k.key}" data-value="${esc(value)}" role="group" aria-label="What ${esc(k.name)} does">${seg('uc','UC action')}${seg('keys','Types keys')}${seg('factory','Factory')}</div>
        <p class="au05-panel-note hint${this.noteAttention?' attention':''}" role="status">${esc(this.note)}</p>
        ${body}`;
    };

    // ---- the knob turn panel ----
    const anyMenu=Object.values(s.actions||{}).some(a=>a?.kind==='quickMenu');
    const turnAct=m.knobTurnAction||'scroll';
    const knobEditor=()=>{
      const view=this.turnView||(m.knobTurn==='uc'?'uc':'factory');
      const seg=(id,label)=>`<button type="button" data-au05-turn-mode="${id}" aria-pressed="${view===id}"${live}>${esc(label)}</button>`;
      const dir=t=>{const j=t.job;return `<div class="au05-dir"><span>${t.key==='knobRight'?'Right':'Left'} · ${esc(t.mode==='unknown'?'not read':t.mode==='uc'?t.ownCode:t.mode==='factory'?'factory':t.label)}</span>
        ${t.checked?'':'<span class="chip attention au05-untested">Untested</span>'}
        ${j?(j.state==='done'?pill('Key updated','ok'):j.state==='failed'?pill('Failed','bad'):pill(j.state==='sending'?'Sending…':'Waiting for key','warn')):''}
        <button class="au05-link" type="button" data-au05-read="${t.key}"${dis}>${t.mode==='unknown'?'Read':'Read again'}</button></div>`;};
      const choice=(v,icon,name,title)=>`<button type="button" class="au05-card${m.knobTurn==='uc'&&turnAct===v?' is-selected':''}" data-au05-turn-pick="uc:${v}" aria-pressed="${m.knobTurn==='uc'&&turnAct===v}" title="${esc(title)}"${live}><span class="au05-card-icon">${icon}</span><span class="au05-card-name">${esc(name)}</span></button>`;
      return `<div class="au05-panel-head"><div><h2>Knob turn</h2><span class="au05-types">${m.knobTurn==='uc'?'Types F23 / F24':'Factory scroll'}${anyMenu?' · moves the quick menu':''}</span></div></div>
        <div class="au05-seg" data-au05-turn-seg role="group" aria-label="What turning the knob does">${seg('uc','UC action')}${seg('factory','Factory scroll')}</div>
        <p class="au05-panel-note hint${this.noteAttention?' attention':''}" role="status">${esc(this.note)}</p>
        ${view==='uc'?`<div class="au05-grid">${choice('scroll',actionIcon('scroll'),'Scroll','One notch per click, in the window under the mouse')}${choice('volume',actionIcon('volume'),'Volume','Volume up and down')}${choice('none',actionIcon('none'),'Nothing','Only moves the quick menu')}</div>`:''}
        <div class="au05-dirs">${(s.turns||[]).map(dir).join('')}</div>`;
    };

    // ---- the drawing ----
    const assignment=id=>{
      if(id==='knobTurn'){const act={scroll:'UC scroll',volume:'Volume',none:'Nothing'}[turnAct]||'UC scroll';return m.knobTurn==='uc'?`${act}${anyMenu?' · menu':''}`:`Factory scroll${anyMenu?' · menu':''}`;}
      const k=keyOf(id),a=s.actions?.[id];
      const waiting=k?.job&&(k.job.state==='waiting'||k.job.state==='sending')?' (waiting)':'';
      if(a&&a.kind!=='none'&&(!k||k.mode==='uc'||k.mode==='unknown'||waiting))return actionName(a)+waiting;
      if(!k||k.mode==='unknown')return 'Not read yet';
      if(k.mode==='factory')return `Factory: ${k.factory}`+waiting;
      if(k.mode==='keys')return `Types ${k.keys||''}`+waiting;
      return 'Nothing';
    };
    const sel=id=>id===this.selected?' is-selected':'';
    const zone=(id,shape)=>shape.replace('<ZONE ','<').replace('/>',` data-au05-zone="${id}" class="au05-zone${sel(id)}${id==='knobTurn'?' au05-ring':''}" role="button" tabindex="0" aria-label="${ZONES[id]}: ${esc(assignment(id))}" aria-pressed="${id===this.selected}"/>`);
    const callout=(id,y,anchorX,anchorY)=>`<g class="au05-callout${sel(id)}" data-au05-zone="${id}"><path d="M${anchorX} ${anchorY}H164V${y+17}H176" class="au05-leader"/><rect x="176" y="${y}" width="196" height="34" rx="8"/><text x="186" y="${y+14}" class="au05-name">${esc(ZONES[id])}</text><text x="186" y="${y+28}" class="au05-assign">${esc(clip(assignment(id)))}</text></g>`;
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
    const editor=this.selected==='knobTurn'?knobEditor():keyOf(this.selected)?keyEditor(keyOf(this.selected)):'<p class="au05-line">Wake the key (press any key) to read its controls.</p>';

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
    const menuBox=`<section class="au05-box"><header class="au05-box-head"><div class="au05-box-title"><span class="au05-box-icon">${IC.menu}</span><h2>Quick menu</h2></div>
      <div class="au05-box-actions"><button class="btn" type="button" data-qm-op="open"${live}>Open</button><button class="btn" type="button" data-qm-op="reset"${native}>Reset</button></div></header>
      <ul class="au05-menu-list" aria-label="Quick menu entries">${rows.join('')||'<li class="au05-line">Empty.</li>'}</ul>
      <div class="au05-box-foot"><button class="btn" type="button" data-qm-op="add-top"${native}>${IC.plus} Add entry</button>${m.last?`<span class="au05-muted">Last: ${esc(m.last)}</span>`:''}</div>${picker}</section>`;

    // ---- Claude sessions ----
    const cs=s.claude||{},full=this.claudeFull||{};
    const when=ms=>ms?new Date(ms).toLocaleTimeString('en-AU',{hour12:false,hour:'2-digit',minute:'2-digit'}):'';
    const csState=full.blocked?'bad':cs.connected?(full.hooks&&full.hooks!=='connected'?'warn':'ok'):'off';
    const csText=full.blocked?'Blocked by Claude settings':cs.connected?(full.hooks&&full.hooks!=='connected'?'Hooks missing: connect again':cs.lastEvent?`Connected · last event ${when(cs.lastEvent)}`:'Connected · no event yet'):'Not connected';
    const projects=[...new Set((cs.sessions||[]).map(x=>x.project||'Other'))];
    const sessionChips=(cs.sessions||[]).length?`<div class="au05-session-groups">${projects.map(g=>`<div class="au05-session-group"><span class="au05-muted">${esc(g)}</span>${cs.sessions.filter(x=>(x.project||'Other')===g).map(x=>`<span class="au05-session${x.state==='working'?' working':''}" title="${esc(x.state==='working'?'Working':'Idle')} · ${esc(when(x.last))}">${esc(x.label||'Untitled session')}</span>`).join('')}</div>`).join('')}</div>`:'';
    const claudeBox=`<section class="au05-box au05-claude"><div class="au05-claude-main"><span class="au05-status-dot ${csState}" aria-hidden="true"></span>
      <div class="au05-claude-text"><h2>Claude sessions</h2><span class="au05-muted">${esc(csText)}</span></div>
      ${cs.connected?`<button class="btn au05-big" type="button" data-claude-op="${full.hooks&&full.hooks!=='connected'&&!full.blocked?'connect':'disconnect'}"${native}>${full.hooks&&full.hooks!=='connected'&&!full.blocked?'Connect again':'Disconnect'}</button>`:`<button class="btn primary au05-big" type="button" data-claude-op="connect"${native||full.blocked||full.error?' disabled':''}>Connect</button>`}</div>
      ${full.blocked?`<p class="au05-line attention">${esc(full.blocked)}</p>`:''}${full.error?`<p class="au05-line attention">${esc(full.error)}</p>`:''}
      ${sessionChips}
      <details class="au05-claude-preview"><summary>What Connect adds</summary><p class="au05-muted">${esc(full.settingsPath||'')} · merged and backed up first; the file is reformatted. Disconnect (or uninstall) removes only these entries.</p><pre>${esc(full.preview||'')}</pre></details></section>`;

    // ---- device settings ----
    const standby=s.standbyS;const standbyOpts=STANDBY.some(([v])=>v===standby)||standby==null?STANDBY:[...STANDBY,[standby,minutes(standby)]];
    const sleepOpts=Object.entries(SLEEP).some(([v])=>Number(v)===s.sleepS)||s.sleepS==null?Object.entries(SLEEP):[...Object.entries(SLEEP),[String(s.sleepS),`${s.sleepS} seconds`]];
    const light=s.light||{};const range=s.lightRange||{modeMax:3,brightnessMax:10};
    const fmax=s.lightFieldMax||{type:3,time:15,breatheLevel:15,breatheBrightness:10,alwaysOnBrightness:10};
    const numbers=(max,sel)=>Array.from({length:max+1},(_,i)=>opt(i,String(i),sel)).join('');
    const FIELDS=[['type','Type'],['time','Time'],['breatheLevel','Breathe'],['breatheBrightness','Breathe bright.'],['alwaysOnBrightness','Always-on']];
    const records=(light.records||[]).map((rec,r)=>`<tr><th scope="row">${r}</th>${FIELDS.map(([f,label],i)=>`<td><select data-au05-record="${r}" data-au05-field="${f}" aria-label="Record ${r} ${label}"${dis}>${numbers(Math.max(fmax[f],rec[i]??0),rec[i])}</select></td>`).join('')}</tr>`).join('');
    const codes=(s.controlCodes||[]).map((c,i)=>c==null?'':`<span class="au05-code"><span class="au05-muted">${['K1','K2','K3','Press','Right','Left'][i]}</span> ${esc(c)}</span>`).filter(Boolean).join('');
    const row=(label,untested,control,rawText)=>`<div class="au05-set-row"><div class="au05-set-label">${esc(label)}${untested}${raw(rawText)}</div>${control}</div>`;
    const notRead=v=>v==null?opt('','Not read','') :'';
    const deviceCard=(icon,title,body,wide=false)=>`<section class="au05-box au05-set-card${wide?' au05-wide':''}"><h3 class="au05-box-title"><span class="au05-box-icon">${icon}</span>${esc(title)}</h3>${body}</section>`;
    const device=`<div class="au05-section-head"><h2>Device</h2><div class="au05-box-actions"><button class="btn" data-au05="read-all" type="button">Read all settings</button><button class="btn" data-au05="reconnect" type="button">Reconnect</button></div></div>
      <div class="au05-settings-grid">
      ${deviceCard(IC.power,'Power',
        row('Standby',this.untested('standbyTime'),`<select data-au05-set="standbyTime" aria-label="Standby time"${dis}>${notRead(standby)}${standbyOpts.map(([v,l])=>opt(v,l,standby)).join('')}</select>`,standby==null?null:`${standby} s`)+
        row('Power off',this.untested('sleepTime'),`<select data-au05-set="sleepTime" aria-label="Power-off time"${dis}>${notRead(s.sleepS)}${sleepOpts.map(([v,l])=>opt(v,l,s.sleepS)).join('')}</select>`,s.sleepS==null?null:`${s.sleepS} s`)+
        row('Standby status','',`<span class="au05-value">${s.standbyStatus==null?'—':esc(s.standbyStatus)}</span>`,null))}
      ${deviceCard(IC.light,'Light',
        row('Mode',this.untested('lightMode'),`<select data-au05-set="lightMode" aria-label="Light mode"${dis}>${notRead(light.mode)}${Array.from({length:Math.max(range.modeMax,light.mode??0)+1},(_,i)=>opt(i,`Mode ${i}`,light.mode)).join('')}</select>`,null)+
        row('Brightness',this.untested('lightBrightness'),`<select data-au05-set="lightBrightness" aria-label="All-on brightness"${dis}>${notRead(light.brightness)}${numbers(Math.max(range.brightnessMax,light.brightness??0),light.brightness)}</select>`,null)+
        row('Hooks mode',this.untested('hooksMode'),`<select data-au05-set="hooksMode" aria-label="Hooks mode"${dis}>${notRead(s.hooksMode)}${opt(0,'Off',s.hooksMode)}${opt(1,'On',s.hooksMode)}</select>`,null)+
        `<div class="au05-records"><div class="au05-set-label">Mode records${this.untested('lightRecord')}</div>${records?`<div class="au05-table"><table><thead><tr><th></th>${FIELDS.map(([,l])=>`<th scope="col">${l}</th>`).join('')}</tr></thead><tbody>${records}</tbody></table></div>`:'<p class="au05-muted">Not read yet.</p>'}</div>`,true)}
      ${deviceCard(IC.mic,'Microphone',
        row('Noise reduction',this.untested('micNr'),`<select data-au05-set="micNr" aria-label="Noise reduction"${dis}>${notRead(s.micNr)}${Array.from({length:Math.max(s.micNrMax??3,s.micNr?.level??0)+1},(_,i)=>opt(i,i===0?'Off':`Level ${i}`,s.micNr?.level)).join('')}</select>`,s.micNr?`${s.micNr.level}, ${s.micNr.a}, ${s.micNr.b}`:null))}
      ${deviceCard(IC.sliders,'Controls',`<div class="au05-codes">${codes||'<span class="au05-muted">Read all settings to see each control\'s code.</span>'}</div>`)}
      </div>`;

    return `${topBar}<div class="au05-workspace"><div class="au05-drawing">${diagram}</div>
    <aside class="au05-editor key-editor" aria-label="${esc(ZONES[this.selected])} settings">${editor}</aside></div>
    <div class="au05-boxes">${menuBox}${claudeBox}</div>
    ${device}`;
  }
  async change(change){
    this.sending++;this.setNote('Sending to the Vibe Key…');this.paint(true);
    try{const st=await this.invoke('au05_change',{change});this.state=st;this.setNote('Saved on the Vibe Key.');return true;}
    catch(error){this.setNote(errorText(error),true);return false;}
    finally{this.sending--;this.paint(true);this.onState();}
  }
  // Setting what a key types returns at once; the side panel follows the job ("Waiting for the key", "Done"
  // or the error). Keys and factory default type on their own, so their UC action is cleared.
  async setMode(key,mode,keys){
    const change=mode==='keys'?{setting:'keyTyping',key,mode,keys}:{setting:'keyTyping',key,mode};
    this.lastChange[key]=change;
    try{this.state=await this.invoke('au05_change',{change});this.setNote('');}
    catch(error){this.setNote(errorText(error),true);this.paint(true);return;}
    const a=this.state?.actions?.[key];
    if(mode!=='uc'&&a&&a.kind!=='none')await this.action(key,'none','','');
    this.paint(true);this.onState();
  }
  // Choosing a UC action saves it at once (no device write), then makes the key type its own code.
  async chooseAction(key,kind){
    await this.action(key,kind,kind==='hotkey'?(this.state?.actions?.[key]?.value||''):'',kind==='hotkey'?(this.state?.actions?.[key]?.app||''):'');
    const k=(this.state?.keys||[]).find(x=>x.key===key);
    const job=k?.job,busy=job&&(job.state==='waiting'||job.state==='sending')&&job.label===k.ownCode;
    if(k&&k.mode!=='uc'&&!busy)await this.setMode(key,'uc');
  }
  // The knob turn: Unified Control (the knob types F23/F24; UC scrolls, changes volume or does nothing) or the
  // factory scroll. The choice is saved at once; the two key changes run as background jobs.
  async chooseTurn(value){
    const uc=value.startsWith('uc:');
    await this.configure(uc?{knobTurn:'uc',knobTurnAction:value.slice(3)}:{knobTurn:'factory'});
    const want=uc?'uc':'factory',turns=this.state?.turns||[];
    if(turns.length&&turns.every(t=>t.mode===want&&(!t.job||t.job.state==='done')))return;
    try{this.state=await this.invoke('au05_change',{change:{setting:'knobTurn',mode:want}});this.setNote('');}
    catch(error){this.setNote(errorText(error),true);}
    this.paint(true);this.onState();
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
    const cop=e.type==='click'&&t.closest?.('[data-claude-op]');
    if(cop){this.claudeOp(cop.dataset.claudeOp);return true;}
    const click=sel=>e.type==='click'&&t.closest?.(sel);
    const qmPick=click('[data-qm-pick]');
    if(qmPick){this.menuPicked(qmPick.dataset.value);return true;}
    const cardPick=click('[data-au05-pick]');
    if(cardPick){const key=cardPick.dataset.au05Pick;this.chooseAction(key,cardPick.dataset.value.slice(3));return true;}
    const segment=click('[data-au05-segment]');
    if(segment){const key=segment.closest('[data-au05-mode]').dataset.au05Mode,v=segment.dataset.au05Segment;
      this.modeView[key]=v;
      if(v==='factory')this.setMode(key,'factory');else{this.paint(true);if(v==='keys')this.container.querySelector(`[data-au05-keys="${key}"]`)?.focus();}
      return true;}
    const turnSeg=click('[data-au05-turn-mode]');
    if(turnSeg){this.turnView=turnSeg.dataset.au05TurnMode;if(this.turnView==='factory')this.chooseTurn('factory');else this.paint(true);return true;}
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
      this.invoke('au05_reconnect').then(s=>{this.state=s;this.setNote('Reconnecting. Press a key to wake the Vibe Key.');this.paint(true);}).catch(error=>this.setNote(errorText(error),true));
      return true;
    }
    if(e.type==='click'&&t.closest?.('[data-au05="read-all"]')){
      this.invoke('au05_read_all').then(s=>{this.state=s;this.setNote('Reading all settings, one at a time. Key settings are read per key.');this.paint(true);}).catch(error=>this.setNote(errorText(error),true));
      return true;
    }
    const zone=e.type==='click'&&t.closest?.('[data-au05-zone]');
    if(zone){this.select(zone.dataset.au05Zone);return true;}
    const retry=e.type==='click'&&t.closest?.('[data-au05-retry]');
    if(retry){const key=retry.dataset.au05Retry,last=this.lastChange[key],a=this.state?.actions?.[key];
      if(a&&a.kind!=='none')this.setMode(key,'uc');else if(last)this.setMode(key,last.mode,last.keys);return true;}
    const read=e.type==='click'&&t.closest?.('[data-au05-read]');
    if(read){this.readKey(read.dataset.au05Read);return true;}
    if(e.type!=='change')return false;
    if(t.dataset.qmLabel!==undefined){const path=t.dataset.qmLabel;this.editMenu(en=>{const [l,i]=Au05Panel.locate(en,path);if(l[i])l[i].label=t.value;});return true;}
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
    if(t.dataset.au05Keys){this.setMode(t.dataset.au05Keys,'keys',t.value);return true;}
    if(t.dataset.au05ActionValue){const key=t.dataset.au05ActionValue;this.action(key,'hotkey',t.value,this.state?.actions?.[key]?.app||'');return true;}
    if(t.dataset.au05ActionApp){const key=t.dataset.au05ActionApp;this.action(key,'hotkey',this.state?.actions?.[key]?.value||'',t.value);return true;}
    return false;
  }
}
