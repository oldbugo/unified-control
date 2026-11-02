// Isolated debug WebView + actual inherited-pipe cooler engine. Fake devices only.
const {chromium}=require(process.env.UC_PLAYWRIGHT_MODULE||'playwright');
const {spawn}=require('node:child_process');
const fs=require('node:fs'),path=require('node:path'),os=require('node:os'),net=require('node:net'),assert=require('node:assert/strict');
const delay=ms=>new Promise(r=>setTimeout(r,ms));
(async()=>{
  const exe=path.resolve(process.env.UC_TEST_EXE||''),worker=path.resolve(process.env.UC_COOLER_WORKER_EXE||'');
  for(const file of [exe,worker])if(!file.toLowerCase().includes(`${path.sep}debug${path.sep}`)||!fs.existsSync(file))throw Error('Debug executables required; installed/release apps refused');
  const home=fs.mkdtempSync(path.join(os.tmpdir(),'uc-native-cooler-'));
  const port=await new Promise(resolve=>{const s=net.createServer();s.listen(0,'127.0.0.1',()=>{const p=s.address().port;s.close(()=>resolve(p));});});
  const env={...process.env,UC_HOME:home,UC_TEST_QUIET:'1',UC_MULTI:'1',UC_FAKE_DEVICE:'1',UC_COOLER_WORKER_EXE:worker,WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:`--remote-debugging-port=${port}`};
  const child=spawn(exe,[],{env,windowsHide:true,stdio:'ignore'});let browser;
  const exit=new Promise(resolve=>child.once('exit',resolve));
  const requestQuit=()=>new Promise((resolve,reject)=>{const p=spawn(exe,['--quit'],{env,windowsHide:true,stdio:'ignore'});p.once('error',reject);p.once('exit',resolve);});
  try{
    for(let n=0;n<100;n++){try{if((await fetch(`http://127.0.0.1:${port}/json/version`)).ok)break;}catch{}await delay(150);}
    browser=await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
    let page;for(let n=0;n<60;n++){page=browser.contexts().flatMap(c=>c.pages()).find(p=>/tauri/.test(p.url()));if(page)break;await delay(100);}assert.ok(page);
    const errors=[];page.on('pageerror',e=>errors.push(e.message));
    await page.waitForFunction(()=>document.getElementById('storage-status')?.textContent==='Changes saved in Unified Control');
    const call=(command,args={})=>page.evaluate(async({command,args})=>{try{return await window.__TAURI_INTERNALS__.invoke(command,args);}catch(error){throw new Error(JSON.stringify(error));}},{command,args});
    const waitNative=async(predicate,arg,options={})=>{const deadline=Date.now()+(options.timeout||30000);while(Date.now()<deadline){if(await page.evaluate(predicate,arg))return;await delay(75);}throw Error('Native state did not meet the expected condition');};
    await call('dock_configure',{patch:{enabled:true}});
    await call('cooler_configure',{enabled:true});
    await waitNative(async()=>{const s=await window.__TAURI_INTERNALS__.invoke('cooler_state');return s.state==='running'&&!s.busy;});
    const state=await call('cooler_state');assert.equal(state.state,'running',JSON.stringify(state));assert.equal(state.fake,true);assert.ok(state.pid);
    assert.equal(state.diagnosticsEnabled,true);
    await call('settings_update',{patch:{coolerDiagnostics:false}});
    assert.equal((await call('cooler_state')).diagnosticsEnabled,false);
    await call('settings_update',{patch:{coolerDiagnostics:true}});
    await waitNative(async()=>!!(await window.__TAURI_INTERNALS__.invoke('cooler_state')).health?.lastSampleAt);
    const sampled=(await call('cooler_state')).health.lastSampleAt;
    await call('win_close');
    await waitNative(async previous=>(await window.__TAURI_INTERNALS__.invoke('cooler_state')).health?.lastSampleAt!==previous,sampled,{timeout:20000});
    const engine=await call('cooler_call',{command:'app_state',args:{}});assert.ok(/^http:\/\/127\.0\.0\.1:\d+\/[a-f0-9]+$/.test(engine.mediaBase));
    await page.locator('[data-page="cooler"]').click();
    await page.waitForFunction(()=>!!document.getElementById('cooler-frame'));
    const frame=page.frameLocator('#cooler-frame');await frame.locator('#navMine').waitFor();
    await frame.locator('#navMine').click();
    const transfer=await call('cooler_call',{command:'import_begin',args:{name:'fixture.png',size:4,kind:'original'}});
    assert.equal(await requestQuit(),2,'host Quit must refuse while a staged import is active');
    await call('cooler_call',{command:'import_abort',args:{transferId:transfer.transferId}});
    const png=Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScLbtAAAAABJRU5ErkJggg==','base64');
    const imported=await frame.locator('body').evaluate(async(_,bytes)=>{const {backend}=await import('./js/backend.js');return backend.importFile(new File([new Uint8Array(bytes)],'fixture.png',{type:'image/png'}));},[...png]);
    assert.ok(imported.id);
    const originals=await call('cooler_call',{command:'originals_list',args:{}});assert.ok(JSON.stringify(originals).includes(imported.id));
    await frame.locator('body').filter({hasText:'fixture'}).waitFor({timeout:15000});
    const live=await call('cooler_call',{command:'live_set',args:{patch:{inApp:false,fps:30}}});assert.equal(live.live.status.fps,30);
    await waitNative(async()=>!(await window.__TAURI_INTERNALS__.invoke('cooler_state')).busy);
    const before=await call('cooler_state');process.kill(before.pid);
    await waitNative(async old=>{const s=await window.__TAURI_INTERNALS__.invoke('cooler_state');return s.state==='running'&&s.pid!==old&&!s.busy;},before.pid,{timeout:30000});
    assert.equal((await call('dock_state')).state,'connected','cooler worker crash must leave dock connected');
    await frame.locator('#navMine').click();
    await page.waitForFunction(()=>document.getElementById('cooler-state')?.textContent==='Engine running');
    assert.deepEqual(errors,[]);
    await page.screenshot({path:path.join(home,'combined-cooler.png')});
    await page.locator('[data-page="dock"]').click();
    await page.waitForFunction(()=>!!document.getElementById('dock-enabled'));
    await waitNative(async()=>!(await window.__TAURI_INTERNALS__.invoke('cooler_state')).busy);
    await call('cooler_configure',{enabled:false});
    assert.equal((await call('cooler_state')).enabled,false);
    assert.equal(await requestQuit(),0);assert.equal(await Promise.race([exit,delay(10000).then(()=>{throw Error('Quiet host did not quit');})]),0);
    const healthLog=path.join(home,'devices','cooler','connection-health.jsonl');assert.ok(fs.existsSync(healthLog));assert.ok(fs.readFileSync(healthLog,'utf8').trim().split('\n').every(line=>JSON.parse(line).health));
    console.log('PASS: quiet native UC/worker IPC; embedded themes UI; background health while window closed; diagnostics opt-out; chunked file import; staged-import Quit refusal; saved Live settings; worker crash/restart leaves dock connected; disable/clean Quit. Fake devices only.');
    console.log(`Isolated evidence: ${home}`);
  }finally{
    if(child.exitCode===null){await requestQuit();await Promise.race([exit,delay(10000)]);}
    if(browser)await browser.close();
  }
})().catch(e=>{console.error(e);process.exitCode=1;});
