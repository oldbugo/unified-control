// Input tester page: keyboard drawing, last combination, event log, devices, microphone and speaker tests.
// In the app, events come from the host's Raw Input listener (core/input_tester.rs), registered only while this page
// is open; each event names the device that sent it. In a browser preview only this window's keys are seen.
import {layout,EXTRA_ROW,MOUSE_KEYS,MEDIA_KEYS,VIBE_KEY,InputModel,drawCode,drawnCodes,logColumns,LOG_HEADER,toTsv,domEvent,dbfs,encodeWav,toBase64,hex} from './input-tester-model.js';
import {WebAudio,FakeAudio} from './input-tester-audio.js';

const esc=s=>String(s??'').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
const errorText=e=>e?.message||String(e||'Something went wrong');
const SHOWN=300,KEEP=4000,POLL_MS=80;
const PREFS='uc.input-tester.v1';
const DRAWN=drawnCodes();
export const INPUT_TESTER_ICON='<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="2.5" y="6" width="19" height="12" rx="2"/><path d="M6 10h1m3 0h1m3 0h1m3 0h1M6 14h1m3 0h4m3 0h1"/></svg>';
const KIND_ORDER={keyboard:0,mouse:1,hid:2,browser:3};

function loadPrefs(){try{return {iso:false,numpad:true,...JSON.parse(localStorage.getItem(PREFS)||'{}')};}catch{return {iso:false,numpad:true};}}
function savePrefs(p){try{localStorage.setItem(PREFS,JSON.stringify(p));}catch{}}

export class InputTesterPanel{
  /** `fake()` says whether this is a fake or test instance (simulated audio, no microphone). */
  constructor(invoke,native,fake=()=>false){
    this.invoke=invoke;this.native=native;this.fake=fake;
    this.container=null;this.model=new InputModel();this.prefs=loadPrefs();
    this.log=[];this.paused=false;this.pausedCount=0;this.filter='';this.logMoves=false;
    this.devices=[];this.counts={};this.now=0;this.claimed=[];this.mode='';this.error='';
    this.seq=0;this.timer=null;this.epoch=0;this.dirtyDevices=false;
    this.audio=null;this.mic={state:'off',format:null,error:'',peak:0,hold:0,holdAt:0,clips:0};this.rec=null;this.recState='';this.note='';
    this.inputs=[];this.outputs=[];this.micId='default';this.outId='default';this.processing=false;
  }
  markup(){return '<div id="it-page" class="it-page" aria-label="Input tester"></div>';}
  active(container){return this.container===container&&!!container.querySelector('#it-page');}
  mount(container){
    this.unmount();
    this.container=container;const epoch=++this.epoch;
    // Each visit is a new listening session (the host starts one too).
    this.model.clear();this.log=[];this.counts={};this.devices=[];this._devs='';this.seq=0;this.lastShown=null;this.paused=false;this.pausedCount=0;this.filter='';this.mode='';this.error='';
    this.root=container.querySelector('#it-page');
    this.root.innerHTML=this.skeleton();
    this.drawBoard();this.paintDevices();this.paintCombo();this.paintLog();this.paintAudio();
    this.listen();
    if(this.native)this.start(epoch,true);else{this.mode='browser';this.paintMode();}
    this.loadAudioDevices();
    this.meter=setInterval(()=>this.paintMeter(),60);
    this.activity=setInterval(()=>{this.now+=250;this.paintActivity();},250);
    const surface=this.root.querySelector('#it-surface');
    if(document.activeElement===document.body||document.activeElement?.closest?.('.nav'))surface?.focus({preventScroll:true});
  }
  unmount(){
    if(!this.container)return;
    this.epoch++;
    clearTimeout(this.timer);clearInterval(this.meter);clearInterval(this.activity);this.timer=null;
    for(const [target,type,fn,opts] of this.listeners||[])target.removeEventListener(type,fn,opts);
    this.listeners=[];
    if(this.native)this.invoke('input_tester_stop').catch(()=>{});
    // Leaving the page closes the microphone and forgets the recording.
    this.audio?.dispose();this.audio=null;this.rec=null;this.recState='';
    this.mic={state:'off',format:null,error:'',peak:0,hold:0,holdAt:0,clips:0};
    this.container=null;this.root=null;
  }

  // ---------- events in ----------
  async start(epoch,fresh=false){
    try{
      const v=await this.invoke('input_tester_start',{fresh});
      if(epoch!==this.epoch)return;
      // The window is hidden (closed to the tray) or minimised: the host does not listen. Try again later.
      if(v.hidden){this.mode='hidden';this.paintMode();clearTimeout(this.timer);this.timer=setTimeout(()=>this.start(epoch),1000);return;}
      this.seq=0;this.model.clear();this.log=[];this.lastShown=null;this.paintKeys();this.paintCombo();this.paintLog();
      this.accept(v);
    }catch(error){if(epoch===this.epoch){this.error=errorText(error);this.paintMode();}}
    this.schedule(epoch);
  }
  schedule(epoch){
    clearTimeout(this.timer);
    this.timer=setTimeout(async()=>{
      if(epoch!==this.epoch)return;
      try{
        const v=await this.invoke('input_tester_poll',{since:this.seq});
        if(epoch!==this.epoch)return;
        // The host stopped listening (for example after the window was closed to the tray): start again.
        if(!v.running){await this.start(epoch);return;}
        this.accept(v);
      }catch(error){if(epoch===this.epoch){this.error=errorText(error);this.paintMode();}}
      this.schedule(epoch);
    },POLL_MS);
  }
  accept(v){
    if(!v?.running)return;
    if(v.mode!==this.mode||(v.error||'')!==this.error){this.mode=v.mode;this.error=v.error||'';this.paintMode();}
    this.now=v.now;this.claimed=v.claimed||[];
    const devs=JSON.stringify(v.devices);
    if(devs!==this._devs){this._devs=devs;this.devices=v.devices||[];this.dirtyDevices=true;this.paintFilterOptions();}
    this.counts=v.counts||{};
    this.take(v.events||[]);
    if(v.seq!=null)this.seq=Math.max(this.seq,v.seq);
    this.paintDevices();this.paintVibe();
  }
  /** New events, oldest first. */
  take(events){
    if(!events.length)return;
    let draw=false;const shown=[];
    for(const e of events){
      if(this.model.apply(e))draw=true;
      if(e.kind==='device')this.dirtyDevices=true;
      if(this.paused){this.pausedCount++;continue;}
      this.log.push(e);shown.push(e);
    }
    if(this.log.length>KEEP)this.log.splice(0,this.log.length-KEEP);
    if(draw){this.paintKeys();this.paintCombo();}
    if(shown.length)this.appendLog(shown);
    if(this.paused)this.paintPause();
  }
  /** Browser preview: this window's DOM events. */
  domInput(ev){
    const e=domEvent(ev,performance.now()-(this.t0??=performance.now()),this.model.heldCodes());
    if(!e)return;
    e.t=Math.round(e.t*10)/10;this.now=e.t;
    if(!this.devices.length){this.devices=[{id:'browser',kind:'browser',name:'This window',role:'Browser events',vid:null,pid:null,simulated:true}];this.paintFilterOptions();}
    e.seq=++this.seq;
    this.take([e]);this.counts=this.model.counts;this.dirtyDevices=true;this.paintDevices();this.paintVibe();
  }

  // ---------- user input ----------
  on(target,type,fn,opts){target.addEventListener(type,fn,opts);(this.listeners??=[]).push([target,type,fn,opts]);}
  listen(){
    const root=this.root,surface=root.querySelector('#it-surface');
    // Keys typed on the test area stay there: no button clicks, scrolling, F11 or browser shortcuts.
    const keyHandler=ev=>{
      if(!surface.contains(ev.target))return;
      ev.preventDefault();ev.stopPropagation();
      if(!this.native)this.domInput(ev);
    };
    this.on(surface,'keydown',keyHandler);this.on(surface,'keyup',keyHandler);
    this.on(surface,'contextmenu',ev=>ev.preventDefault());
    this.on(surface,'wheel',ev=>{ev.preventDefault();if(!this.native)this.domInput(ev);},{passive:false});
    this.on(surface,'mousedown',ev=>{if(ev.button>0)ev.preventDefault();surface.focus({preventScroll:true});if(!this.native)this.domInput(ev);});
    this.on(surface,'mouseup',ev=>{if(!this.native)this.domInput(ev);});
    // Back and forward mouse buttons must not move the app's page history while testing.
    const xbutton=ev=>{if(ev.button===3||ev.button===4)ev.preventDefault();};
    this.on(window,'mouseup',xbutton,true);this.on(window,'mousedown',xbutton,true);
    this.on(root,'click',ev=>{if(this.click(ev))ev.stopPropagation();});
    this.on(root,'change',ev=>{if(this.change(ev))ev.stopPropagation();});
  }
  click(ev){
    const t=ev.target.closest('[data-it]');if(!t||!this.root.contains(t))return false;
    const what=t.dataset.it;
    if(what==='clear'){this.clear();return true;}
    if(what==='pause'){this.paused=!this.paused;this.pausedCount=0;t.textContent=this.paused?'Resume log':'Pause log';t.setAttribute('aria-pressed',String(this.paused));this.paintPause();return true;}
    if(what==='copy'){this.copy();return true;}
    if(what==='filter'){this.setFilter(this.filter===t.dataset.dev?'':t.dataset.dev);return true;}
    if(what==='numpad'){this.prefs.numpad=!this.prefs.numpad;savePrefs(this.prefs);t.setAttribute('aria-pressed',String(this.prefs.numpad));this.drawBoard();return true;}
    if(what==='mic'){this.toggleMic();return true;}
    if(what==='record'){this.record();return true;}
    if(what==='play'){this.playBack();return true;}
    if(what==='save'){this.save();return true;}
    if(what==='discard'){this.rec=null;this.recState='';this.note='Recording discarded.';this.paintAudio();return true;}
    if(what==='tone'){this.tone(t.dataset.side);return true;}
    return false;
  }
  change(ev){
    const id=ev.target.id;
    if(id==='it-layout'){this.prefs.iso=ev.target.value==='iso';savePrefs(this.prefs);this.drawBoard();return true;}
    if(id==='it-filter'){this.setFilter(ev.target.value);return true;}
    if(id==='it-moves'){
      this.logMoves=ev.target.checked;
      if(this.native)this.invoke('input_tester_configure',{patch:{logMoves:this.logMoves}}).catch(()=>{});
      return true;
    }
    if(id==='it-mic-select'){this.micId=ev.target.value;if(this.audio?.isOpen)this.openMic();return true;}
    if(id==='it-processing'){this.processing=ev.target.checked;if(this.audio?.isOpen)this.openMic();return true;}
    if(id==='it-out-select'){this.outId=ev.target.value;return true;}
    return false;
  }
  clear(){
    this.model.clear();this.log=[];this.pausedCount=0;this.lastShown=null;
    if(this.native)this.invoke('input_tester_configure',{patch:{clear:true}}).then(v=>{if(v?.running){this.counts=v.counts||{};this.paintDevices();}}).catch(()=>{});
    else{this.counts=this.model.counts;}
    this.dirtyDevices=true;
    this.paintKeys();this.paintCombo();this.paintLog();this.paintDevices();this.paintVibe();this.paintPause();
  }
  setFilter(id){
    this.filter=id;const sel=this.root.querySelector('#it-filter');if(sel)sel.value=id;
    this.dirtyDevices=true;this.paintDevices();this.paintLog();
  }
  async copy(){
    const text=toTsv(this.visible(),id=>this.deviceName(id));
    let ok=false;
    try{await navigator.clipboard.writeText(text);ok=true;}catch{
      const ta=document.createElement('textarea');ta.value=text;ta.setAttribute('readonly','');ta.style.position='fixed';ta.style.opacity='0';
      this.root.append(ta);ta.select();try{ok=document.execCommand('copy');}catch{}ta.remove();
    }
    this.setLogNote(ok?`Copied ${this.visible().length} events.`:'The log could not be copied.');
  }
  visible(){return this.filter?this.log.filter(e=>e.device===this.filter):this.log;}
  deviceName(id){
    if(id==='injected')return 'Software (no device)';
    const d=this.devices.find(d=>d.id===id);
    return d?d.name:id==='browser'?'This window':'Unknown device';
  }

  // ---------- painting ----------
  skeleton(){
    const btn=(it,label,extra='')=>`<button class="btn" type="button" data-it="${it}" ${extra}>${label}</button>`;
    return `
    <div class="page-heading it-heading"><div><span class="eyebrow">Test your inputs</span><h1>Input tester</h1>
      <p>Press keys, buttons and knobs on any keyboard, mouse or Vibe Key. Each event shows which device sent it. Nothing is sent to other apps by this page.</p></div>
      <span class="chip" id="it-mode" role="status"></span></div>
    <section class="it-card it-board-card" aria-labelledby="it-board-title">
      <div class="it-card-head"><h2 id="it-board-title">Keyboard</h2>
        <div class="it-combo" aria-live="polite"><span class="it-combo-label">Last combination</span><span id="it-combo" class="it-combo-keys"></span></div>
        <span class="it-spacer"></span>
        <label class="it-inline"><span class="sr-only">Keyboard layout</span><select id="it-layout" aria-label="Keyboard layout"><option value="ansi"${this.prefs.iso?'':' selected'}>ANSI (US)</option><option value="iso"${this.prefs.iso?' selected':''}>ISO (UK, EU)</option></select></label>
        ${btn('numpad','Number pad',`aria-pressed="${this.prefs.numpad}"`)}
        ${btn('clear','Clear')}
      </div>
      <div id="it-surface" class="it-surface" tabindex="0" role="application" aria-label="Test area: keys, clicks and wheel turns here are captured. Click outside to leave.">
        <div class="it-extra"><span class="it-row-label">F13–F24</span><div id="it-extra" class="it-keyrow"></div></div>
        <div id="it-board" class="it-board"></div>
        <div class="it-strips">
          <div class="it-strip"><span class="it-row-label">Mouse</span><div id="it-mouse" class="it-keyrow"></div></div>
          <div class="it-strip"><span class="it-row-label">Media</span><div id="it-media" class="it-keyrow"></div></div>
          <div class="it-strip" id="it-other-wrap" hidden><span class="it-row-label">Other</span><div id="it-other" class="it-keyrow"></div></div>
        </div>
        <p class="it-hint">Lit: held now. Outlined: pressed since Clear. Click here first so typing does not press buttons on this page.</p>
      </div>
    </section>
    <div class="it-grid">
      <section class="it-card" aria-labelledby="it-vibe-title">
        <div class="it-card-head"><h2 id="it-vibe-title">Vibe Key</h2></div>
        <p class="it-small">When a Vibe Key control is set to a Unified Control action, it types its own code. Unified Control claims that code as a hotkey, so other apps never get it.</p>
        <div id="it-vibe" class="it-vibe"></div>
      </section>
      <section class="it-card" aria-labelledby="it-dev-title">
        <div class="it-card-head"><h2 id="it-dev-title">Devices</h2><span class="it-spacer"></span><span class="it-small">Select a device to show only its events</span></div>
        <ul id="it-devices" class="it-devices"></ul>
      </section>
    </div>
    <section class="it-card it-log-card" aria-labelledby="it-log-title">
      <div class="it-card-head"><h2 id="it-log-title">Event log</h2><span class="it-small">Newest first</span><span class="it-spacer"></span>
        <label class="it-inline"><span class="sr-only">Device</span><select id="it-filter" aria-label="Show events from"><option value="">All devices</option></select></label>
        <label class="it-check"><input id="it-moves" type="checkbox"${this.logMoves?' checked':''}> Mouse moves</label>
        ${btn('pause','Pause log','aria-pressed="false"')}${btn('copy','Copy log')}</div>
      <p id="it-log-note" class="it-small it-log-note" role="status"></p>
      <div class="it-log-wrap"><table class="it-log"><thead><tr>${LOG_HEADER.map(h=>`<th scope="col">${esc(h)}</th>`).join('')}</tr></thead><tbody id="it-log-body"></tbody></table>
      <p id="it-log-empty" class="it-empty">No events yet. Press a key, click or turn a knob.</p></div>
    </section>
    <div class="it-grid">
      <section class="it-card" aria-labelledby="it-mic-title" id="it-mic-card"></section>
      <section class="it-card" aria-labelledby="it-out-title" id="it-out-card"></section>
    </div>`;
  }
  keyHtml(k){return `<span class="it-key" data-it-code="${esc(k.code)}" style="grid-column:${k.x+1} / span ${k.w};grid-row:${k.y+1} / span ${k.h}">${esc(k.label)}</span>`;}
  drawBoard(){
    const board=this.root?.querySelector('#it-board');if(!board)return;
    board.innerHTML=layout(this.prefs).map(b=>`<div class="it-block it-block-${b.id}" style="--cols:${b.cols};--rows:${b.rows}">${b.keys.map(k=>this.keyHtml(k)).join('')}</div>`).join('');
    const row=(id,keys)=>{const el=this.root.querySelector(id);if(el)el.innerHTML=keys.map(([code,label])=>`<span class="it-key it-flat" data-it-code="${esc(code)}">${esc(label)}</span>`).join('');};
    row('#it-extra',EXTRA_ROW[0]);row('#it-mouse',MOUSE_KEYS);row('#it-media',MEDIA_KEYS);
    this.paintKeys();
  }
  paintKeys(){
    if(!this.root)return;
    const held=this.model.heldCodes(),was=this.model.pressed;
    for(const el of this.root.querySelectorAll('[data-it-code]')){
      const c=el.dataset.itCode;
      el.classList.toggle('held',held.has(c));el.classList.toggle('was',was.has(c)&&!held.has(c));
    }
    // Keys the drawing does not show get a chip of their own.
    const other=[...was].filter(c=>!DRAWN.has(c));
    const wrap=this.root.querySelector('#it-other-wrap'),box=this.root.querySelector('#it-other');
    if(wrap&&box){
      wrap.hidden=!other.length;
      const names=new Map(this.log.filter(e=>other.includes(drawCode(e))).map(e=>[drawCode(e),e.name||e.code]));
      const html=other.map(c=>`<span class="it-key it-flat${held.has(c)?' held':' was'}" data-it-code="${esc(c)}">${esc(names.get(c)||c)}</span>`).join('');
      if(box.innerHTML!==html)box.innerHTML=html;
    }
  }
  paintCombo(){
    const el=this.root?.querySelector('#it-combo');if(!el)return;
    el.innerHTML=this.model.combo.length?this.model.combo.map((c,i)=>`${i?'<span class="it-plus" aria-hidden="true">+</span>':''}<kbd>${esc(c)}</kbd>`).join(''):'<span class="it-faint">None yet</span>';
    el.setAttribute('aria-label',this.model.combo.length?`Last combination: ${this.model.combo.join(' + ')}`:'No combination yet');
  }
  paintMode(){
    const el=this.root?.querySelector('#it-mode');if(!el)return;
    const [text,cls]=this.error?[`Listening failed: ${this.error}`,'attention']
      :this.mode==='raw'?['Listening to all devices','accent']
      :this.mode==='simulated'?['Simulated devices','']
      :this.mode==='hidden'?['Paused while the window is hidden','']
      :this.mode==='browser'?['Browser preview: this window only','']:['Starting…',''];
    el.textContent=text;el.className=`chip ${cls}`.trim();
  }
  paintFilterOptions(){
    const sel=this.root?.querySelector('#it-filter');if(!sel)return;
    const list=this.sortedDevices();
    sel.innerHTML=`<option value="">All devices</option>`+list.map(d=>`<option value="${esc(d.id)}"${d.id===this.filter?' selected':''}>${esc(d.name)} · ${esc(d.role)}</option>`).join('');
  }
  sortedDevices(){
    const ids=new Set(this.devices.map(d=>d.id));
    const extra=Object.keys(this.counts).filter(id=>!ids.has(id)).map(id=>({id,kind:id==='injected'?'hid':'browser',name:this.deviceName(id),role:id==='injected'?'Input sent by an app':'',vid:null,pid:null}));
    return [...this.devices,...extra].sort((a,b)=>(KIND_ORDER[a.kind]??9)-(KIND_ORDER[b.kind]??9)||a.name.localeCompare(b.name));
  }
  paintDevices(){
    const ul=this.root?.querySelector('#it-devices');if(!ul||!this.dirtyDevices&&ul.children.length)return this.paintCounts();
    this.dirtyDevices=false;
    const list=this.sortedDevices();
    ul.innerHTML=list.length?list.map(d=>{
      const ids=d.vid!=null?`${hex(d.vid,4).slice(2)}:${hex(d.pid,4).slice(2)}`:'';
      const on=this.filter===d.id;
      return `<li><button type="button" class="it-device${on?' is-selected':''}" data-it="filter" data-dev="${esc(d.id)}" aria-pressed="${on}">
        <span class="it-dot" data-it-dot="${esc(d.id)}" aria-hidden="true"></span>
        <span class="it-device-copy"><strong>${esc(d.name)}</strong><span class="it-small">${esc([d.role,ids].filter(Boolean).join(' · '))}</span></span>
        <span class="it-count" data-it-count="${esc(d.id)}"></span></button></li>`;
    }).join(''):'<li class="it-empty">Looking for devices…</li>';
    this.paintCounts();
  }
  paintCounts(){
    if(!this.root)return;
    for(const el of this.root.querySelectorAll('[data-it-count]')){
      const c=this.counts[el.dataset.itCount];
      const parts=c?[[c.down,'key'],[c.repeat,'repeat'],[c.buttons,'click'],[c.wheel,'wheel'],[c.hid,'HID'],[c.moves,'move']].filter(([n])=>n>0).map(([n,w])=>`${n} ${w}${n===1?'':'s'}`):[];
      const text=parts.length?parts.join(' · '):'No events';
      if(el.textContent!==text)el.textContent=text;
    }
    this.paintActivity();
  }
  paintActivity(){
    if(!this.root)return;
    for(const el of this.root.querySelectorAll('[data-it-dot]')){
      const c=this.counts[el.dataset.itDot];
      el.classList.toggle('on',!!c&&c.last!=null&&this.now-c.last<500);
    }
  }
  paintVibe(){
    const box=this.root?.querySelector('#it-vibe');if(!box)return;
    const held=this.model.heldCodes(),was=this.model.pressed;
    const html=VIBE_KEY.map(v=>{
      const claimed=this.claimed.includes(v.vk);
      const cls=held.has(v.code)?' held':was.has(v.code)?' was':'';
      return `<div class="it-vibe-item${cls}" data-it-code="${v.code}"><span class="it-vibe-name">${esc(v.label)}</span><span class="it-vibe-code">${v.code}</span>${claimed?'<span class="it-claim">Claimed by Unified Control</span>':'<span class="it-faint">Not claimed</span>'}</div>`;
    }).join('');
    if(box.innerHTML!==html)box.innerHTML=html;
  }
  rowHtml(e,prev){
    const cols=logColumns(e,prev,id=>this.deviceName(id));
    const kind=e.claimed?' claimed':e.kind==='device'?' device':'';
    return `<tr class="it-ev it-${esc(e.kind)}${kind}">${cols.map((c,i)=>`<td${i===9&&e.claimed?' class="it-claim-cell"':''}>${esc(c)}</td>`).join('')}</tr>`;
  }
  paintLog(){
    const body=this.root?.querySelector('#it-log-body');if(!body)return;
    const list=this.visible().slice(-SHOWN);
    let html='';for(let i=list.length-1;i>=0;i--)html+=this.rowHtml(list[i],list[i-1]);
    body.innerHTML=html;this.lastShown=list[list.length-1]||null;
    this.root.querySelector('#it-log-empty').hidden=!!list.length;
  }
  appendLog(events){
    const body=this.root?.querySelector('#it-log-body');if(!body)return;
    let html='';
    for(const e of events){
      if(this.filter&&e.device!==this.filter)continue;
      html=this.rowHtml(e,this.lastShown)+html;this.lastShown=e;
    }
    if(!html)return;
    body.insertAdjacentHTML('afterbegin',html);
    while(body.rows.length>SHOWN)body.deleteRow(-1);
    this.root.querySelector('#it-log-empty').hidden=true;
  }
  paintPause(){
    this.setLogNote(this.paused?`Log paused. ${this.pausedCount} new event${this.pausedCount===1?'':'s'} not shown; the drawing and counts still update.`:'');
  }
  setLogNote(text){const el=this.root?.querySelector('#it-log-note');if(el)el.textContent=text;}

  // ---------- audio ----------
  backend(){
    if(!this.audio)this.audio=this.fake()||globalThis.__UC_FAKE_AUDIO__?new FakeAudio():new WebAudio();
    if(this.audio.simulated)globalThis.__ucInputTesterAudio=this.audio;
    return this.audio;
  }
  async loadAudioDevices(){
    const epoch=this.epoch;
    try{
      const a=this.backend();
      const [inputs,outputs]=await Promise.all([a.list('audioinput'),a.list('audiooutput')]);
      if(epoch!==this.epoch)return;
      this.inputs=inputs;this.outputs=outputs;
    }catch(error){this.mic.error=errorText(error);}
    this.paintAudio();
  }
  async toggleMic(){if(this.audio?.isOpen){this.audio.close();this.mic={...this.mic,state:'off',format:null};this.paintAudio();}else await this.openMic();}
  async openMic(){
    const epoch=this.epoch,a=this.backend();
    this.mic={state:'starting',format:null,error:'',peak:0,hold:0,holdAt:0,clips:0};this.paintAudio();
    try{
      const format=await a.open(this.micId,{processing:this.processing});
      if(epoch!==this.epoch){a.close();return;}
      this.mic.state='on';this.mic.format=format;
      // Names are available once the microphone may be used.
      const [inputs,outputs]=await Promise.all([a.list('audioinput'),a.list('audiooutput')]);
      if(epoch!==this.epoch)return;
      this.inputs=inputs;this.outputs=outputs;
    }catch(error){
      if(epoch!==this.epoch)return;
      this.mic.state='error';
      this.mic.error=error?.name==='NotAllowedError'?'Microphone use was not allowed. Allow it when Unified Control asks, or in Windows Settings > Privacy > Microphone.'
        :error?.name==='NotFoundError'||error?.name==='OverconstrainedError'?'This microphone was not found. It may be unplugged.':errorText(error);
    }
    this.paintAudio();
  }
  paintMeter(){
    const bar=this.root?.querySelector('#it-meter-fill');if(!bar)return;
    const lv=this.audio?.isOpen?this.audio.level():null,now=performance.now();
    if(!lv){bar.style.width='0%';return;}
    const db=dbfs(lv.rms),pk=dbfs(lv.peak);
    if(pk>this.mic.hold||now-this.mic.holdAt>1500){this.mic.hold=pk;this.mic.holdAt=now;}
    if(lv.peak>=.999)this.mic.clips++;
    this.mic.peak=Math.max(this.mic.peak||-90,pk);
    const pos=v=>`${Math.max(0,Math.min(100,(v+60)/60*100)).toFixed(1)}%`;
    bar.style.width=pos(db);
    this.root.querySelector('#it-meter-hold').style.left=pos(this.mic.hold);
    const text=`Level ${db.toFixed(0)} dB · Peak ${this.mic.peak.toFixed(0)} dB${this.mic.clips?` · Clipped ${this.mic.clips}×`:''}`;
    const t=this.root.querySelector('#it-meter-text');if(t.textContent!==text)t.textContent=text;
    bar.classList.toggle('hot',pk>-3);
  }
  async record(){
    const epoch=this.epoch;
    if(!this.audio?.isOpen)await this.openMic();
    if(!this.audio?.isOpen||epoch!==this.epoch)return;
    this.recState='recording';this.rec=null;this.note='';this.paintAudio();
    try{
      const rec=await this.audio.record(5,p=>{const el=this.root?.querySelector('#it-rec-progress');if(el)el.value=p;});
      if(epoch!==this.epoch)return;
      this.rec=rec;this.recState='ready';this.note='Recorded 5 seconds. It is kept only in this window until you leave the page.';
    }catch(error){if(epoch===this.epoch){this.recState='';this.note=errorText(error);}}
    this.paintAudio();
  }
  async playBack(){
    if(!this.rec)return;
    this.recState='playing';this.paintAudio();
    try{await this.audio.play(this.rec,this.outId);this.note='';}catch(error){this.note=errorText(error);}
    if(this.rec)this.recState='ready';this.paintAudio();
  }
  async save(){
    if(!this.rec)return;
    if(!this.native){this.note='Saving needs the Unified Control app.';this.paintAudio();return;}
    try{
      const wav=toBase64(encodeWav(this.rec.channels,this.rec.sampleRate));
      const d=new Date(),p=n=>String(n).padStart(2,'0');
      const stamp=`${d.getFullYear()}-${p(d.getMonth()+1)}-${p(d.getDate())}-${p(d.getHours())}${p(d.getMinutes())}${p(d.getSeconds())}`;
      const r=await this.invoke('input_tester_save_audio',{wav,stamp});
      this.note=`Saved: ${r.path}`;
    }catch(error){this.note=errorText(error);}
    this.paintAudio();
  }
  async tone(side){
    try{await this.backend().tone(side,this.outId);}catch(error){this.note=errorText(error);this.paintAudio();}
  }
  paintAudio(){
    const micCard=this.root?.querySelector('#it-mic-card'),outCard=this.root?.querySelector('#it-out-card');if(!micCard||!outCard)return;
    const opts=(list,sel)=>list.length?list.map(d=>`<option value="${esc(d.id)}"${d.id===sel?' selected':''}>${esc(d.label)}</option>`).join(''):'<option value="default">Windows default</option>';
    const m=this.mic,f=m.format,on=m.state==='on',busy=this.recState==='recording'||this.recState==='playing';
    const fmt=f?`<dl class="it-format"><dt>Device</dt><dd>${esc(f.label)}</dd><dt>Sample rate</dt><dd>${f.sampleRate?`${(f.sampleRate/1000).toFixed(1)} kHz`:'Not reported'}</dd><dt>Channels</dt><dd>${f.channels===1?'1 (mono)':f.channels===2?'2 (stereo)':esc(f.channels??'Not reported')}</dd><dt>Bits</dt><dd>${f.bits?`${f.bits}-bit`:'Not reported'}</dd><dt>Processing</dt><dd>${f.processing?'On (echo, noise, gain)':'Off (raw sound)'}</dd></dl>
      <p class="it-small">This is the format Windows gives to Unified Control. Discord and other apps may ask for another format and add their own processing.</p>`:'';
    micCard.innerHTML=`<div class="it-card-head"><h2 id="it-mic-title">Microphone</h2><span class="it-spacer"></span>${this.audio?.simulated?'<span class="chip">Simulated</span>':''}</div>
      <div class="it-audio-row"><label class="it-inline it-grow"><span class="sr-only">Microphone</span><select id="it-mic-select" aria-label="Microphone">${opts(this.inputs,this.micId)}</select></label>
        <button class="btn${on?' active':' primary'}" type="button" data-it="mic" ${m.state==='starting'?'disabled':''}>${on?'Stop':'Start'}</button></div>
      <label class="it-check"><input id="it-processing" type="checkbox"${this.processing?' checked':''}> Echo, noise and gain processing (like calls)</label>
      <div class="it-meter" role="img" aria-label="Microphone level"><div id="it-meter-fill" class="it-meter-fill"></div><div id="it-meter-hold" class="it-meter-hold"></div></div>
      <p id="it-meter-text" class="it-small" aria-live="off">${on?'':m.state==='starting'?'Starting the microphone…':'Start the microphone to see its level.'}</p>
      ${m.error?`<p class="it-small attention" role="alert">${esc(m.error)}</p>`:''}
      ${fmt}
      <div class="it-audio-row">
        <button class="btn" type="button" data-it="record" ${busy||m.state==='starting'?'disabled':''}>${this.recState==='recording'?'Recording…':'Record 5 seconds'}</button>
        <button class="btn" type="button" data-it="play" ${this.rec&&!busy?'':'disabled'}>${this.recState==='playing'?'Playing…':'Play back'}</button>
        <button class="btn" type="button" data-it="save" ${this.rec&&!busy?'':'disabled'}>Save</button>
        <button class="btn ghost" type="button" data-it="discard" ${this.rec&&!busy?'':'disabled'}>Discard</button>
      </div>
      ${this.recState==='recording'?'<progress id="it-rec-progress" class="it-progress" max="1" value="0"></progress>':''}
      <p class="it-small" id="it-audio-note" role="status">${esc(this.note||'Recordings stay on this PC and are not saved unless you press Save (to Downloads).')}</p>`;
    outCard.innerHTML=`<div class="it-card-head"><h2 id="it-out-title">Speakers</h2></div>
      <label class="it-inline"><span class="sr-only">Speaker</span><select id="it-out-select" aria-label="Speaker">${opts(this.outputs,this.outId)}</select></label>
      <p class="it-small">Plays a soft 440 Hz tone for one second. Play back uses this speaker too.</p>
      <div class="it-audio-row"><button class="btn" type="button" data-it="tone" data-side="left">Left</button><button class="btn" type="button" data-it="tone" data-side="both">Both</button><button class="btn" type="button" data-it="tone" data-side="right">Right</button></div>`;
  }
}
