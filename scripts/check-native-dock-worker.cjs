// Actual host/child IPC and crash isolation. Disposable debug app; fake devices,
// fake native actions, inactive accounts and an invisible WebView only.
'use strict';
const {chromium}=require(process.env.UC_PLAYWRIGHT_MODULE||'playwright');
const {spawn}=require('node:child_process');
const fs=require('node:fs');
const path=require('node:path');
const os=require('node:os');
const net=require('node:net');
const readline=require('node:readline');
const assert=require('node:assert/strict');
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));

async function bounded(promise,label,timeout=15000){
  let timer;
  try{return await Promise.race([promise,new Promise((_,reject)=>{timer=setTimeout(()=>reject(Error(`${label} timed out`)),timeout);})]);}
  finally{clearTimeout(timer);}
}
async function wait(check,label,timeout=20000){
  const deadline=Date.now()+timeout;
  while(Date.now()<deadline){if(await bounded(Promise.resolve().then(check),label,Math.max(1,deadline-Date.now())))return;await delay(35);}
  throw Error(`${label} did not become true`);
}
function debugExecutable(value,name){
  const file=fs.realpathSync(path.resolve(value||''));
  if(!file.toLowerCase().includes(`${path.sep}debug${path.sep}`)||path.basename(file).toLowerCase()!==name||!fs.statSync(file).isFile()){
    throw Error(`Explicit debug ${name} required; release/installed executables refused`);
  }
  return file;
}
function pidOf(state){return state.worker?.pid;}
function isBusy(state){return state.busy===true||state.worker?.busy===true;}
function profileBytes(home){
  const folder=path.join(home,'profiles');
  return Object.fromEntries(fs.readdirSync(folder).filter(name=>name.endsWith('.json')).sort().map(name=>[name,fs.readFileSync(path.join(folder,name),'utf8')]));
}
async function checkParentPipeLoss(exe,env,home,snapshot){
  const worker=spawn(exe,['--dock-worker'],{env:{...env,UC_DOCK_WORKER:'1'},windowsHide:true,stdio:['pipe','pipe','ignore']});
  const exited=new Promise((resolve,reject)=>{worker.once('exit',(code,signal)=>resolve({code,signal}));worker.once('error',reject);});
  const lines=readline.createInterface({input:worker.stdout});
  const pending=new Map();let next=1,last;
  lines.on('line',line=>{
    const message=JSON.parse(line);
    if(message.type==='status')last=message;
    else if(message.type==='reply'){
      const reply=pending.get(message.id);pending.delete(message.id);
      if(reply)Object.hasOwn(message.result,'Ok')?reply.resolve(message.result.Ok):reply.reject(Error(message.result.Err));
    }
  });
  const call=request=>{
    const id=next++;
    const reply=new Promise((resolve,reject)=>pending.set(id,{resolve,reject}));
    worker.stdin.write(`${JSON.stringify({version:1,id,request})}\n`);
    return bounded(reply,'standalone fake worker reply');
  };
  try{
    await call({op:'init',state:snapshot,home:path.join(home,'parent-loss'),resources:path.resolve(__dirname,'../src-tauri/resources'),fake:true});
    await call({op:'enabled',enabled:true});
    await wait(()=>last?.status.state==='connected'&&last.status.framesSent>=14,'parent-loss fixture initial render');
    await call({op:'button',index:0,pressed:true});
    await wait(()=>last?.busy===true,'parent-loss fixture held fake action');
    const closedAt=Date.now();worker.stdin.end();
    await delay(750);
    assert.equal(worker.exitCode,null,'parent pipe loss drains an active action instead of truncating it');
    assert.deepEqual(await bounded(exited,'parent-loss cooperative worker exit',7000),{code:0,signal:null});
    assert.ok(Date.now()-closedAt>=2000,'held fake action completed its bounded delay before worker exit');
  }finally{
    worker.stdin.end();
    await bounded(exited,'parent-loss fixture cleanup',10000);
    lines.close();
  }
}

(async()=>{
  const exe=debugExecutable(process.env.UC_TEST_EXE,'unified-control.exe');
  const coolerExe=debugExecutable(process.env.UC_COOLER_WORKER_EXE,'uc-cooler-worker.exe');
  const home=fs.mkdtempSync(path.join(os.tmpdir(),'uc-native-dock-worker-'));
  const port=await new Promise(resolve=>{const server=net.createServer();server.listen(0,'127.0.0.1',()=>{const port=server.address().port;server.close(()=>resolve(port));});});
  const env={...process.env,UC_HOME:home,UC_TEST_QUIET:'1',UC_MULTI:'1',UC_FAKE_DEVICE:'1',UC_COOLER_WORKER_EXE:coolerExe,
    UC_DOCK_TEST_ACTION_DELAY_MS:'3000',WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:`--remote-debugging-port=${port}`};
  // A caller's timed tray hooks must not interrupt this independent fixture.
  delete env.UC_TEST_TRAY;delete env.UC_TEST_TRAY_QUIT_MS;delete env.UC_TEST_TRAY_ICON;
  const child=spawn(exe,[],{env,windowsHide:true,stdio:['ignore','pipe','pipe']});
  const exited=new Promise((resolve,reject)=>{child.once('exit',(code,signal)=>resolve({code,signal}));child.once('error',reject);});
  let logs='',browser,page;
  const capture=data=>{logs=(logs+data).slice(-512*1024);};child.stdout.on('data',capture);child.stderr.on('data',capture);
  async function request(args,input){
    const proxy=spawn(exe,args,{env,windowsHide:true,stdio:['pipe','pipe','pipe']});
    let stdout='';proxy.stdout.on('data',data=>{stdout+=data;});proxy.stderr.resume();
    const result=new Promise((resolve,reject)=>{proxy.once('error',reject);proxy.once('exit',code=>resolve({code,stdout}));});
    proxy.stdin.end(input===undefined?'':JSON.stringify(input));
    return bounded(result,'request-only proxy');
  }
  const call=(command,args={})=>bounded(page.evaluate(async({command,args})=>{
    try{return await window.__TAURI_INTERNALS__.invoke(command,args);}
    catch(error){throw Error(JSON.stringify(error));}
  },{command,args}),`invoke ${command}`);
  const dock=()=>call('dock_state');
  const cooler=()=>call('cooler_state');
  const coolerCall=(command,args={})=>call('cooler_call',{command,args});
  const agent=async(method,params={})=>{
    const response=await request(['--agent-request'],{method,params});
    const value=JSON.parse(response.stdout);assert.equal(value.ok,true,JSON.stringify(value));return value.result;
  };
  const press=async(index,states=[true,false])=>{for(const pressed of states)await call('dock_test_button',{index,pressed});};
  const errors=[];
  try{
    await wait(async()=>{if(child.exitCode!==null)throw Error(`Quiet host exited: ${logs}`);try{return(await fetch(`http://127.0.0.1:${port}/json/version`)).ok;}catch{return false;}},'quiet WebView endpoint');
    browser=await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
    await wait(()=>{page=browser.contexts().flatMap(context=>context.pages()).find(page=>/tauri/.test(page.url()));return!!page;},'host WebView');
    page.on('pageerror',error=>errors.push(error.message));
    await wait(()=>page.evaluate(()=>document.getElementById('storage-status')?.textContent==='Changes saved in Unified Control'),'initial editor load');
    await call('settings_update',{patch:{agentAccess:true}});
    const initial=(await call('profiles_state')).state;
    const firstPage=initial.profiles[0].pages[0];
    firstPage.keys[0].action='media';firstPage.keys[0].value='Play / pause';
    firstPage.keys[0].a.text='Worker fixture A';firstPage.keys[0].b.text='Worker fixture B';
    await call('profiles_save',{snapshot:initial,expectedRevision:(await call('profiles_state')).revision});
    await call('dock_configure',{patch:{enabled:true,brightness:42}});
    await wait(async()=>{const state=await dock();return state.state==='connected'&&state.framesSent>=14&&Number.isInteger(pidOf(state));},'dock child initial delivery');
    const before=await dock(),dockPid=pidOf(before);
    assert.notEqual(dockPid,child.pid,'dock must run outside the Tauri host');
    assert.equal(before.fake,true);assert.equal(before.usageReaderRunning,false,'fake child never starts account readers');
    await call('cooler_configure',{enabled:true});
    await wait(async()=>{const state=await cooler();return state.state==='running'&&!state.busy;},'cooler child ready');
    await coolerCall('live_set',{patch:{inApp:true,music:false,fps:30}});
    await wait(async()=>(await coolerCall('app_state')).live.status.measuredFps>0,'cooler fake Live frames');
    const coolerBefore=await cooler();assert.notEqual(coolerBefore.pid,dockPid);assert.notEqual(coolerBefore.pid,child.pid);

    // Holding the first fake action opens a deterministic busy interval. No real
    // Windows input/audio/display operation is permitted by NativeActions(fake).
    await press(0,[true,true,false]);
    await wait(async()=>isBusy(await dock()),'held fake dock action');
    assert.equal((await request(['--quit'])).code,2,'Quit refuses an active dock action');
    assert.equal(child.exitCode,null);
    await assert.rejects(call('dock_configure',{patch:{enabled:false}}),'disable refuses an active dock action');
    assert.equal((await dock()).enabled,true,'refused disable preserves runtime enablement');
    assert.equal(JSON.parse(fs.readFileSync(path.join(home,'settings.json'),'utf8')).dock.enabled,true,'refused disable preserves saved setting');
    await wait(async()=>{const state=await dock();return!isBusy(state)&&state.actions===1;},'single deduplicated action');
    const durable=profileBytes(home);

    // Kill only the PID returned by this disposable fake app. A crash must not
    // retain a delivered-page claim or restart/replay an action in the new child.
    process.kill(dockPid);
    await wait(async()=>{const state=await dock();return state.state!=='connected'&&!state.currentPage;},'lost dock pipe invalidates delivered-page state',10000);
    const unavailable=await dock();assert.equal(unavailable.usageReaderRunning,false);
    assert.equal((await agent('get_status')).cooler.state,'running','MCP remains available during dock recovery');
    assert.equal((await agent('get_layout')).pageId,initial.currentPage,'host layout survives dock crash');
    assert.equal((await cooler()).pid,coolerBefore.pid,'dock crash does not restart cooler');
    assert.equal((await coolerCall('app_state')).live.status.running,true,'cooler Live survives dock crash');
    assert.deepEqual(profileBytes(home),durable,'worker crash does not write profile files');
    const latest=await call('profiles_state'),snapshot=latest.state;
    const next=JSON.parse(JSON.stringify(snapshot.profiles[0].pages[0]));
    next.id='page-worker-recovery';next.name='Worker recovery';
    for(const key of next.keys){key.action='none';key.value='';}
    next.keys[0].a.text='Fresh after crash';next.keys[0].b.text='Fresh after crash';
    snapshot.profiles[0].pages.push(next);snapshot.currentPage=next.id;
    await call('profiles_save',{snapshot,expectedRevision:latest.revision});
    await wait(async()=>{const state=await dock();return state.state==='connected'&&pidOf(state)!==dockPid&&state.currentPage===next.id&&state.framesSent>=14;},'fresh latest layout delivered by replacement child');
    const replacement=await dock();assert.equal(replacement.actions,0,'replacement never replays old action/input queues');
    assert.equal(replacement.usageReaderRunning,false);
    const afterRecovery=profileBytes(home);await delay(350);
    assert.deepEqual(profileBytes(home),afterRecovery,'worker replica never writes host profile storage');

    process.kill(coolerBefore.pid);
    await wait(async()=>{const state=await cooler();return state.state==='running'&&!state.busy&&state.pid!==coolerBefore.pid;},'cooler replacement child');
    assert.equal((await dock()).state,'connected','cooler crash leaves dock connected');
    assert.equal(pidOf(await dock()),pidOf(replacement),'cooler crash never restarts dock child');
    await wait(async()=>(await coolerCall('app_state')).live.status.measuredFps>0,'cooler Live restores after its own crash');
    assert.equal((await agent('get_layout')).pageId,next.id,'MCP reads latest durable profile after both recoveries');
    await page.locator('.nav-item[data-page="general"]').click();
    await page.locator('.nav-item[data-page="general"][aria-current="page"]').waitFor({timeout:2000});
    assert.equal(child.exitCode,null,'editor remains usable after either worker crashes');

    await wait(async()=>!isBusy(await dock()),'dock idle before disable');
    // A cached idle report is not an admission lease: a new render may start
    // before configure reaches the child. Retry only its explicit busy refusal.
    await wait(async()=>{
      try{await call('dock_configure',{patch:{enabled:false}});return true;}
      catch(error){if(!error.message.includes('Dock is busy; try again when idle'))throw error;return false;}
    },'idle dock disable admission',7000);
    await wait(async()=>(await dock()).state==='disabled','dock disabled');
    const disabled=await dock();await delay(700);
    const stillDisabled=await dock();assert.equal(stillDisabled.enabled,false);assert.equal(stillDisabled.usageReaderRunning,false);
    assert.equal(stillDisabled.framesSent,disabled.framesSent,'disabled dock does not send new frames');
    assert.equal((await coolerCall('app_state')).live.status.running,true,'dock disable does not stop cooler Live');
    await coolerCall('live_set',{patch:{inApp:false}});
    await wait(async()=>!(await cooler()).busy,'cooler idle before shutdown');
    const shutdownPids=[pidOf(stillDisabled),(await cooler()).pid].filter(Boolean);
    assert.deepEqual(errors,[]);
    assert.equal((await request(['--quit'])).code,0,'idle Quit completes cooperatively');
    assert.deepEqual(await bounded(exited,'quiet host exit',12000),{code:0,signal:null});
    for(const pid of shutdownPids)await wait(()=>{try{process.kill(pid,0);return false;}catch(error){if(error.code!=='ESRCH')throw error;return true;}},'worker exits with parent',5000);
    await checkParentPipeLoss(exe,env,home,initial);
    console.log('PASS: actual separate dock child; fake action deduplication; active-action Quit/disable refusal; durable host-only profiles; immediate lost-pipe status invalidation; fresh latest-layout crash recovery without action replay; both-way worker crash isolation; cooler Live continuity/recovery; request-only MCP/editor availability; disabled-source inactivity, cooperative child shutdown and parent-pipe-loss action draining. Fake devices only.');
    console.log(`Isolated evidence: ${home}`);
  }finally{
    if(child.exitCode===null){await delay(3500);await request(['--quit']).catch(()=>{});await Promise.race([exited,delay(12000)]);}
    if(browser)await browser.close();
  }
})().catch(error=>{console.error(error);process.exitCode=1;});
