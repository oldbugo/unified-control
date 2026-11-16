// Ulanzi AU05 Vibe Key: the General switch, the Home summary and the device page.
// Settings stored on the key are sent as they are changed (no Save button). UC's own settings
// (the switch and a UC action per key) are saved in settings.json at once.
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
      });
    }
    this.paint(true);this.refresh();
  }
  select(zone){
    if(!Object.hasOwn(ZONES,zone))return;
    this.selected=zone;this.paint(true);
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
    root.querySelector('#au05-battery').textContent=s.battery?`${s.battery.percent} % (${(s.battery.millivolts/1000).toFixed(2)} V)`:'Not read yet';
    root.querySelector('#au05-events').innerHTML=this.eventsMarkup();
    const controls=root.querySelector('#au05-controls');
    const menuSig=s.menu?{...s.menu,view:undefined}:null;
    const sig=JSON.stringify([s.turns,s.keys,s.standbyS,s.sleepS,s.light,s.micNr,s.hooksMode,s.standbyStatus,s.controlCodes,s.actions,s.fake,s.enabled,this.sending>0,this.selected,menuSig,s.menuHotkeys]);
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
    return `<div class="page-heading"><div><span class="eyebrow">ULANZI AU05</span><h1>Vibe Key</h1><p>Three keys, a knob and a microphone. Changes go to the key as you make them.</p></div><span id="au05-page-state" class="chip"></span></div>
    <p class="au05-status"><span id="au05-page-message"></span> · Battery <span id="au05-battery"></span></p>
    <p id="au05-note" class="hint${this.noteAttention?' attention':''}" role="status">${esc(this.note)}</p>
    <div id="au05-controls"></div>
    <div class="settings-section-heading"><h2>Live events</h2><p>Key and knob messages as the key sends them. The newest is first.</p></div>
    <section class="settings-list"><ul id="au05-events" class="au05-events" aria-live="polite"></ul></section>
    <div class="settings-section-heading"><h2>Coming later</h2></div>
    <section class="settings-list"><div class="setting-row"><div><strong>Light follows Claude Code</strong><p>The light will show when Claude is working or needs your approval. This is planned and not active.</p></div><span class="chip">Planned</span></div></section>`;
  }
  eventsMarkup(){
    const ev=this.state?.events||[];
    if(!ev.length)return '<li class="hint">No events yet. Press a key or turn the knob.</li>';
    return ev.slice(0,20).map(e=>`<li><span>${esc(new Date(e.at).toLocaleTimeString('en-AU',{hour12:false}))}</span> <strong>${esc(e.control)}</strong> ${e.pressed?'down':'up'} <span class="hint">${esc(e.code)}</span>${e.action?` <span class="chip">${esc(e.action)}</span>`:''}</li>`).join('');
  }
  // Every setting can be sent; the ones not yet checked on the real key carry an "Untested" label.
  untested(name){return this.state?.checked?.[name]===true?'':' <span class="chip attention">Untested</span>';}
  controlsMarkup(){
    const s=this.state||{};const off=!this.native||s.enabled!==true||this.sending>0;
    const dis=off?' disabled':'';
    const opt=(v,label,sel)=>`<option value="${esc(v)}"${String(v)===String(sel)?' selected':''}>${esc(label)}</option>`;
    const raw=v=>v==null?'':` <span class="hint">raw ${esc(v)}</span>`;
    const cat=s.catalogue?.actions||[];
    const actionName=a=>{
      if(!a||a.kind==='none')return 'Nothing';
      if(a.kind==='quickMenu')return 'Open quick menu';
      if(a.kind==='hotkey'){const app=(s.catalogue?.apps||[]).find(x=>x.id===a.app);return a.value?`${a.value}${app?` in ${app.name}`:''}`:'Custom (not set)';}
      const e=cat.find(x=>x.id===a.kind);return e?`${e.group==='Windows / media'?'':`${e.group} `}${e.label}`:a.kind;
    };
    const keyOf=id=>(s.keys||[]).find(k=>k.key===id);
    const keyEditor=k=>{
      const a=s.actions?.[k.key]||{kind:'none',value:'',app:''};
      const job=k.job;
      // The choice shows what the user chose, even while the key change is still waiting.
      const pendingMode=job&&job.state!=='done'&&job.state!=='failed'?(job.label===k.ownCode?'uc':job.label?.startsWith('Factory')?'factory':'keys'):null;
      const effective=pendingMode||k.mode;
      const value=(effective==='uc'||(effective==='unknown'&&a.kind!=='none'))?`uc:${a.kind}`:effective;
      const groups=[...new Set(cat.map(e=>e.group))];
      const options=`${value==='unknown'?opt('unknown','Not read yet: choose to set it','unknown'):''}
        <optgroup label="Unified Control action">${opt('uc:none','Nothing (harmless: the key does nothing)',value)}${opt('uc:quickMenu','Open quick menu',value)}${opt('uc:hotkey','Custom shortcut…',value)}</optgroup>
        ${groups.map(g=>`<optgroup label="${esc(g)}">${cat.filter(e=>e.group===g).map(e=>opt(`uc:${e.id}`,`${e.label}${e.keys&&e.keys!==e.label?` (${e.keys})`:''}${e.status==='verified'?'':' · unverified'}`,value)).join('')}</optgroup>`).join('')}
        <optgroup label="Stored on the key (works without Unified Control)">${opt('keys','Types keys…',value)}${opt('factory',`Factory default (${k.factory})`,value)}</optgroup>`;
      const entry=cat.find(e=>e.id===a.kind);
      const custom=value==='uc:hotkey'?`<div class="au05-key-controls"><input data-au05-action-value="${k.key}" maxlength="60" placeholder="Ctrl + Enter" aria-label="Key combination for ${esc(k.name)}" value="${esc(a.value)}">
        <select data-au05-action-app="${k.key}" aria-label="Send to">${opt('','Send to the focused app',a.app||'')}${(s.catalogue?.apps||[]).map(x=>opt(x.id,`Bring ${x.name} forward first${x.verified?'':' (unverified)'}`,a.app||'')).join('')}</select></div>`:'';
      const hot=value.startsWith('uc:')&&a.kind!=='none'?(k.hotkey?.state==='ready'?`<span class="chip accent">Ready · ${esc(k.ownCode)}</span>`:k.hotkey?.state==='failed'?`<span class="chip attention">${esc(k.hotkey.message||'Not available')}</span>`:''):'';
      const jobLine=job?`<p class="au05-job${job.state==='failed'?' attention':''}" role="status">${job.state==='done'?'Key updated.':esc(job.message)}${job.state==='failed'?` <button class="btn" type="button" data-au05-retry="${k.key}">Try again</button>`:''}</p>`:'';
      // An action without the key typing its own code yet (for example after a restart): offer to finish it.
      const finish=!job&&a.kind!=='none'&&k.mode!=='uc'&&k.mode!=='unknown'?`<p class="attention">The key does not type ${esc(k.ownCode)} yet, so the action cannot run. <button class="btn" type="button" data-au05-retry="${k.key}">Set it up</button></p>`:'';
      // The knob press chooses in the menu only when it types its own code (F19).
      const press=keyOf('knobPress');
      const menuHint=a.kind==='quickMenu'?`<p class="hint">Opens the quick menu. Turn the knob to move, press the knob to choose, press this key again, Esc or wait ${s.menu?.closeAfterS||8} seconds to close.</p>${press&&press.mode!=='uc'&&k.key!=='knobPress'?'<p class="attention">To choose with the knob press, set the knob press to a Unified Control action (Nothing is fine).</p>':''}`:'';
      const read=`<button class="btn" type="button" data-au05-read="${k.key}"${dis}>${k.mode==='unknown'?'Read from key':'Read again'}</button>`;
      return `<div class="au05-editor-head"><h2>${esc(ZONES[k.key])}</h2>${k.checked?'':' <span class="chip attention">Untested</span>'}</div>
      <p>Types now: ${esc(k.label||'Not read yet')}</p>
      <div class="field"><label for="au05-mode-${k.key}">What it does</label><select id="au05-mode-${k.key}" data-au05-mode="${k.key}"${this.native&&s.enabled===true?'':' disabled'}>${options}</select></div>
      <input data-au05-keys="${k.key}" maxlength="60" placeholder="Ctrl + D" aria-label="Keys ${esc(k.name)} types" value="${esc(k.mode==='keys'?k.keys||'':'')}" ${value==='keys'?'':'hidden'}>
      ${custom}${hot}${entry&&entry.status!=='verified'?`<p class="hint">Unverified shortcut: ${esc(entry.note)}. Check it in the app (Ctrl + / shows its shortcut list).</p>`:entry?`<p class="hint">${esc(entry.note)}.</p>`:''}
      ${menuHint}${jobLine}${finish}<p class="au05-panel-note hint${this.noteAttention?' attention':''}" role="status">${esc(this.note)}</p>
      <p class="hint">A Unified Control action: the key types its own unused code (${esc(k.ownCode)}) and Unified Control runs the action in any app. Keys and Factory default are stored on the key.</p>
      ${read}`;
    };
    const m=s.menu||{};
    const anyMenu=Object.values(s.actions||{}).some(a=>a?.kind==='quickMenu');
    const assignment=id=>{
      if(id==='knobTurn'){const act={scroll:'UC scroll',volume:'Volume',none:'Nothing'}[m.knobTurnAction]||'UC scroll';return m.knobTurn==='uc'?`${act}${anyMenu?' · moves the menu':''}`:`Factory scroll${anyMenu?' · moves the menu':''}`;}
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
      <path d="M62 117l4 4M108 117l-4 4M62 163l4-4M108 163l-4-4" class="au05-mark"/>
      ${[['k1',215],['k2',307],['k3',399]].map(([id,y])=>`${zone(id,`<ZONE rect x="37" y="${y}" width="96" height="84" rx="12"/>`)}<circle cx="85" cy="${y+42}" r="25" class="au05-cap"/>`).join('')}
      <rect x="80" y="243" width="10" height="17" rx="5" class="au05-icon"/><path d="M76 255a9 9 0 0 0 18 0M85 264v5" class="au05-icon"/>
      <path d="M76 349l6 6 12-12" class="au05-icon"/>
      <path d="M78 434l14 14M92 434l-14 14" class="au05-icon"/>
      ${callout('knobTurn',78,131,110)}${callout('knobPress',126,118,140)}${callout('k1',240,133,257)}${callout('k2',332,133,349)}${callout('k3',424,133,441)}
    </svg>`;
    const hk=s.menuHotkeys||{};
    const hkText=h=>h?.state==='ready'?'claimed':h?.state==='failed'?esc(h.message||'not available'):'not claimed';
    const turnValue=m.knobTurn==='uc'?`uc:${m.knobTurnAction||'scroll'}`:'factory';
    const turnLine=t=>{const j=t.job;const now=t.mode==='unknown'?'not read yet':t.mode==='uc'?`${t.ownCode} (Unified Control)`:t.mode==='factory'?`factory ${t.key==='knobRight'?'scroll down':'scroll up'}`:t.label;
      return `<div class="setting-row au05-turn-row"><div><strong>${esc(t.name)}</strong>${t.checked?'':' <span class="chip attention">Untested</span>'}<p>Types now: ${esc(now)}${j?` · <span class="${j.state==='failed'?'attention':''}">${j.state==='done'?'Key updated':esc(j.message)}</span>`:''}</p></div>
        <button class="btn" type="button" data-au05-read="${t.key}"${dis}>${t.mode==='unknown'?'Read from key':'Read again'}</button></div>`;};
    const knobEditor=`<div class="au05-editor-head"><h2>Knob turn</h2></div>
      <div class="field"><label for="au05-turn">What turning does</label><select id="au05-turn" data-au05-turn${this.native&&s.enabled===true?'':' disabled'}>
        <optgroup label="Unified Control (the knob types F23 / F24)">${opt('uc:scroll','Scroll (one notch per click)',turnValue)}${opt('uc:volume','Volume up / down',turnValue)}${opt('uc:none','Nothing (only moves the quick menu)',turnValue)}</optgroup>
        <optgroup label="Stored on the key">${opt('factory','Factory scroll',turnValue)}</optgroup></select></div>
      <p class="hint">${m.knobTurn==='uc'?'The knob types its own codes F23 (right) and F24 (left). Unified Control turns them into scrolling or volume, and moves the quick menu while it is open. Nothing scrolls behind the menu.':'Factory scroll: turning right scrolls down, left scrolls up, in the app under the mouse. While the quick menu is open, turning also moves its highlight (the window behind may scroll too).'}</p>
      ${m.knobTurn==='uc'?`<p class="hint">F23: ${hkText(hk.knobRight)} · F24: ${hkText(hk.knobLeft)}</p>`:''}
      <section class="settings-list">${(s.turns||[]).map(turnLine).join('')}</section>`;
    const editor=this.selected==='knobTurn'
      ?knobEditor
      :keyOf(this.selected)?keyEditor(keyOf(this.selected)):'<p class="hint">Wake the key to read its controls.</p>';
    const standby=s.standbyS;const standbyOpts=STANDBY.some(([v])=>v===standby)||standby==null?STANDBY:[...STANDBY,[standby,minutes(standby)]];
    const sleepOpts=Object.entries(SLEEP).some(([v])=>Number(v)===s.sleepS)||s.sleepS==null?Object.entries(SLEEP):[...Object.entries(SLEEP),[String(s.sleepS),`${s.sleepS} seconds`]];
    const light=s.light||{};const range=s.lightRange||{modeMax:3,brightnessMax:10};
    const fmax=s.lightFieldMax||{type:3,time:15,breatheLevel:15,breatheBrightness:10,alwaysOnBrightness:10};
    const numbers=(max,sel)=>Array.from({length:max+1},(_,i)=>opt(i,String(i),sel)).join('');
    const FIELDS=[['type','Type'],['time','Time'],['breatheLevel','Breathe level'],['breatheBrightness','Breathe brightness'],['alwaysOnBrightness','Always-on brightness']];
    const records=(light.records||[]).map((rec,r)=>`<tr><th scope="row">Record ${r}</th>${FIELDS.map(([f,label],i)=>`<td><select data-au05-record="${r}" data-au05-field="${f}" aria-label="Record ${r} ${label}"${dis}>${numbers(Math.max(fmax[f],rec[i]??0),rec[i])}</select></td>`).join('')}</tr>`).join('');
    const codes=(s.controlCodes||[]).map((c,i)=>c==null?'':`${['K1','K2','K3','Knob press','Knob right','Knob left'][i]} ${esc(c)} (raw ${esc(s.controlCodesRaw?.[i])})`).filter(Boolean).join(' · ');
    const native=this.native?'':' disabled';
    const profiles=m.profiles||[],current=profiles.find(p=>p.id===m.profile);
    const profileBar=`<div class="au05-profile-bar">
      <div class="field"><label for="au05-profile">Profile</label><select id="au05-profile" data-au05-profile${native}>${profiles.map(p=>opt(p.id,p.name,m.profile)).join('')}</select></div>
      <div class="field"><label for="au05-target">Target app</label><select id="au05-target" data-au05-target${native}>${(m.targets||[]).map(t=>opt(t.id,t.name,m.target)).join('')}</select></div>
      <div class="field"><label for="au05-profile-name">Profile name</label><input id="au05-profile-name" data-au05-profile-name maxlength="60" value="${esc(current?.name||'')}"${native}></div>
      <div class="au05-profile-tools"><button class="btn" type="button" data-au05-profile-op="add"${native}>New profile</button><button class="btn" type="button" data-au05-profile-op="remove"${native||profiles.length<2?' disabled':''}>Remove profile</button></div></div>
      <p class="hint">A profile is a set of actions for K1, K2, K3 and the knob press; switching applies at once and writes nothing to the key. The target app decides what "Target app" actions do.</p>`;
    const groups=[...new Set(cat.map(e=>e.group))];
    const whatValue=e=>e.type==='action'?`a:${e.action?.kind}`:e.type==='profile'?`p:${e.profile}`:e.type==='target'?`t:${e.target}`:e.type==='submenu'?'submenu':'session';
    const whatOptions=e=>{const v=whatValue(e);
      return `${e.type==='action'&&e.action?.kind==='hotkey'?opt(v,`Custom shortcut ${e.action.value}`,v):''}${groups.map(g=>`<optgroup label="${esc(g)}">${cat.filter(x=>x.group===g).map(x=>opt(`a:${x.id}`,`${x.group}: ${x.label}${x.status==='verified'?'':' · unverified'}`,v)).join('')}</optgroup>`).join('')}
        <optgroup label="Switch profile">${profiles.map(p=>opt(`p:${p.id}`,`Profile: ${p.name}`,v)).join('')}</optgroup>
        <optgroup label="Target app">${(m.targets||[]).map(t=>opt(`t:${t.id}`,`Target app: ${t.name}`,v)).join('')}</optgroup>
        <optgroup label="Other">${opt('submenu','Sub-menu',v)}${opt('session','Open a Claude session (not built yet)',v)}</optgroup>`;};
    const menuRows=[];
    const walk=(list,path,depth)=>list.forEach((e,i)=>{
      const p=[...path,i].join('.');
      menuRows.push(`<li class="au05-menu-row" style="--depth:${depth}" data-qm-row="${p}">
        <input id="qm-label-${p}" data-qm-label="${p}" maxlength="60" value="${esc(e.label)}" aria-label="Name of menu entry ${esc(e.label)}"${native}>
        <select id="qm-what-${p}" data-qm-what="${p}" aria-label="What ${esc(e.label)} does"${native}>${whatOptions(e)}</select>
        <span class="au05-menu-tools"><button class="btn" type="button" data-qm-op="up" data-qm-path="${p}" aria-label="Move ${esc(e.label)} up"${i===0?' disabled':native}>↑</button><button class="btn" type="button" data-qm-op="down" data-qm-path="${p}" aria-label="Move ${esc(e.label)} down"${i===list.length-1?' disabled':native}>↓</button><button class="btn" type="button" data-qm-op="remove" data-qm-path="${p}" aria-label="Remove ${esc(e.label)}"${native}>Remove</button>${e.type==='submenu'&&depth<1?`<button class="btn" type="button" data-qm-op="add" data-qm-path="${p}"${native}>Add inside</button>`:''}</span></li>`);
      if(e.type==='submenu')walk(e.entries||[],[...path,i],depth+1);
    });
    walk(m.entries||[],[],0);
    const menuSection=`<div class="settings-section-heading au05-device-heading"><div><h2>Quick menu</h2><p>Give a key the action "Open quick menu". Turn the knob to move, press the knob to choose; press the key again, Esc or wait ${m.closeAfterS||8} seconds to close. The menu appears near the top-right corner of the main screen; drag it by its title to move it, and it opens there next time.</p></div><div class="au05-device-actions"><button class="btn" type="button" data-qm-op="open"${this.native&&s.enabled===true?'':' disabled'}>Open the menu</button><button class="btn" type="button" data-qm-op="reset"${native}>Reset to the default menu</button></div></div>
      <section class="settings-list"><ul class="au05-menu-list" aria-label="Quick menu entries">${menuRows.join('')||'<li class="hint">The menu is empty.</li>'}</ul>
      <div class="setting-row"><button class="btn" type="button" data-qm-op="add-top"${native}>Add entry</button>${m.last?`<span class="hint">Last choice: ${esc(m.last)}</span>`:''}</div></section>`;
    return `${profileBar}<div class="au05-workspace"><div class="au05-drawing">${diagram}<p class="hint">Choose a key or the knob. A control lights up when you press or turn it.</p></div>
    <aside class="au05-editor key-editor" aria-label="${esc(ZONES[this.selected])} settings">${editor}</aside></div>
    ${menuSection}
    <div class="settings-section-heading au05-device-heading"><div><h2>Device settings</h2><p>Settings marked Untested were not yet checked on the real key. Each change is read back from the key.</p></div><div class="au05-device-actions"><button class="btn" data-au05="read-all" type="button">Read all settings</button><button class="btn" data-au05="reconnect" type="button">Reconnect</button></div></div>
    <div class="au05-settings-grid"><div><h3 class="au05-sub">Power</h3>
    <section class="settings-list"><div class="setting-row"><div><strong>Standby time</strong>${this.untested('standbyTime')}<p>The key sleeps after this much idle time. Press a key to wake it.${raw(standby==null?null:`${standby} s`)}</p></div>
    <select data-au05-set="standbyTime" aria-label="Standby time"${dis}>${standby==null?opt('', 'Not read yet',''):''}${standbyOpts.map(([v,l])=>opt(v,l,standby)).join('')}</select></div>
    <div class="setting-row"><div><strong>Power-off time</strong>${this.untested('sleepTime')}<p>The key turns off after this much idle time.${raw(s.sleepS==null?null:`${s.sleepS} s`)}</p></div>
    <select data-au05-set="sleepTime" aria-label="Power-off time"${dis}>${s.sleepS==null?opt('','Not read yet',''):''}${sleepOpts.map(([v,l])=>opt(v,l,s.sleepS)).join('')}</select></div>
    <div class="setting-row"><div><strong>Standby status</strong><p>Read only.${raw(s.standbyStatus)}</p></div></div></section>
    <h3 class="au05-sub">Microphone</h3>
    <section class="settings-list"><div class="setting-row"><div><strong>Noise reduction</strong>${this.untested('micNr')}<p>The two other values are written back as read.${raw(s.micNr?`level ${s.micNr.level}, ${s.micNr.a}, ${s.micNr.b}`:null)}</p></div>
    <select data-au05-set="micNr" aria-label="Noise reduction"${dis}>${s.micNr==null?opt('','Not read yet',''):''}${Array.from({length:Math.max(s.micNrMax??3,s.micNr?.level??0)+1},(_,i)=>opt(i,i===0?'Off (0)':`Level ${i}`,s.micNr?.level)).join('')}</select></div></section>
    <h3 class="au05-sub">Controls</h3>
    <section class="settings-list"><div class="setting-row"><div><strong>Each control's own code</strong><p>Read only.${codes?` ${codes}`:' Use Read all settings.'}</p></div></div></section></div>
    <div><h3 class="au05-sub">Light</h3>
    <section class="settings-list"><div class="setting-row"><div><strong>Light mode</strong>${this.untested('lightMode')}<p>Which light record is in use.${raw(light.mode)}</p></div>
    <select data-au05-set="lightMode" aria-label="Light mode"${dis}>${light.mode==null?opt('','Not read yet',''):''}${Array.from({length:Math.max(range.modeMax,light.mode??0)+1},(_,i)=>opt(i,`Mode ${i}`,light.mode)).join('')}</select></div>
    <div class="setting-row"><div><strong>All-on brightness</strong>${this.untested('lightBrightness')}<p>${raw(light.brightness)}</p></div>
    <select data-au05-set="lightBrightness" aria-label="All-on brightness"${dis}>${light.brightness==null?opt('','Not read yet',''):''}${numbers(Math.max(range.brightnessMax,light.brightness??0),light.brightness)}</select></div>
    <div class="setting-row au05-records"><div><strong>Light mode records</strong>${this.untested('lightRecord')}<p>Four records, as the key reports them. The numbers are the raw values.</p>
    ${records?`<div class="au05-table"><table><thead><tr><th></th>${FIELDS.map(([,l])=>`<th scope="col">${l}</th>`).join('')}</tr></thead><tbody>${records}</tbody></table></div>`:'<p class="hint">Not read yet.</p>'}</div></div>
    <div class="setting-row"><div><strong>Hooks mode</strong>${this.untested('hooksMode')}<p>${raw(s.hooksMode)}</p></div>
    <select data-au05-set="hooksMode" aria-label="Hooks mode"${dis}>${s.hooksMode==null?opt('','Not read yet',''):''}${opt(0,'0 (off)',s.hooksMode)}${opt(1,'1 (on)',s.hooksMode)}</select></div></section></div></div>`;
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
    return {type:'claudeSession',label:'Claude session',session:''};
  }
  menuOp(op,path){
    if(op==='open'){this.invoke('au05_menu',{input:{op:'toggle'}}).catch(error=>this.setNote(errorText(error),true));return;}
    if(op==='reset'){this.configure({menu:'default'});return;}
    const fresh=()=>this.entryFor('a:claudeDictation');
    this.editMenu(entries=>{
      if(op==='add-top'){entries.push(fresh());return;}
      const [list,i]=Au05Panel.locate(entries,path);
      if(op==='up'&&i>0)[list[i-1],list[i]]=[list[i],list[i-1]];
      if(op==='down'&&i<list.length-1)[list[i+1],list[i]]=[list[i],list[i+1]];
      if(op==='remove')list.splice(i,1);
      if(op==='add'&&list[i]?.type==='submenu')(list[i].entries||=[]).push(fresh());
    });
  }
  // Delegated events from the page.
  handle(e){
    const t=e.target;
    const qm=e.type==='click'&&t.closest?.('[data-qm-op]');
    if(qm){this.menuOp(qm.dataset.qmOp,qm.dataset.qmPath);return true;}
    const pop=e.type==='click'&&t.closest?.('[data-au05-profile-op]');
    if(pop){
      const m=this.state?.menu||{};
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
    if(t.dataset.qmWhat!==undefined){const path=t.dataset.qmWhat;this.editMenu(en=>{const [l,i]=Au05Panel.locate(en,path);if(l[i])l[i]=this.entryFor(t.value,l[i]);});return true;}
    if(t.dataset.au05Profile!==undefined){this.configure({profile:t.value});return true;}
    if(t.dataset.au05Target!==undefined){this.configure({target:t.value});return true;}
    if(t.dataset.au05ProfileName!==undefined){this.configure({profileEdit:{op:'rename',id:this.state?.menu?.profile,name:t.value}});return true;}
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
