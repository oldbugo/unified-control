// Actual native host and both workers, synthetic hardware sources only.
// Never opens installed apps, user profiles, accounts or real device handles.
'use strict';
const {chromium}=require(process.env.UC_PLAYWRIGHT_MODULE||'playwright');
const {spawn}=require('node:child_process');
const fs=require('node:fs');
const path=require('node:path');
const os=require('node:os');
const net=require('node:net');
const assert=require('node:assert/strict');
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
async function bounded(promise,label,timeout=15000){
  let timer;
  try{return await Promise.race([promise,new Promise((_,reject)=>{timer=setTimeout(()=>reject(Error(`${label} timed out`)),timeout);})]);}
  finally{clearTimeout(timer);}
}
async function wait(check,label,timeout=20000){
  const deadline=Date.now()+timeout;
  while(Date.now()<deadline){if(await bounded(Promise.resolve().then(check),label,Math.max(1,deadline-Date.now())))return;await delay(60);}
  throw Error(`${label} did not become true`);
}
function debugExecutable(value,name){
  const file=fs.realpathSync(path.resolve(value||''));
  if(!file.toLowerCase().includes(`${path.sep}debug${path.sep}`)||path.basename(file).toLowerCase()!==name||!fs.statSync(file).isFile())throw Error(`Explicit debug ${name} required; installed/release executables refused`);
  return file;
}
function stateOf(value){return value?.state&&typeof value.state==='object'?value.state:value;}
function consumers(value){return{dock:stateOf(value.dock),cooler:stateOf(value.cooler)};}
function assertNoLocalReaders(value){
  const {dock,cooler}=consumers(value);
  assert.equal(dock.shared,true,'dock must use the injected shared source');
  assert.equal(cooler.shared,true,'cooler must use the injected shared source');
  assert.equal(dock.local_polls,0,'dock must not run its replaced local hardware sampler');
  assert.equal(cooler.local_legacy_polls,0,'cooler legacy sampler must not read native hardware');
  assert.equal(cooler.local_metric_polls,0,'cooler metrics hub must not duplicate native CPU/RAM/GPU reads');
}
function assertRichSnapshot(snapshot){
  assert.equal(snapshot.version,1);assert.ok(snapshot.generation>0&&snapshot.sequence>0);
  assert.ok(snapshot.observed_at_ms<=Date.now());
  assert.ok(snapshot.expires_at_ms>snapshot.observed_at_ms&&snapshot.expires_at_ms<=snapshot.observed_at_ms+3000);
  assert.ok(snapshot.expires_at_ms>Date.now(),'source keeps the original unexpired observation time');
  assert.ok(Number.isFinite(snapshot.cpu_load_pct));
  assert.ok(snapshot.ram_total_bytes>0&&snapshot.ram_used_bytes<=snapshot.ram_total_bytes);
  const gpu=snapshot.gpu;assert.ok(gpu&&typeof gpu.name==='string'&&gpu.name.length>0);
  for(const key of ['temp_c','load_pct','clock_mhz','mem_used_bytes','mem_total_bytes','power_w','fan_pct'])assert.ok(Number.isFinite(gpu[key]),`rich GPU field ${key} is present`);
}

// This uses only ordinary cooler theme/Live operations against the fake worker.
// The concrete fixture is defined with the worker implementation's existing API.
async function activateCoolerHardware(coolerCall){
  const png=Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScLbtAAAAABJRU5ErkJggg==','base64');
  const imported=await coolerCall('import_bytes',{name:'shared-hardware.png',bytes:[...png]});
  assert.ok(imported.id,'synthetic image produces a theme fixture');
  const theme=await coolerCall('theme_standard');
  assert.ok(theme.widgets.some(widget=>widget.type==='stats'),'legacy hardware stats participate in this fixture');
  // Disable music so this hardware-only check starts no media session reader.
  theme.widgets=theme.widgets.filter(widget=>widget.type!=='music');
  theme.widgets.push({id:'sharedGpu4Hz',type:'sensor',x:0.05,y:0.65,w:0.45,h:0.25,props:{metric:'gpu.load',style:'number',rate:4}});
  await coolerCall('theme_set',{originalId:imported.id,theme});
  await coolerCall('theme_use',{originalId:imported.id});
  await coolerCall('live_set',{patch:{inApp:true,music:false,fps:30}});
}

(async()=>{
  const exe=debugExecutable(process.env.UC_TEST_EXE,'unified-control.exe');
  const coolerExe=debugExecutable(process.env.UC_COOLER_WORKER_EXE,'uc-cooler-worker.exe');
  const home=fs.mkdtempSync(path.join(os.tmpdir(),'uc-native-shared-hardware-'));
  const port=await new Promise(resolve=>{const server=net.createServer();server.listen(0,'127.0.0.1',()=>{const port=server.address().port;server.close(()=>resolve(port));});});
  const env={...process.env,UC_HOME:home,UC_TEST_QUIET:'1',UC_MULTI:'1',UC_FAKE_DEVICE:'1',UC_COOLER_WORKER_EXE:coolerExe,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:`--remote-debugging-port=${port}`};
  for(const key of ['UC_TEST_TRAY','UC_TEST_TRAY_QUIT_MS','UC_TEST_TRAY_ICON','UC_DOCK_TEST_ACTION_DELAY_MS'])delete env[key];
  const child=spawn(exe,[],{env,windowsHide:true,stdio:['ignore','pipe','pipe']});
  const exited=new Promise((resolve,reject)=>{child.once('exit',(code,signal)=>resolve({code,signal}));child.once('error',reject);});
  let browser,page,logs='';
  const capture=data=>{logs=(logs+data).slice(-512*1024);};child.stdout.on('data',capture);child.stderr.on('data',capture);
  const call=(command,args={})=>bounded(page.evaluate(async({command,args})=>{try{return await window.__TAURI_INTERNALS__.invoke(command,args);}catch(error){throw Error(JSON.stringify(error));}},{command,args}),`invoke ${command}`);
  const hardware=patch=>call('hardware_test_control',{patch});
  const read=()=>hardware({op:'read'});
  const dock=()=>call('dock_state');
  const cooler=()=>call('cooler_state');
  const coolerCall=(command,args={})=>call('cooler_call',{command,args});
  const quit=()=>bounded(new Promise((resolve,reject)=>{const request=spawn(exe,['--quit'],{env,windowsHide:true,stdio:'ignore'});request.once('error',reject);request.once('exit',resolve);}),'request-only Quit');
  const disableDock=()=>wait(async()=>{try{await call('dock_configure',{patch:{enabled:false}});return true;}catch(error){if(!error.message.includes('Dock is busy; try again when idle'))throw error;return false;}},'idle dock disable',7000);
  const errors=[];
  try{
    await wait(async()=>{if(child.exitCode!==null)throw Error(`Quiet fake UC exited: ${logs}`);try{return(await fetch(`http://127.0.0.1:${port}/json/version`)).ok;}catch{return false;}},'quiet WebView ready');
    browser=await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
    await wait(()=>{page=browser.contexts().flatMap(context=>context.pages()).find(page=>/tauri/.test(page.url()));return!!page;},'quiet UC page');
    page.on('pageerror',error=>errors.push(error.message));
    await wait(()=>page.evaluate(()=>document.getElementById('storage-status')?.textContent==='Changes saved in Unified Control'),'initial profiles ready');
    await assert.rejects(hardware({op:'provider_failure',enabled:true,extra:true}),'test controls reject unknown fields before changing state');
    await assert.rejects(hardware({op:'pause_delivery',consumer:'other',enabled:true}),'test controls reject unknown consumers');
    const cold=await read();assert.equal(cold.collector.active,false);assert.equal(cold.collector.samples,0);
    assert.equal(cold.collector.testFailure,false,'invalid controls do not change provider state');
    await delay(1100);
    assert.deepEqual((await read()).collector.reads,cold.collector.reads,'no enabled consumer performs no hardware reads');
    const collectorGeneration=cold.collector.generation;
    const profiles=await call('profiles_state');
    profiles.state.profiles[0].pages[0].keys[0].a.kind='cpu';
    profiles.state.profiles[0].pages[0].keys[0].b.kind='cpu';
    await call('profiles_save',{snapshot:profiles.state,expectedRevision:profiles.revision});
    await call('dock_configure',{patch:{enabled:true}});
    await wait(async()=>{const value=await read();return value.collector.active&&value.collector.demands.dock?.cpu&&consumers(value).dock.cpu_present;},'CPU-only dock demand and fresh injected value');
    const dockOnly=await read();
    assert.equal(dockOnly.collector.hz,1);assert.equal(dockOnly.collector.reads.ram,0);assert.equal(dockOnly.collector.reads.gpu,0);

    // Sensor interest follows the delivered page, including physical folder
    // transitions which call the same runtime activation hook.
    const pageLayout=await call('profiles_state'),mainPage=pageLayout.state.currentPage;
    const idlePage=JSON.parse(JSON.stringify(pageLayout.state.profiles[0].pages[0]));
    idlePage.id='page-shared-idle';idlePage.name='No hardware';
    for(const key of idlePage.keys){key.a={kind:'text',text:''};key.b={kind:'text',text:''};key.action='none';key.value='';}
    pageLayout.state.profiles[0].pages.push(idlePage);pageLayout.state.currentPage=idlePage.id;
    await call('profiles_save',{snapshot:pageLayout.state,expectedRevision:pageLayout.revision});
    await wait(async()=>(await dock()).currentPage===idlePage.id&&!(await read()).collector.active,'hidden sensor page releases demand');
    const hiddenReads=(await read()).collector.reads;await delay(1100);
    assert.deepEqual((await read()).collector.reads,hiddenReads,'a sensor face on another page must not keep collection active');
    const restored=await call('profiles_state');restored.state.currentPage=mainPage;
    await call('profiles_save',{snapshot:restored.state,expectedRevision:restored.revision});
    await wait(async()=>(await read()).dock.cpu_present,'returning to hardware page restores fresh demand');

    await call('cooler_configure',{enabled:true});
    await wait(async()=>{const state=await cooler();return state.state==='running'&&!state.busy;},'cooler fake engine ready');
    await activateCoolerHardware(coolerCall);
    await wait(async()=>{const value=await read(),state=consumers(value).cooler;return value.collector.hz===4&&value.collector.demands.cooler?.gpu&&state.fresh&&state.cpu_present&&state.ram_present&&state.gpu_fields>0;},'union demand and both cooler hardware adapters');
    const both=await read();assertRichSnapshot(both.snapshot);assertNoLocalReaders(both);
    assert.equal(both.collector.generation,collectorGeneration,'two consumers use the original single collector');
    const richBits=consumers(both).cooler.gpu_fields;
    assert.equal(richBits,255,'native receiver preserves all rich GPU fields and name');
    const sampleStart=await read();await delay(1250);const sampleEnd=await read();
    const delta=sampleEnd.collector.samples-sampleStart.collector.samples;
    assert.ok(delta>=3&&delta<=7,`4Hz collection is coalesced across consumers (${delta} samples)`);
    for(const family of ['cpu','ram','gpu'])assert.equal(sampleEnd.collector.reads[family]-sampleStart.collector.reads[family],delta,`${family} is collected once per shared sample`);
    assertNoLocalReaders(sampleEnd);

    // A failed provider produces a new unknown sample; old numbers cannot be
    // merged into the replacement snapshot or revived by a local fallback.
    await hardware({op:'provider_failure',enabled:true});
    await wait(async()=>{const value=await read(),state=consumers(value);return value.collector.sourceFailures>0&&!state.dock.cpu_present&&!state.cooler.cpu_present&&!state.cooler.ram_present&&state.cooler.gpu_fields===0;},'provider failure clears both receiver values');
    assertNoLocalReaders(await read());
    await hardware({op:'provider_failure',enabled:false});
    await wait(async()=>{const state=consumers(await read());return state.dock.cpu_present&&state.cooler.cpu_present&&state.cooler.ram_present&&state.cooler.gpu_fields===richBits;},'provider recovery refreshes both workers');

    // Continue demand exchange but withhold samples from only one consumer.
    // Its original observation expires while its peer keeps receiving updates.
    for(const consumer of ['cooler','dock']){
      const before=await read(),oldSequence=consumers(before)[consumer].accepted_sequence;
      const pauseResult=await hardware({op:'pause_delivery',consumer,enabled:true});
      const lastPossibleSequence=pauseResult.collector.sequence;
      await wait(async()=>{const value=await read(),state=consumers(value),peer=consumer==='cooler'?'dock':'cooler';return!state[consumer].fresh&&!state[consumer].cpu_present&&state[peer].fresh&&state[peer].accepted_sequence>oldSequence;},`${consumer} expiry leaves peer fresh`,9000);
      const paused=await read();assertNoLocalReaders(paused);
      assert.ok(consumers(paused)[consumer].accepted_sequence<=lastPossibleSequence,'read/null exchanges never manufacture a newer observation');
      const expiredSequence=consumers(paused)[consumer].accepted_sequence;
      await delay(350);
      assert.equal(consumers(await read())[consumer].accepted_sequence,expiredSequence,'null exchanges do not renew an expired observation');
      if(consumer==='cooler'){assert.equal(consumers(paused).cooler.ram_present,false);assert.equal(consumers(paused).cooler.gpu_fields,0);}
      await hardware({op:'pause_delivery',consumer,enabled:false});
      await wait(async()=>{const state=consumers(await read())[consumer];return state.fresh&&state.cpu_present&&state.accepted_sequence>oldSequence;},`${consumer} fresh delivery recovery`);
    }

    const dockBefore=await dock(),coolerBefore=await cooler();
    process.kill(dockBefore.worker.pid);
    await wait(async()=>{const value=await dock();return value.state==='connected'&&value.worker.pid!==dockBefore.worker.pid;},'dock child recovers');
    await wait(async()=>consumers(await read()).dock.cpu_present,'replacement dock receives current shared sample');
    assert.equal((await cooler()).pid,coolerBefore.pid,'dock recovery leaves cooler child intact');
    assert.equal((await read()).collector.generation,collectorGeneration,'dock recovery never creates another collector');
    process.kill(coolerBefore.pid);
    await wait(async()=>{const value=await cooler();return value.state==='running'&&!value.busy&&value.pid!==coolerBefore.pid;},'cooler child recovers');
    await wait(async()=>{const value=await read();return consumers(value).cooler.cpu_present&&value.collector.hz===4;},'replacement cooler reinstates demand/current readings');
    assert.equal((await read()).collector.generation,collectorGeneration,'cooler recovery never creates another collector');
    assertNoLocalReaders(await read());

    await disableDock();
    await wait(async()=>{const value=await read();return!value.collector.demands.dock&&consumers(value).cooler.cpu_present;},'dock disable leaves cooler readings fresh');
    const continuing=(await read()).collector.sequence;
    await wait(async()=>(await read()).collector.sequence>continuing,'remaining cooler consumer keeps the collector active');
    await coolerCall('live_set',{patch:{inApp:false}});
    await wait(async()=>!(await cooler()).busy,'cooler idle before disable');
    await call('cooler_configure',{enabled:false});
    await wait(async()=>(await read()).collector.active===false,'last consumer removes all hardware demand');
    const idle=await read();await delay(1200);const idleEnd=await read();
    assert.deepEqual(idleEnd.collector.reads,idle.collector.reads,'no consumers means no reads after prior use');
    assert.equal(idleEnd.collector.gpuActive,false,'no retained GPU source after demand ends');
    assert.deepEqual(errors,[]);
    await wait(async()=>{const code=await quit();if(code===0)return true;assert.equal(code,2,'only busy refusal may defer safe Quit');return false;},'cooperative idle Quit',10000);
    assert.deepEqual(await bounded(exited,'quiet host exit',12000),{code:0,signal:null});
    console.log('PASS: actual host/two-worker shared hardware delivery; demand union/rate coalescing; zero local duplicate readers; rich GPU availability; provider failure/expiry unknowns and recovery; peer independence under paused delivery; worker restart without a second collector; one remaining consumer stays fresh; no consumers/no reads; cooperative Quit. Synthetic sources and fake devices only.');
    console.log(`Isolated evidence: ${home}`);
  }finally{
    if(child.exitCode===null){
      if(page&&!page.isClosed()){
        await hardware({op:'provider_failure',enabled:false}).catch(()=>{});
        for(const consumer of ['dock','cooler'])await hardware({op:'pause_delivery',consumer,enabled:false}).catch(()=>{});
      }
      for(let attempt=0;attempt<20&&child.exitCode===null;attempt++){if(await quit().catch(()=>-1)===0)break;await delay(100);}
      await Promise.race([exited,delay(12000)]);
    }
    if(browser)await browser.close();
  }
})().catch(error=>{console.error(error);process.exitCode=1;});
