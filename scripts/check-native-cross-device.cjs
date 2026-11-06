// Isolated native host, dock/cooler workers and provider-only helper. Synthetic
// account/media/weather sources only; no installed app, real device or account.
'use strict';
const {chromium}=require(process.env.UC_PLAYWRIGHT_MODULE||'playwright');
const {spawn}=require('node:child_process');
const fs=require('node:fs'),path=require('node:path'),os=require('node:os'),net=require('node:net'),assert=require('node:assert/strict');
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
async function bounded(promise,label,timeout=20000){let timer;try{return await Promise.race([promise,new Promise((_,reject)=>{timer=setTimeout(()=>reject(Error(`${label} timed out`)),timeout);})]);}finally{clearTimeout(timer);}}
async function wait(check,label,timeout=25000){const end=Date.now()+timeout;while(Date.now()<end){if(await bounded(Promise.resolve().then(check),label,Math.max(1,end-Date.now())))return;await delay(70);}throw Error(`${label} did not become true`);}
function debugExecutable(value,name){const file=fs.realpathSync(path.resolve(value||''));if(!file.toLowerCase().includes(`${path.sep}debug${path.sep}`)||path.basename(file).toLowerCase()!==name||!fs.statSync(file).isFile())throw Error(`Explicit debug ${name} required; installed/release executables refused`);return file;}

const readline=require('node:readline');
async function callbackDrain(exe,env,home,snapshot,closePipe){
  const child=spawn(exe,['--dock-worker'],{env:{...env,UC_DOCK_WORKER:'1'},windowsHide:true,stdio:['pipe','pipe','ignore']});
  const exited=new Promise((resolve,reject)=>{child.once('exit',(code,signal)=>resolve({code,signal}));child.once('error',reject);});
  let id=0,last,callback,reports=0;const pending=new Map(),lines=readline.createInterface({input:child.stdout});
  lines.on('line',line=>{const message=JSON.parse(line);if(message.type==='status'){last=message;reports++;}else if(message.type==='cooler_action')callback=message;else if(message.type==='reply'){const p=pending.get(message.id);pending.delete(message.id);if(p)Object.hasOwn(message.result,'Ok')?p.resolve(message.result.Ok):p.reject(Error(message.result.Err));}});
  const send=request=>{const requestId=++id;const reply=new Promise((resolve,reject)=>pending.set(requestId,{resolve,reject}));child.stdin.write(JSON.stringify({version:1,id:requestId,request})+'\n');return bounded(reply,'raw dock RPC',7000);};
  try{
    await send({op:'init',state:snapshot,home:path.join(home,closePipe?'callback-eof':'callback-shutdown'),resources:path.resolve(__dirname,'../src-tauri/resources'),fake:true});
    await send({op:'enabled',enabled:true});await wait(()=>last?.status.state==='connected'&&last.status.framesSent>=14,'raw callback dock connected');
    await send({op:'button',index:0,pressed:true});await wait(()=>callback,'raw child callback');
    assert.deepEqual(callback.action,{op:'brightness_set',value:35});
    if(closePipe){child.stdin.end();assert.deepEqual(await bounded(exited,'callback EOF cancellation',7000),{code:0,signal:null});}
    else{
      // The command dispatcher is now joining the action executor. Completion
      // must bypass that queue so this drain cannot wait on itself.
      const shutdown=send({op:'shutdown'}),beforeReport=reports;
      child.stdin.write(JSON.stringify({version:1,id:0,request:{op:'action_result',action_id:callback.action_id+1,result:{Err:'wrong correlation'}}})+'\n');
      await wait(()=>reports>beforeReport,'busy report after wrong completion');assert.equal(last.busy,true,'unmatched completion cannot release executor');
      child.stdin.write(JSON.stringify({version:1,id:0,request:{op:'action_result',action_id:callback.action_id,result:{Ok:null}}})+'\n');
      await shutdown;child.stdin.end();assert.deepEqual(await bounded(exited,'callback shutdown drain',7000),{code:0,signal:null});
    }
  }finally{child.stdin.end();await bounded(exited,'raw callback cleanup',10000);lines.close();}
}
(async()=>{
  const exe=debugExecutable(process.env.UC_TEST_EXE,'unified-control.exe'),worker=debugExecutable(process.env.UC_COOLER_WORKER_EXE,'uc-cooler-worker.exe');
  const home=fs.mkdtempSync(path.join(os.tmpdir(),'uc-native-cross-device-'));
  const port=await new Promise(resolve=>{const server=net.createServer();server.listen(0,'127.0.0.1',()=>{const value=server.address().port;server.close(()=>resolve(value));});});
  const env={...process.env,UC_HOME:home,UC_TEST_QUIET:'1',UC_MULTI:'1',UC_FAKE_DEVICE:'1',UC_COOLER_WORKER_EXE:worker,UC_COOLER_TEST_ACTION_DELAY_MS:'2500',WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:`--remote-debugging-port=${port}`};
  for(const key of ['UC_TEST_TRAY','UC_TEST_TRAY_QUIT_MS','UC_TEST_TRAY_ICON','UC_DOCK_TEST_ACTION_DELAY_MS'])delete env[key];
  const child=spawn(exe,[],{env,windowsHide:true,stdio:['ignore','pipe','pipe']}),exited=new Promise((resolve,reject)=>{child.once('exit',(code,signal)=>resolve({code,signal}));child.once('error',reject);});
  let browser,page,logs='';const capture=data=>{logs=(logs+data).slice(-512*1024);};child.stdout.on('data',capture);child.stderr.on('data',capture);
  const call=(command,args={})=>bounded(page.evaluate(async({command,args})=>{try{return await window.__TAURI_INTERNALS__.invoke(command,args);}catch(error){throw Error(JSON.stringify(error));}},{command,args}),`invoke ${command}`);
  const cooler=()=>call('cooler_state'),dock=()=>call('dock_state'),coolerCall=(command,args={})=>call('cooler_call',{command,args}),controls=()=>call('cooler_controls_state');
  const quit=()=>bounded(new Promise((resolve,reject)=>{const request=spawn(exe,['--quit'],{env,windowsHide:true,stdio:'ignore'});request.once('error',reject);request.once('exit',resolve);}),'request-only Quit');
  const press=async(index,states=[true,false])=>{for(const pressed of states)await call('dock_test_button',{index,pressed});};
  const settled=()=>wait(async()=>{const state=await dock();return !state.busy&&!state.worker?.busy&&!state.worker?.crossActionBusy;},'cross-device action settled');
  const runKey=async index=>{const before=(await dock()).actions;await press(index);await wait(async()=>(await dock()).actions>before,'physical action completion');await settled();};
  const errors=[];let snapshot;
  try{
    await wait(async()=>{if(child.exitCode!==null)throw Error(`Quiet app exited: ${logs}`);try{return(await fetch(`http://127.0.0.1:${port}/json/version`)).ok;}catch{return false;}},'quiet WebView');
    browser=await chromium.connectOverCDP(`http://127.0.0.1:${port}`);await wait(()=>{page=browser.contexts().flatMap(context=>context.pages()).find(page=>/tauri/.test(page.url()));return!!page;},'UC page');
    page.on('pageerror',error=>errors.push(error.message));await wait(()=>page.evaluate(()=>document.getElementById('storage-status')?.textContent==='Changes saved in Unified Control'),'initial saved profiles');
    assert.equal((await controls()).available,false,'read-only catalog does not enable a cooler');
    await call('cooler_configure',{enabled:true});await wait(async()=>{const s=await cooler();return s.state==='running'&&!s.busy;},'fake cooler ready');
    await assert.rejects(coolerCall('dock_action',{action:{op:'brightness_set',value:2}}),'private physical command is not a generic UI/MCP route');
    const png=Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScLbtAAAAABJRU5ErkJggg==','base64');
    const themes=[];for(const name of ['Cross Alpha.png','Cross Beta.png']){const value=await coolerCall('import_bytes',{name,bytes:[...png]});themes.push(value.id);}
    assert.notEqual(themes[0],themes[1]);const standard=await coolerCall('theme_standard');
    const theme={...standard,widgets:standard.widgets.filter(widget=>widget.type==='clock')};
    for(const id of themes)await coolerCall('theme_set',{originalId:id,theme});
    await coolerCall('theme_use',{originalId:themes[0]});await coolerCall('live_set',{patch:{inApp:true,music:false}});
    const initial=await controls(),embedded=initial.embeddedThemeId;
    const profiles=await call('profiles_state');snapshot=profiles.state;const keys=snapshot.profiles[0].pages[0].keys;
    const actions=[{op:'brightness_set',value:35},{op:'brightness_step',delta:10},{op:'live',mode:'toggle'},{op:'theme_next'},{op:'theme_previous'},{op:'theme_select',theme_id:themes[1]}];
    for(const [index,action]of actions.entries()){keys[index].action='cooler';keys[index].value=JSON.stringify(action);keys[index].a.text=`Cross ${index} A`;keys[index].b.text=`Cross ${index} B`;}
    await call('profiles_save',{snapshot,expectedRevision:profiles.revision});
    assert.equal((await controls()).brightness.requested,initial.brightness.requested,'saving cross-device actions is inert');
    await call('dock_configure',{patch:{enabled:true}});await wait(async()=>{const s=await dock();return s.state==='connected'&&s.framesSent>=14;},'dock connected');
    await page.locator('.nav-item[data-page="cooler"]').click();await wait(()=>page.frames().some(frame=>/cooler\/index.html/.test(frame.url())),'embedded cooler page');
    const frame=page.frames().find(frame=>/cooler\/index.html/.test(frame.url()));await frame.locator('#liveSwitch').waitFor();
    await wait(()=>frame.locator('#liveSwitch').isChecked(),'initial Live switch');
    await press(0,[true,true,false]);await wait(async()=>(await dock()).worker.crossActionBusy,'host callback admitted');
    assert.equal(await quit(),2,'Quit refuses an admitted host action');await assert.rejects(call('cooler_configure',{enabled:false}),'target disable refuses admitted host callback');
    await settled();assert.equal((await dock()).actions,1,'held duplicate physical press executes once');
    let state=await controls();assert.equal(state.brightness.requested,35);assert.equal(state.brightness.reported,35);assert.equal(state.embeddedThemeId,embedded);
    await frame.locator('#gear').click();await wait(async()=>(await frame.locator('#scrBrightness').textContent()).includes('Screen brightness: 35%'),'embedded brightness readout reflects dock change');await frame.locator('#drawerClose').click();
    await runKey(2);assert.equal((await controls()).liveEnabled,false);await wait(async()=>!(await frame.locator('#liveSwitch').isChecked()),'Live switch reflects dock off');
    await runKey(2);assert.equal((await controls()).liveEnabled,true);await wait(()=>frame.locator('#liveSwitch').isChecked(),'Live switch reflects dock on');
    const order=(await controls()).themes.map(theme=>theme.id);const at=order.indexOf(themes[0]);assert.ok(at>=0);
    await runKey(3);assert.equal((await controls()).currentThemeId,order[(at+1)%order.length]);
    await runKey(4);assert.equal((await controls()).currentThemeId,themes[0]);
    await runKey(5);assert.equal((await controls()).currentThemeId,themes[1]);assert.equal((await controls()).embeddedThemeId,embedded,'live selection never changes Embedded');
    const selectedName=(await controls()).themes.find(theme=>theme.id===themes[1]).name;await wait(async()=>(await frame.locator('#screenTitle').textContent()).includes(selectedName),'embedded selected-theme title follows dock selection');

    // Kill only the disposable requesting process after host admission. Its
    // pending target operation must remain busy and finish once, without replay.
    const oldDock=(await dock()).worker.pid,coolerPid=(await cooler()).pid;
    await press(1);await wait(async()=>(await dock()).worker.crossActionBusy,'crash fixture host admission');process.kill(oldDock);
    assert.equal(await quit(),2,'requester crash cannot retire target operation');await assert.rejects(call('cooler_configure',{enabled:false}),'requester crash cannot release target-disable guard');await assert.rejects(call('dock_configure',{patch:{enabled:false}}),'requester crash cannot retire originating action admission');
    await wait(async()=>{const d=await dock();return d.state==='connected'&&d.worker.pid!==oldDock&&!d.worker.crossActionBusy;},'dock replacement and original target completion');
    await settled();state=await controls();assert.equal(state.brightness.requested,45,'admitted relative change executes exactly once');assert.equal((await cooler()).pid,coolerPid);assert.equal((await dock()).actions,0,'replacement replays no prior action');
    await runKey(1);assert.equal((await controls()).brightness.requested,55,'fresh press works after requester recovery');
    const survivingDock=(await dock()).worker.pid;process.kill(coolerPid);
    await wait(async()=>{const c=await cooler();return c.state==='running'&&!c.busy&&c.pid!==coolerPid;},'target crash recovery');
    assert.equal((await dock()).worker.pid,survivingDock,'target crash leaves requester intact');assert.equal((await dock()).actions,1,'target restart replays no dock action');
    await runKey(0);assert.equal((await controls()).brightness.requested,35,'fresh explicit action works after target recovery');
    await coolerCall('live_set',{patch:{inApp:false}});await settled();await call('cooler_configure',{enabled:false});
    const errorsBefore=(await dock()).errors;await runKey(0);assert.ok((await dock()).errors>errorsBefore,'disabled peer produces key failure');assert.equal((await cooler()).enabled,false,'action never silently enables disabled target');
    assert.deepEqual(errors,[]);await wait(async()=>await quit()===0,'safe final Quit');assert.deepEqual(await bounded(exited,'host exit',12000),{code:0,signal:null});
    const rawEnv={...env};delete rawEnv.UC_COOLER_TEST_ACTION_DELAY_MS;
    await callbackDrain(exe,rawEnv,home,snapshot,false);await callbackDrain(exe,rawEnv,home,snapshot,true);
    console.log('PASS: actual fake dock button to host to cooler IPC; duplicate press deduplication; named brightness/Live/theme changes; iframe Live/theme/brightness reflection; Embedded preserved; admitted callback busy survives requester crash; no action replay across either worker restart; disabled target rejection; correlated completion during Shutdown and EOF cancellation; cooperative Quit. Fake devices only.');console.log(`Isolated evidence: ${home}`);
  }finally{
    if(child.exitCode===null){for(let attempt=0;attempt<30&&child.exitCode===null;attempt++){if(await quit().catch(()=>-1)===0)break;await delay(200);}await Promise.race([exited,delay(12000)]);}
    if(browser)await browser.close();
  }
})().catch(error=>{console.error(error);process.exitCode=1;});
