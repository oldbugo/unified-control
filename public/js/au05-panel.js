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
const ACTIONS={none:'Nothing (harmless)',claudeDictation:'Start dictation in Claude',chatgptDictation:'Start dictation in ChatGPT (not checked yet)',enter:'Send (Enter)',hotkey:'Custom key combination…'};
const STANDBY=[[60,'1 minute'],[120,'2 minutes'],[300,'5 minutes'],[600,'10 minutes'],[900,'15 minutes'],[1800,'30 minutes'],[3600,'1 hour']];
const SLEEP={3600:'1 hour',7200:'2 hours',10800:'3 hours',14400:'4 hours',0:'Never'};
const errorText=e=>typeof e==='string'?e:e?.error||e?.message||'The Vibe Key request failed';
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
  mount(container){this.container=container;this.paint(true);this.refresh();}
  unmount(){this.container=null;}
  active(container){return this.container===container&&!!container.querySelector('#au05-page');}
  setNote(text,attention=false){this.note=text;this.noteAttention=attention;const el=this.container?.querySelector('#au05-note');if(el){el.textContent=text;el.classList.toggle('attention',attention);}}
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
    const sig=JSON.stringify([s.keys,s.standbyS,s.sleepS,s.light,s.micNr,s.hooksMode,s.standbyStatus,s.controlCodes,s.actions,s.fake,s.enabled,this.sending>0]);
    if(force||(sig!==this.lastSig&&!controls.contains(document.activeElement))){this.lastSig=sig;controls.innerHTML=this.controlsMarkup();}
  }
  skeleton(){
    return `<div class="page-heading"><div><span class="eyebrow">ULANZI AU05</span><h1>Vibe Key</h1><p>Three keys, a knob and a microphone. Changes go to the key as you make them.</p></div><span id="au05-page-state" class="chip"></span></div>
    <section class="settings-list"><div class="setting-row"><div><strong>Connection</strong><p id="au05-page-message"></p></div><button class="btn" data-au05="read-all" type="button">Read all settings</button><button class="btn" data-au05="reconnect" type="button">Reconnect</button></div>
    <div class="setting-row"><div><strong>Battery</strong><p id="au05-battery"></p></div></div></section>
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
    const keyRows=(s.keys||[]).map(k=>{
      const a=s.actions?.[k.key]||{kind:'none',value:''};
      const hot=k.hotkey?.state==='ready'?`<span class="chip accent">Ready · ${esc(k.ownCode)}</span>`:k.hotkey?.state==='failed'?`<span class="chip attention">${esc(k.hotkey.message||'Not available')}</span>`:'';
      const uc=k.mode==='uc'?`<select data-au05-action="${k.key}" aria-label="Unified Control action for ${esc(k.name)}"${dis}>${Object.entries(ACTIONS).map(([v,l])=>opt(v,l,a.kind)).join('')}</select>
        ${a.kind==='hotkey'?`<input data-au05-action-value="${k.key}" maxlength="60" placeholder="Ctrl + Enter" aria-label="Key combination for ${esc(k.name)}" value="${esc(a.value)}"${dis}>`:''}${hot}`:'';
      const read=`<button class="btn" type="button" data-au05-read="${k.key}"${dis}>${k.mode==='unknown'?'Read from key':'Read again'}</button>`;
      return `<div class="setting-row au05-key"><div><strong>${esc(k.name)}</strong>${k.checked?'':' <span class="chip attention">Untested</span>'}<p>Types now: ${esc(k.label||'Not read yet')}</p>${read}</div>
      <div class="au05-key-controls"><select data-au05-mode="${k.key}" aria-label="What ${esc(k.name)} does"${dis}>
      ${k.mode==='unknown'?opt('unknown','Not read yet','unknown'):''}${opt('uc','Unified Control action',k.mode)}${opt('keys','Types keys…',k.mode)}${opt('factory',`Factory default (${k.factory})`,k.mode)}</select>
      <input data-au05-keys="${k.key}" maxlength="60" placeholder="Ctrl + D" aria-label="Keys ${esc(k.name)} types" value="${esc(k.mode==='keys'?k.keys||'':'')}" ${k.mode==='keys'?'':'hidden'}${dis}>${uc}</div></div>`;
    }).join('');
    const standby=s.standbyS;const standbyOpts=STANDBY.some(([v])=>v===standby)||standby==null?STANDBY:[...STANDBY,[standby,minutes(standby)]];
    const sleepOpts=Object.entries(SLEEP).some(([v])=>Number(v)===s.sleepS)||s.sleepS==null?Object.entries(SLEEP):[...Object.entries(SLEEP),[String(s.sleepS),`${s.sleepS} seconds`]];
    const light=s.light||{};const range=s.lightRange||{modeMax:3,brightnessMax:10};
    const fmax=s.lightFieldMax||{type:3,time:15,breatheLevel:15,breatheBrightness:10,alwaysOnBrightness:10};
    const numbers=(max,sel)=>Array.from({length:max+1},(_,i)=>opt(i,String(i),sel)).join('');
    const FIELDS=[['type','Type'],['time','Time'],['breatheLevel','Breathe level'],['breatheBrightness','Breathe brightness'],['alwaysOnBrightness','Always-on brightness']];
    const records=(light.records||[]).map((rec,r)=>`<tr><th scope="row">Record ${r}</th>${FIELDS.map(([f,label],i)=>`<td><select data-au05-record="${r}" data-au05-field="${f}" aria-label="Record ${r} ${label}"${dis}>${numbers(Math.max(fmax[f],rec[i]??0),rec[i])}</select></td>`).join('')}</tr>`).join('');
    const codes=(s.controlCodes||[]).map((c,i)=>c==null?'':`${['K1','K2','K3','Knob press','Knob right','Knob left'][i]} ${esc(c)} (raw ${esc(s.controlCodesRaw?.[i])})`).filter(Boolean).join(' · ');
    return `<div class="settings-section-heading"><h2>Keys</h2><p>Unified Control action: the key types its own unused code (F19 to F22) and Unified Control runs the action you choose, in any app. Types keys and Factory default are stored on the key and also work when Unified Control is closed. The knob turns keep their factory scroll. Key settings are read one key at a time.</p></div>
    <section class="settings-list">${keyRows||'<div class="setting-row"><p class="hint">Wake the key to read its keys.</p></div>'}</section>
    <div class="settings-section-heading"><h2>Power</h2><p>Settings marked Untested were not yet checked on the real key. Each change is read back from the key.</p></div>
    <section class="settings-list"><div class="setting-row"><div><strong>Standby time</strong>${this.untested('standbyTime')}<p>The key sleeps after this much idle time. Press a key to wake it.${raw(standby==null?null:`${standby} s`)}</p></div>
    <select data-au05-set="standbyTime" aria-label="Standby time"${dis}>${standby==null?opt('', 'Not read yet',''):''}${standbyOpts.map(([v,l])=>opt(v,l,standby)).join('')}</select></div>
    <div class="setting-row"><div><strong>Power-off time</strong>${this.untested('sleepTime')}<p>The key turns off after this much idle time.${raw(s.sleepS==null?null:`${s.sleepS} s`)}</p></div>
    <select data-au05-set="sleepTime" aria-label="Power-off time"${dis}>${s.sleepS==null?opt('','Not read yet',''):''}${sleepOpts.map(([v,l])=>opt(v,l,s.sleepS)).join('')}</select></div>
    <div class="setting-row"><div><strong>Standby status</strong><p>Read only.${raw(s.standbyStatus)}</p></div></div></section>
    <div class="settings-section-heading"><h2>Light</h2></div>
    <section class="settings-list"><div class="setting-row"><div><strong>Light mode</strong>${this.untested('lightMode')}<p>Which light record is in use.${raw(light.mode)}</p></div>
    <select data-au05-set="lightMode" aria-label="Light mode"${dis}>${light.mode==null?opt('','Not read yet',''):''}${Array.from({length:Math.max(range.modeMax,light.mode??0)+1},(_,i)=>opt(i,`Mode ${i}`,light.mode)).join('')}</select></div>
    <div class="setting-row"><div><strong>All-on brightness</strong>${this.untested('lightBrightness')}<p>${raw(light.brightness)}</p></div>
    <select data-au05-set="lightBrightness" aria-label="All-on brightness"${dis}>${light.brightness==null?opt('','Not read yet',''):''}${numbers(Math.max(range.brightnessMax,light.brightness??0),light.brightness)}</select></div>
    <div class="setting-row au05-records"><div><strong>Light mode records</strong>${this.untested('lightRecord')}<p>Four records, as the key reports them. The numbers are the raw values.</p>
    ${records?`<div class="au05-table"><table><thead><tr><th></th>${FIELDS.map(([,l])=>`<th scope="col">${l}</th>`).join('')}</tr></thead><tbody>${records}</tbody></table></div>`:'<p class="hint">Not read yet.</p>'}</div></div>
    <div class="setting-row"><div><strong>Hooks mode</strong>${this.untested('hooksMode')}<p>${raw(s.hooksMode)}</p></div>
    <select data-au05-set="hooksMode" aria-label="Hooks mode"${dis}>${s.hooksMode==null?opt('','Not read yet',''):''}${opt(0,'0 (off)',s.hooksMode)}${opt(1,'1 (on)',s.hooksMode)}</select></div></section>
    <div class="settings-section-heading"><h2>Microphone</h2></div>
    <section class="settings-list"><div class="setting-row"><div><strong>Noise reduction</strong>${this.untested('micNr')}<p>The two other values are written back as read.${raw(s.micNr?`level ${s.micNr.level}, ${s.micNr.a}, ${s.micNr.b}`:null)}</p></div>
    <select data-au05-set="micNr" aria-label="Noise reduction"${dis}>${s.micNr==null?opt('','Not read yet',''):''}${Array.from({length:Math.max(s.micNrMax??3,s.micNr?.level??0)+1},(_,i)=>opt(i,i===0?'Off (0)':`Level ${i}`,s.micNr?.level)).join('')}</select></div></section>
    <div class="settings-section-heading"><h2>Controls</h2></div>
    <section class="settings-list"><div class="setting-row"><div><strong>Each control's own code</strong><p>Read only.${codes?` ${codes}`:' Use Read all settings.'}</p></div></div></section>`;
  }
  async change(change){
    this.sending++;this.setNote('Sending to the Vibe Key…');this.paint(true);
    try{const st=await this.invoke('au05_change',{change});this.state=st;this.setNote('Saved on the Vibe Key.');return true;}
    catch(error){this.setNote(errorText(error),true);return false;}
    finally{this.sending--;this.paint(true);this.onState();}
  }
  // Keys and factory default type on their own, so their UC action is cleared (its hotkey is released).
  async setMode(key,mode,keys){
    const ok=await this.change(mode==='keys'?{setting:'keyTyping',key,mode,keys}:{setting:'keyTyping',key,mode});
    const a=this.state?.actions?.[key];
    if(ok&&mode!=='uc'&&a&&a.kind!=='none')await this.action(key,'none','');
  }
  // Key typing is read only on request, one key at a time (the key can take up to 20 s to answer).
  async readKey(key){
    this.sending++;this.setNote('Reading from the Vibe Key… This can take up to 20 seconds.');this.paint(true);
    try{this.state=await this.invoke('au05_read_key',{key});this.setNote('');}
    catch(error){this.setNote(errorText(error),true);}
    finally{this.sending--;this.paint(true);}
  }
  async action(control,kind,value){
    try{this.state=await this.invoke('au05_configure',{patch:{action:{control,kind,value}}});this.setNote('Saved.');}
    catch(error){this.setNote(errorText(error),true);}
    finally{this.paint(true);}
  }
  // Delegated events from the page.
  handle(e){
    const t=e.target;
    if(e.type==='click'&&t.closest?.('[data-au05="reconnect"]')){
      this.invoke('au05_reconnect').then(s=>{this.state=s;this.setNote('Reconnecting. Press a key to wake the Vibe Key.');this.paint(true);}).catch(error=>this.setNote(errorText(error),true));
      return true;
    }
    if(e.type==='click'&&t.closest?.('[data-au05="read-all"]')){
      this.invoke('au05_read_all').then(s=>{this.state=s;this.setNote('Reading all settings, one at a time. Key settings are read per key.');this.paint(true);}).catch(error=>this.setNote(errorText(error),true));
      return true;
    }
    const read=e.type==='click'&&t.closest?.('[data-au05-read]');
    if(read){this.readKey(read.dataset.au05Read);return true;}
    if(e.type!=='change')return false;
    if(t.dataset.au05Set){if(t.value!=='')this.change({setting:t.dataset.au05Set,value:Number(t.value)});return true;}
    if(t.dataset.au05Record){this.change({setting:'lightRecord',record:Number(t.dataset.au05Record),field:t.dataset.au05Field,value:Number(t.value)});return true;}
    if(t.dataset.au05Mode){
      const key=t.dataset.au05Mode,input=this.container.querySelector(`[data-au05-keys="${key}"]`);
      if(t.value==='keys'){input.hidden=false;input.focus();return true;}
      if(t.value==='unknown')return true;
      this.setMode(key,t.value);return true;
    }
    if(t.dataset.au05Keys){this.setMode(t.dataset.au05Keys,'keys',t.value);return true;}
    if(t.dataset.au05Action){this.action(t.dataset.au05Action,t.value,'');return true;}
    if(t.dataset.au05ActionValue){this.action(t.dataset.au05ActionValue,'hotkey',t.value);return true;}
    return false;
  }
}
