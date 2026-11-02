// The parent is the only native IPC caller. The embedded UI receives only this
// explicit CoolerScreen command set and event set, never a generic host invoke.
export const COOLER_COMMANDS=new Set(`app_state settings_update activity job_get job_cancel set_focus originals_list import_begin import_append import_finish import_abort import_bytes import_path missing_delete original_get original_patch original_delete original_reveal estimate loop_search loop_point version_create version_patch version_delete version_send version_remake version_retry sources_list browse_set link_add source_set source_items source_add theme_get theme_set theme_standard theme_preview theme_asset_add theme_set_base calibration_set fit_cal theme_use theme_apply gallery_set_dir gallery_open gallery_sync device_state device_check device_presence device_restart device_setup device_restore device_reconnect ext_sensor_status lyrics_status game_fps_status lyrics_offset_set weather_search weather_set_place screen_preview screen_preview_end twin_watch live_set live_state autostart_set`.split(' '));
const EVENTS=new Set(['cs:job','cs:library','cs:device','cs:app']);
const escape=s=>String(s??'').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
// Health runs every 10s and PnP every 30s. These coarse display limits
// tolerate missed polls; they do not turn a cached check into a live guarantee.
const HEALTH_MAX_AGE=60_000,PNP_MAX_AGE=90_000;
const CHECKS={
  connected:{states:['connected'],text:'Live reported a verified connection.'},
  descriptor_failure:{states:['failed'],text:'Windows reported a cooler USB enumeration failure.'},
  not_found:{states:['failed'],text:'Windows did not expose the cooler transport.'},
  no_reply:{states:['failed'],text:'The cooler did not answer the firmware handshake.'},
  write_timeout:{states:['failed'],text:'The cooler USB write timed out.'},
  short_write:{states:['failed'],text:'The cooler accepted only part of a USB transfer.'},
  transport_error:{states:['failed'],text:'The cooler transport reported an error.'},
  busy:{states:['blocked'],text:'The cooler transport was in use.'},
  ownership:{states:['blocked'],text:'Another cooler application owned the screen.'},
  worker_timeout:{states:['unresponsive'],text:'The cooler engine did not answer its health request.'},
  worker_unavailable:{states:['waiting','unavailable'],text:'Cooler health information was unavailable.'},
  sample_deferred:{states:['waiting'],text:'The check was deferred while commands were being sent.'},
  engine_disabled:{states:['disabled'],text:'The engine was disabled; the screen connection is unknown.'},
  live_off:{states:['off'],text:'Live was off; no live connection was expected.'},
  intentional_hold:{states:['paused'],text:'Live was intentionally paused.'},
  connecting:{states:['connecting'],text:'Live was connecting.'},
};
const WORKER_STATES={
  starting:['Engine starting','Starting the cooler engine. The screen connection is unknown.'],
  stopping:['Engine stopping','The cooler engine is stopping. The screen connection is unknown.'],
  reconnecting:['Engine reconnecting','The cooler engine is restarting. The screen connection is unknown.'],
  blocked:['Engine blocked','Another cooler application owns the screen.'],
  unavailable:['Engine unavailable','The cooler engine is unavailable. The screen connection is unknown.'],
  error:['Engine error','The cooler engine reported an error. The screen connection is unknown.'],
};
function recent(timestamp,now,maxAge){
  if(typeof timestamp!=='string'||!/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/.test(timestamp))return false;
  const at=Date.parse(timestamp);return Number.isFinite(at)&&new Date(at).toISOString()===timestamp&&at<=now+1000&&now-at<=maxAge;
}
export function coolerSummary(state,{native=true,readError=false,now=Date.now()}={}){
  if(!native)return {status:'Preview',description:'Cooler controls are available in the Windows app.'};
  if(readError)return {status:'Status unavailable',description:'Cooler status could not be refreshed. The screen connection is unknown.'};
  if(state?.enabled===false)return {status:'Disabled',description:'Cooler screen is disabled.'};
  if(state?.enabled!==true)return {status:'Status unknown',description:'Waiting for cooler engine status. The screen connection is unknown.'};
  const worker=Object.hasOwn(WORKER_STATES,state.state)?WORKER_STATES[state.state]:null;
  if(worker)return {status:worker[0],description:worker[1]};
  if(!['running','ready','connected'].includes(state.state))return {status:'Status unknown',description:'Cooler engine status is unavailable. The screen connection is unknown.'};
  const status='Engine running',health=state.health;
  if(state.diagnosticsEnabled===false)return {status,description:'Connection diagnostics are off. The screen connection is unknown.'};
  if(!health||health.code==='waiting_sample'||health.code==='diagnostics_off')return {status,description:'Waiting for a connection check. The screen connection is unknown.'};
  const check=Object.hasOwn(CHECKS,health.code)?CHECKS[health.code]:null;
  if(!check||!check.states.includes(health.state))return {status,description:'The latest connection check is unavailable. The screen connection is unknown.'};
  if(!recent(health.lastSampleAt,now,HEALTH_MAX_AGE))return {status,description:'The last connection check is missing or out of date. The screen connection is unknown.'};
  const fromPnp=health.code==='descriptor_failure'&&health.pnp?.available===true&&health.pnp.normalNodes===0&&health.pnp.exactMatchedDescriptorCount>0;
  if(fromPnp&&!recent(health.pnpSampledAt,now,PNP_MAX_AGE))return {status,description:'The USB information used by the last connection check is missing or out of date. The screen connection is unknown.'};
  return {status,description:`Last connection check: ${check.text}`};
}
export class CoolerPanel{
  constructor(invoke,native,onState=()=>{}){
    this.invoke=invoke;this.native=native;this.onState=onState;this.readError=false;this.hasState=false;this.state={enabled:native?null:false,state:native?'unknown':'disabled',diagnosticsEnabled:true};this.frame=null;this.container=null;this.polling=false;this.configuring=false;this.epoch=0;this.pendingLeave=new Map();this.nextLeave=0;this.diagnosticsWrite=Promise.resolve();this.diagnosticsSaving=false;
    window.addEventListener('message',e=>this.receive(e));
    this.timer=setInterval(()=>this.refresh(),1500);
  }
  summary(){return coolerSummary(this.state,{native:this.native,readError:this.readError});}
  acceptState(state){this.state=state;this.readError=false;this.hasState=true;this.paint();this.onState();}
  markup(){const summary=this.summary();return `<div class="page-heading"><div><span class="eyebrow">Lovingcool SC70</span><h1>Cooler screen</h1><p>Your CoolerScreen themes and live controls.</p></div></div><section class="device-header"><div><strong>Cooler screen</strong><p id="cooler-message">${escape(summary.description)}</p></div><label class="dock-switch">Enabled <input id="cooler-enabled" type="checkbox" role="switch" ${this.state.enabled?'checked':''} ${this.native?'':'disabled'}></label><span id="cooler-state" class="chip">${escape(summary.status)}</span></section><p class="hint" id="cooler-help">Close the separate CoolerScreen app before enabling this device. Your dock stays independent.</p><div id="cooler-content"></div>`;}
  mount(container){this.container=container;container.querySelector('#cooler-enabled').addEventListener('change',e=>this.configure(e.target.checked));const controls=document.createElement('div');controls.className='dock-controls';controls.innerHTML=`<label class="dock-switch">Connection diagnostics <input id="cooler-diagnostics" type="checkbox" ${this.state.diagnosticsEnabled!==false?'checked':''} ${this.native?'':'disabled'}></label><button id="cooler-reset" class="btn" ${this.native?'':'disabled'}>Reset cooler USB</button><p class="hint" id="cooler-health" role="status"></p>`;container.querySelector('#cooler-help').after(controls);controls.querySelector('#cooler-diagnostics').addEventListener('change',e=>this.setDiagnostics(e.target.checked));controls.querySelector('#cooler-reset').addEventListener('click',()=>this.reset());this.paint();this.refresh();}
  active(container){return this.container===container&&!!container.querySelector('#cooler-content');}
  async refresh(){if(!this.native||this.polling||this.configuring||this.diagnosticsSaving)return;this.polling=true;const epoch=this.epoch;try{const state=await this.invoke('cooler_state');if(epoch===this.epoch)this.acceptState(state);}catch{if(epoch===this.epoch){this.readError=true;this.hasState=true;this.paint();this.onState();}}finally{this.polling=false;}}
  setDiagnostics(enabled){this.resetNote='';this.epoch++;this.diagnosticsSaving=true;this.diagnosticsWrite=this.diagnosticsWrite.catch(()=>{}).then(async()=>{try{const result=await this.invoke('settings_update',{patch:{coolerDiagnostics:enabled}});this.acceptState(result.cooler||this.state);}catch(error){this.paint();const note=this.container?.querySelector('#cooler-health');if(note)note.textContent='The diagnostics setting could not be saved.';}finally{this.diagnosticsSaving=false;}});return this.diagnosticsWrite;}
  async reset(){if(this.configuring||!this.native)return;this.configuring=true;this.epoch++;this.paint();const note=this.container?.querySelector('#cooler-health');try{await this.prepareLeave();if(note)note.textContent='Waiting for Windows approval to reset only the cooler USB port.';const job=await this.invoke('cooler_call',{command:'device_reconnect',args:{}});const deadline=Date.now()+120000;for(;;){const value=await this.invoke('cooler_call',{command:'job_get',args:{jobId:job.jobId}});if(value.status==='done'){this.resetNote='The cooler answered after the USB reset. Check the physical display.';if(note)note.textContent=this.resetNote;break;}if(['error','cancelled'].includes(value.status))throw new Error(value.error||'The cooler reset did not complete.');if(Date.now()>=deadline)throw new Error('The cooler recovery is still running. Check its progress before retrying.');await new Promise(resolve=>setTimeout(resolve,500));}}catch(error){this.resetNote=error?.message||error?.error||'The cooler USB reset could not finish.';if(note)note.textContent=this.resetNote;}finally{this.configuring=false;this.paint();}}
  async configure(enabled){if(this.configuring)return;this.configuring=true;this.epoch++;const control=this.container?.querySelector('#cooler-enabled');if(control)control.disabled=true;try{if(!enabled)await this.prepareLeave();this.acceptState(await this.invoke('cooler_configure',{enabled}));}catch(error){const message=this.container?.querySelector('#cooler-message');if(message)message.textContent='The cooler setting could not be changed.';}finally{this.configuring=false;if(control){control.disabled=!this.native;control.checked=!!this.state.enabled;}}}
  paint(){
    if(!this.container?.isConnected||!this.container.querySelector('#cooler-content'))return;
    const message=this.container.querySelector('#cooler-message'),status=this.container.querySelector('#cooler-state'),control=this.container.querySelector('#cooler-enabled'),content=this.container.querySelector('#cooler-content');
    const summary=this.summary();message.textContent=summary.description;status.textContent=summary.status;control.checked=!!this.state.enabled;
    const diagnostics=this.container.querySelector('#cooler-diagnostics'),health=this.container.querySelector('#cooler-health'),reset=this.container.querySelector('#cooler-reset');
    if(diagnostics){diagnostics.checked=this.state.diagnosticsEnabled!==false;diagnostics.disabled=!this.native||this.diagnosticsSaving;}
    if(health&&!this.configuring)health.textContent=this.resetNote||summary.description;
    if(reset)reset.disabled=!this.native||!this.state.enabled||this.state.state!=='running'||!!this.state.busy||this.configuring;
    const ready=this.state.enabled&&['running','ready','connected'].includes(this.state.state);
    if(ready&&!this.frame){this.frameReady=false;this.frame=document.createElement('iframe');this.frame.id='cooler-frame';this.frame.title='CoolerScreen controls';this.frame.src='cooler/index.html';this.frame.style.cssText='width:100%;height:max(640px,calc(100vh - 270px));border:1px solid var(--border);border-radius:8px;background:var(--bg)';content.replaceChildren(this.frame);}
    else if(!ready&&(!this.frame||!this.state.enabled)){this.frame=null;content.innerHTML=`<section class="empty-state"><h2>${this.native?'CoolerScreen controls':'CoolerScreen integration'}</h2><p>${this.native?'Enable the cooler to open its themes, library and live controls here.':'The integrated controls are available in the Windows app.'}</p></section>`;}
  }
  send(message){this.frame?.contentWindow?.postMessage({channel:'uc-cooler-v1',...message},location.origin);}
  event(data){if(data&&EVENTS.has(data.event))this.send({type:'event',event:data.event,payload:data.payload});}
  async receive(e){
    if(e.origin!==location.origin||e.source!==this.frame?.contentWindow||e.data?.channel!=='uc-cooler-v1')return;
    const m=e.data;
    if(m.type==='bridge-ready'){this.frameReady=true;return;}
    if(m.type==='leave-ready'){const p=this.pendingLeave.get(m.id);if(p){this.pendingLeave.delete(m.id);clearTimeout(p.timer);m.error?p.reject(new Error(m.error)):p.resolve();}return;}
    if(m.type!=='call'||!Number.isSafeInteger(m.id)||m.id<1)return;
    if(!COOLER_COMMANDS.has(m.command)||!m.args||Array.isArray(m.args)||typeof m.args!=='object'){this.send({type:'result',id:m.id,error:'Unsupported cooler command.'});return;}
    const source=e.source;
    try{const result=await this.invoke('cooler_call',{command:m.command,args:m.args});if(source===this.frame?.contentWindow)this.send({type:'result',id:m.id,result});}
    catch(error){if(source===this.frame?.contentWindow)this.send({type:'result',id:m.id,error:typeof error==='string'?{error}:error&&typeof error==='object'?{error:error.error||error.message||'CoolerScreen request failed.',code:error.code,status:error.status,failedUsb:!!error.failedUsb}:{error:'CoolerScreen request failed.'}});}
  }
  async prepareLeave({navigation=false}={}){await this.diagnosticsWrite;if(!this.frame||!this.frameReady)return;return new Promise((resolve,reject)=>{const id=++this.nextLeave;const timer=setTimeout(()=>{this.pendingLeave.delete(id);reject(new Error('Cooler edits have not finished saving. Keep this page open and retry.'));},120000);this.pendingLeave.set(id,{resolve,reject,timer});this.send({type:'prepare-leave',id,navigation});});}
  unmount(){this.frame=null;this.frameReady=false;this.container=null;}
}
