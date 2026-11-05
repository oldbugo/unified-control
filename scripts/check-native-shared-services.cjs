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
const selected=state=>(state.providers||[]).filter(provider=>provider.selected);
const verified=state=>selected(state).length===2&&selected(state).every(provider=>provider.code==='verified');
function noLocalReaders(state){assert.equal(state.dock.local_reader_starts,0,'dock must not spawn an account helper');assert.equal(state.usage.readerStarts,0,'synthetic fixture never starts a real account reader');assert.equal(state.cooler.local_music_starts,0,'cooler must not start a replaced local music watcher');assert.equal(state.cooler.local_weather_fetches,0,'cooler must not start a replaced local forecast fetcher');}

(async()=>{
  const exe=debugExecutable(process.env.UC_TEST_EXE,'unified-control.exe'),worker=debugExecutable(process.env.UC_COOLER_WORKER_EXE,'uc-cooler-worker.exe');
  const home=fs.mkdtempSync(path.join(os.tmpdir(),'uc-native-shared-services-'));
  const port=await new Promise(resolve=>{const server=net.createServer();server.listen(0,'127.0.0.1',()=>{const value=server.address().port;server.close(()=>resolve(value));});});
  const env={...process.env,UC_HOME:home,UC_TEST_QUIET:'1',UC_MULTI:'1',UC_FAKE_DEVICE:'1',UC_COOLER_WORKER_EXE:worker,WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:`--remote-debugging-port=${port}`};
  for(const key of ['UC_TEST_TRAY','UC_TEST_TRAY_QUIT_MS','UC_TEST_TRAY_ICON','UC_DOCK_TEST_ACTION_DELAY_MS'])delete env[key];
  const child=spawn(exe,[],{env,windowsHide:true,stdio:['ignore','pipe','pipe']}),exited=new Promise((resolve,reject)=>{child.once('exit',(code,signal)=>resolve({code,signal}));child.once('error',reject);});
  let browser,page,logs='';const capture=data=>{logs=(logs+data).slice(-512*1024);};child.stdout.on('data',capture);child.stderr.on('data',capture);
  const call=(command,args={})=>bounded(page.evaluate(async({command,args})=>{try{return await window.__TAURI_INTERNALS__.invoke(command,args);}catch(error){throw Error(JSON.stringify(error));}},{command,args}),`invoke ${command}`);
  const control=patch=>call('services_test_control',{patch}),read=()=>control({op:'read'});
  const cooler=()=>call('cooler_state'),dock=()=>call('dock_state'),coolerCall=(command,args={})=>call('cooler_call',{command,args});
  const quit=()=>bounded(new Promise((resolve,reject)=>{const request=spawn(exe,['--quit'],{env,windowsHide:true,stdio:'ignore'});request.once('error',reject);request.once('exit',resolve);}),'request-only Quit');
  const sourceControl=(failure=false,paused=false)=>control({op:'sources',failure,paused});
  const usageControl=(mode='ok',paused=false)=>control({op:'usage',mode,paused});
  const errors=[];
  try{
    await wait(async()=>{if(child.exitCode!==null)throw Error(`Quiet app exited: ${logs}`);try{return(await fetch(`http://127.0.0.1:${port}/json/version`)).ok;}catch{return false;}},'quiet WebView');
    browser=await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
    await wait(()=>{page=browser.contexts().flatMap(context=>context.pages()).find(page=>/tauri/.test(page.url()));return!!page;},'UC page');
    page.on('pageerror',error=>errors.push(error.message));
    await wait(()=>page.evaluate(()=>document.getElementById('storage-status')?.textContent==='Changes saved in Unified Control'),'initial saved profiles');
    await assert.rejects(control({op:'sources',failure:true,paused:false,extra:true}),'test controls reject unknown fields');
    await assert.rejects(usageControl('unsupported'),'test controls reject unknown source modes');
    const cold=await read();assert.equal(cold.sources.process_starts,0);assert.equal(cold.sources.running,false);assert.equal(cold.usage.readerStarts,0);
    await delay(1050);assert.equal((await read()).sources.process_starts,0,'cold empty demand starts no provider process');

    // Two providers and repeated presentations share one logical account owner;
    // the actual Node reader count remains zero in this synthetic instance.
    const profiles=await call('profiles_state'),keys=profiles.state.profiles[0].pages[0].keys;
    for(const [index,kind,a,b]of[[0,'codex','rings','weekly-number'],[1,'codex','rings','tokens'],[2,'claude','rings','five-hour-number']]){
      keys[index].a={kind,text:'',usageView:a};keys[index].b={kind,text:'',usageView:b};
    }
    await call('profiles_save',{snapshot:profiles.state,expectedRevision:profiles.revision});
    await call('dock_configure',{patch:{enabled:true}});
    await wait(async()=>verified((await read()).dock),'both shared account providers verified');
    const ai=await read(),owner=ai.usage.ownerGeneration,epochs=ai.usage.sourceEpochs;
    assert.equal(ai.usage.activeProviders,2);assert.equal(epochs,1);assert.ok(ai.usage.specCount>=5);assert.equal(ai.dock.local_reader_starts,0);
    const previewFaces=keys.map(key=>key.a);
    await wait(async()=>{const frames=await call('key_previews',{faces:previewFaces,clock:'12:34'});return[0,1,2].every(index=>frames[index].unknown===false);},'shared account preview frames');

    await call('cooler_configure',{enabled:true});
    await wait(async()=>{const state=await cooler();return state.state==='running'&&!state.busy;},'cooler worker ready');
    const png=Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScLbtAAAAABJRU5ErkJggg==','base64');
    const imported=await coolerCall('import_bytes',{name:'shared-services.png',bytes:[...png]});assert.ok(imported.id);
    const standard=await coolerCall('theme_standard');
    const activeTheme={...standard,widgets:standard.widgets.filter(widget=>widget.type==='clock'||widget.type==='music')};
    activeTheme.widgets.push({id:'sourceWeather',type:'weather',x:0.62,y:0.06,w:0.35,h:0.87,props:{unit:'C'}});
    const inactiveTheme={...standard,widgets:standard.widgets.filter(widget=>widget.type==='clock')};
    await coolerCall('theme_set',{originalId:imported.id,theme:inactiveTheme});await coolerCall('theme_use',{originalId:imported.id});
    await coolerCall('live_set',{patch:{inApp:true,music:false,fps:30}});
    await delay(1100);assert.equal((await read()).sources.process_starts,0,'Live without source widgets starts no source worker');
    await coolerCall('theme_set',{originalId:imported.id,theme:activeTheme});
    await wait(async()=>{const state=await read();return state.sources.running&&state.cooler.music_present;},'music widget starts one provider-only process');
    const musicOnly=await read();assert.equal(musicOnly.sources.source.fake,true);assert.equal(musicOnly.sources.source.music_starts,1);assert.equal(musicOnly.sources.source.weather_fetches,0,'weather widget without selected place makes no request');
    const firstPlace={name:'Synthetic One',lat:1,lon:2,tz:'UTC'},secondPlace={name:'Synthetic Two',lat:3,lon:4,tz:'UTC'};
    await coolerCall('weather_set_place',{place:firstPlace});
    await wait(async()=>{const state=await read();return state.cooler.music_present&&state.cooler.weather_present;},'both injected media/weather values');
    const both=await read(),sourcePid=both.sources.pid;assert.equal(both.sources.process_starts,1);assert.equal(both.sources.source.weather_fetches,1);noLocalReaders(both);
    assert.notEqual(sourcePid,(await cooler()).pid,'provider-only process is separate from cooler device worker');
    assert.notEqual(sourcePid,(await dock()).worker.pid,'provider process is separate from dock worker');

    // Removing widgets stops native interests while retaining bounded in-memory
    // forecast/retry state. Readding the same place must not repeat HTTP.
    await coolerCall('theme_set',{originalId:imported.id,theme:inactiveTheme});
    await wait(async()=>{const state=await read();return state.sources.state==='idle'&&state.sources.source.music_active===false&&state.sources.source.weather_active===false;},'warm process with inactive source interests');
    await delay(1100);const idle=await read();await delay(1100);const idleLater=await read();
    assert.equal(idleLater.sources.source.music_reads,idle.sources.source.music_reads,'idle process makes no media reads');
    assert.equal(idleLater.sources.source.weather_fetches,idle.sources.source.weather_fetches,'idle process makes no forecast requests');
    assert.equal(idleLater.sources.pid,sourcePid,'idle cache remains in the existing owned process');
    await coolerCall('theme_set',{originalId:imported.id,theme:activeTheme});
    await wait(async()=>{const state=await read();return state.cooler.music_present&&state.cooler.weather_present;},'reactivated cached sources');
    assert.equal((await read()).sources.source.weather_fetches,1,'same-place cache survives quick off/on');

    for(const mode of['error','stale','logout','unverified']){
      await usageControl(mode);
      await wait(async()=>{const state=(await read()).dock;return selected(state).length===2&&selected(state).every(provider=>provider.code!=='verified');},`${mode} suppresses account numbers`);
      const frames=await call('key_previews',{faces:previewFaces,clock:'12:34'});assert.ok([0,1,2].every(index=>frames[index].unknown),`${mode} suppresses numeric preview pixels`);
    }
    await usageControl();await wait(async()=>verified((await read()).dock),'account recovery');
    await usageControl('ok',true);
    await wait(async()=>{const state=await read();return!state.dock.fresh&&!verified(state.dock)&&state.cooler.music_present&&state.cooler.weather_present;},'account delivery expiry leaves music/weather fresh',9000);
    assert.equal((await read()).usage.sourceEpochs,epochs,'delivery pause does not restart the shared account owner');
    await usageControl();await wait(async()=>verified((await read()).dock),'account delivery recovery');
    await sourceControl(false,true);
    await wait(async()=>{const state=await read();return!state.cooler.fresh&&!state.cooler.music_present&&!state.cooler.weather_present&&verified(state.dock);},'source transport expiry leaves AI fresh',9000);
    await sourceControl();await wait(async()=>{const state=await read();return state.cooler.music_present&&state.cooler.weather_present;},'source transport recovery');
    await sourceControl(true,false);
    await wait(async()=>{const state=await read();return!state.cooler.music_present&&state.cooler.weather_present&&state.cooler.weather_status==='stale';},'failed reads clear music and retain only explicit same-place stale weather');
    await coolerCall('weather_set_place',{place:secondPlace});
    await wait(async()=>!(await read()).cooler.weather_present,'new place never reuses the prior city forecast');
    await sourceControl();await wait(async()=>{const state=await read();return state.cooler.music_present&&state.cooler.weather_present;},'changed-place recovery');

    const beforeCrash=await read(),coolerPid=(await cooler()).pid,dockPid=(await dock()).worker.pid;
    process.kill(beforeCrash.sources.pid);
    await wait(async()=>{const state=await read();return state.sources.pid&&state.sources.pid!==beforeCrash.sources.pid&&state.cooler.music_present&&state.cooler.weather_present;},'provider-only process crash recovery');
    assert.equal((await cooler()).pid,coolerPid,'source crash leaves cooler engine intact');assert.equal((await dock()).worker.pid,dockPid,'source crash leaves dock intact');
    assert.equal((await read()).usage.ownerGeneration,owner,'source crash leaves account owner intact');
    const survivingSource=(await read()).sources.pid;
    process.kill(coolerPid);
    await wait(async()=>{const value=await cooler();return value.state==='running'&&!value.busy&&value.pid!==coolerPid;},'cooler crash recovery');
    await wait(async()=>{const state=await read();return state.cooler.music_present&&state.cooler.weather_present;},'replacement cooler receives shared source observations');
    assert.equal((await read()).sources.pid,survivingSource,'cooler recovery preserves the existing provider process/cache');
    process.kill(dockPid);
    await wait(async()=>{const value=await dock();return value.state==='connected'&&value.worker.pid!==dockPid;},'dock crash recovery');
    await wait(async()=>verified((await read()).dock),'replacement dock uses the shared account owner');
    const recovered=await read();assert.equal(recovered.usage.ownerGeneration,owner);assert.equal(recovered.usage.sourceEpochs,epochs);noLocalReaders(recovered);
    await coolerCall('theme_set',{originalId:imported.id,theme:inactiveTheme});
    await wait(async()=>(await read()).sources.state==='idle','final idle source interests');
    assert.deepEqual(errors,[]);
    const ownedSource=(await read()).sources.pid;
    await wait(async()=>{const code=await quit();if(code===0)return true;assert.equal(code,2,'only busy refusal may defer Quit');return false;},'cooperative safe Quit',15000);
    assert.deepEqual(await bounded(exited,'host exit',12000),{code:0,signal:null});
    await wait(()=>{try{process.kill(ownedSource,0);return false;}catch(error){if(error.code!=='ESRCH')throw error;return true;}},'owned provider process exits on Quit',5000);
    console.log('PASS: shared account owner and provider-only media/weather process; activation/no-place gates; no local duplicate readers; original account failure/expiry suppression; independent delivery expiry/recovery; matching-place stale weather; changed-place isolation; warm idle cache/read silence and no repeated HTTP; source/cooler/dock crash isolation; cooperative Quit. Synthetic sources and fake devices only.');
    console.log(`Isolated evidence: ${home}`);
  }finally{
    if(child.exitCode===null){if(page&&!page.isClosed()){await usageControl().catch(()=>{});await sourceControl().catch(()=>{});}for(let attempt=0;attempt<20&&child.exitCode===null;attempt++){if(await quit().catch(()=>-1)===0)break;await delay(100);}await Promise.race([exited,delay(12000)]);}
    if(browser)await browser.close();
  }
})().catch(error=>{console.error(error);process.exitCode=1;});
