// Isolated debug UC + cooler worker. No real devices, accounts or desktop input.
const {chromium}=require(process.env.UC_PLAYWRIGHT_MODULE||'playwright');
const {spawn}=require('node:child_process');
const fs=require('node:fs'),path=require('node:path'),os=require('node:os'),net=require('node:net'),assert=require('node:assert/strict');
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
(async()=>{
  const exe=path.resolve(process.env.UC_TEST_EXE||''),worker=path.resolve(process.env.UC_COOLER_WORKER_EXE||'');
  for(const file of [exe,worker])if(!file.toLowerCase().includes(`${path.sep}debug${path.sep}`)||!fs.existsSync(file))throw Error('Explicit debug executables required; installed/release apps refused');
  const home=fs.mkdtempSync(path.join(os.tmpdir(),'uc-native-cooler-lifetime-'));
  const port=await new Promise(resolve=>{const server=net.createServer();server.listen(0,'127.0.0.1',()=>{const value=server.address().port;server.close(()=>resolve(value));});});
  const env={...process.env,UC_HOME:home,UC_TEST_QUIET:'1',UC_MULTI:'1',UC_FAKE_DEVICE:'1',UC_COOLER_WORKER_EXE:worker,WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:`--remote-debugging-port=${port}`};
  const child=spawn(exe,[],{env,windowsHide:true,stdio:'ignore'}),exit=new Promise(resolve=>child.once('exit',resolve));
  const requestQuit=()=>new Promise((resolve,reject)=>{const request=spawn(exe,['--quit'],{env,windowsHide:true,stdio:'ignore'});request.once('error',reject);request.once('exit',resolve);});
  const idleQuit=async()=>{const end=Date.now()+8000;let code;do{code=await requestQuit();if(code!==2)return code;await delay(75);}while(Date.now()<end);return code;};
  let browser,page;
  try{
    for(let i=0;i<100;i++){try{if((await fetch(`http://127.0.0.1:${port}/json/version`)).ok)break;}catch{}await delay(100);}
    browser=await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
    for(let i=0;i<80;i++){page=browser.contexts().flatMap(context=>context.pages()).find(candidate=>/tauri/.test(candidate.url()));if(page)break;await delay(100);}assert.ok(page);
    await page.waitForFunction(()=>document.querySelector('#storage-status')?.textContent==='Changes saved in Unified Control');
    const call=(command,args={})=>page.evaluate(async({command,args})=>{try{return await window.__TAURI_INTERNALS__.invoke(command,args);}catch(error){throw Error(JSON.stringify(error));}},{command,args});
    const poll=async(predicate,timeout=30000)=>{const deadline=Date.now()+timeout;while(Date.now()<deadline){if(await predicate())return;await delay(100);}throw Error('Expected native condition was not reached');};
    const cooler=(command,args={})=>call('cooler_call',{command,args});
    await call('cooler_configure',{enabled:true});
    await poll(async()=>{const state=await call('cooler_state');return state.state==='running'&&!state.busy;});
    await cooler('live_set',{patch:{inApp:false,music:false,fps:30}});
    await cooler('twin_watch',{on:true});
    assert.equal((await cooler('app_state')).live.status.twinning,true);
    await cooler('live_set',{patch:{inApp:true}});
    assert.equal((await cooler('app_state')).live.status.twinning,false,'saved Live takes ownership of an editor-only pipeline');
    await cooler('twin_watch',{on:false});
    assert.equal((await cooler('app_state')).live.status.running,true,'ending a digital twin cannot stop saved Live');
    await poll(async()=>(await cooler('app_state')).live.status.measuredFps>0);
    const checkCount=()=>fs.readFileSync(path.join(home,'devices','cooler','device.log'),'utf8').split('\n').filter(line=>line.includes('device task: check')).length;
    const initialChecks=checkCount();
    await poll(async()=>await page.locator('.nav-item[data-page="cooler"]').isVisible());
    await page.locator('.nav-item[data-page="cooler"]').click();
    await page.frameLocator('#cooler-frame').locator('#navScreen').waitFor();
    await page.frameLocator('#cooler-frame').locator('body').evaluate(async()=>{await(await import('./app.js')).initialized;});
    assert.equal(checkCount(),initialChecks,'entering a running Live workspace does not schedule a USB check');
    for(let visit=0;visit<3;visit++){
      await page.locator('.nav-item[data-page="home"]').click();
      await page.locator('.nav-item[data-page="home"][aria-current="page"]').waitFor({timeout:2000});
      await page.locator('.nav-item[data-page="cooler"]').click();
      await page.frameLocator('#cooler-frame').locator('body').evaluate(async()=>{await(await import('./app.js')).initialized;});
      const live=(await cooler('app_state')).live.status;
      assert.equal(live.running,true);assert.equal(live.deviceHold,false,'returning to the editor never pauses Live for a check');
      assert.equal(checkCount(),initialChecks,'repeated editor entry does not reestablish the connection');
    }
    await page.frameLocator('#cooler-frame').locator('body').evaluate(async()=>{const {backend}=await import('./js/backend.js');await backend.call('twin_watch',{on:true});});
    const baseline=(await cooler('app_state')).live.settings;
    await page.locator('.nav-item[data-page="home"]').click();
    await page.locator('.nav-item[data-page="home"][aria-current="page"]').waitFor({timeout:2000});
    assert.equal(await page.locator('#cooler-frame').count(),0,'hidden editor is removed rather than retaining visual loops');
    // Outlast the editor-only twin watchdog, which can stop an unclaimed stream.
    const until=Date.now()+11500;
    while(Date.now()<until){assert.equal((await cooler('app_state')).live.status.running,true,'Live survives leaving its editor');await delay(500);}
    assert.ok((await cooler('app_state')).live.status.measuredFps>0,'Live keeps producing frames after the editor is removed');
    await call('win_close');
    const closedUntil=Date.now()+2500;
    while(Date.now()<closedUntil){assert.equal((await cooler('app_state')).live.status.running,true,'Live survives closing UC to its tray');await delay(250);}
    assert.ok((await cooler('app_state')).live.status.measuredFps>0,'Live keeps producing frames while UC is in its tray');
    assert.deepEqual((await cooler('app_state')).live.settings,baseline,'navigation and window close preserve Live settings');
    assert.equal((await call('cooler_state')).state,'running');
    await cooler('live_set',{patch:{inApp:false}});
    assert.equal((await cooler('app_state')).live.status.running,false,'explicit Live off still stops playback');
    await call('cooler_configure',{enabled:false});
    assert.equal(await idleQuit(),0);assert.equal(await Promise.race([exit,delay(10000).then(()=>{throw Error('Quiet host did not quit');})]),0);
    console.log('PASS: native fake cooler ownership transfer from editor twin to saved Live; actual section navigation removes iframe while Live survives its watchdog; close-to-tray preserves Live and settings; explicit Live off, device disable and clean Quit still work.');
    console.log(`Isolated evidence: ${home}`);
  }finally{
    if(child.exitCode===null){await idleQuit();await Promise.race([exit,delay(10000)]);}
    if(browser)await browser.close();
  }
})().catch(error=>{console.error(error);process.exitCode=1;});
