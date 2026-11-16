// Headless browser fixtures only: no native app, account, device or OS actions.
const {chromium}=require(process.env.UC_PLAYWRIGHT_MODULE||'playwright');
const fs=require('node:fs'),path=require('node:path'),http=require('node:http'),assert=require('node:assert/strict');
const root=path.resolve(__dirname,'../public');
const server=http.createServer((req,res)=>{
  const file=path.resolve(root,'.'+new URL(req.url,'http://localhost').pathname.replace(/\/$/,'/index.html'));
  if(!file.startsWith(root+path.sep)){res.writeHead(403);return res.end();}
  fs.readFile(file,(error,data)=>{res.writeHead(error?404:200,{'Content-Type':file.endsWith('.js')?'text/javascript':file.endsWith('.css')?'text/css':'text/html'});res.end(error?'Not found':data);});
});
(async()=>{
  await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));let browser;
  try{
    browser=await chromium.launch({headless:true});const page=await browser.newPage({viewport:{width:1440,height:1000}}),errors=[];
    page.on('pageerror',error=>errors.push(error.message));
    await page.addInitScript(()=>{if(window!==parent){window.__nativeBypassCalls=0;Object.defineProperty(window,'__TAURI_INTERNALS__',{value:{invoke(){window.__nativeBypassCalls++;throw new Error('Embedded UI bypassed its UC bridge.');}},configurable:false,writable:false});}});
    await page.goto(`http://127.0.0.1:${server.address().port}/`);
    await page.evaluate(async()=>{
      const {CoolerPanel}=await import('/js/cooler-panel.js');
      window.__calls=[];window.__transfers=new Map();window.__transferNext=0;window.__holdAppend=false;window.__failAppend=false;window.__appendWaiters=[];window.__coolerState={enabled:false,state:'disabled',message:'Cooler disabled'};
      window.__app={appVersion:'fixture',fake:true,mask:{mode:'none',band:{},circle:{}},screenFit:{},screenGuides:{},weather:{},galleryDir:'Fixture',live:{settings:{inApp:false},status:{}},mediaBase:''};
      const invoke=async(command,args={})=>{
        window.__calls.push({command,args:args.command==='import_append'?{...args,args:{...args.args,bytesLength:args.args.bytes.length,bytes:args.args.bytes.length<10?args.args.bytes:null}}:args});
        if(command==='cooler_state')return window.__coolerState;
        if(command==='settings_update'){window.__coolerState.diagnosticsEnabled=args.patch.coolerDiagnostics;return {cooler:window.__coolerState};}
        if(command==='cooler_configure'){window.__coolerState={enabled:args.enabled,state:args.enabled?'running':'disabled',message:'Fixture engine'};return window.__coolerState;}
        if(command!=='cooler_call')throw new Error('unexpected host command');
        const c=args.command;
        if(c==='app_state'&&window.__holdAppState)await new Promise(resolve=>window.__appStateWaiters.push(resolve));
        if(c==='app_state'||c==='settings_update'||c==='live_set')return window.__app;
        if(window.__holdCleanup&&((c==='twin_watch'&&args.args.on===false)||(c==='fit_cal'&&args.args.action==='end')))await new Promise(resolve=>window.__cleanupWaiters.push(resolve));
        if(c==='originals_list')return {originals:[],longMissing:[],rev:1};
        if(c==='sources_list')return {sources:[],ticked:[]};
        if(c==='activity')return {jobs:[],rev:1,deviceBusy:false};
        if(c==='device_check')return {state:'ready'};
        if(c==='device_reconnect')return {jobId:'fake-reset'};
        if(c==='job_get'&&args.args.jobId==='fake-reset')return {status:'done',result:{firmwareVerified:true}};
        if(c==='import_begin'){const transferId='fixture-'+(++window.__transferNext);window.__transfers.set(transferId,{...args.args,received:0});return {transferId};}
        if(c==='import_append'){
          const value=args.args,transfer=window.__transfers.get(value.transferId);
          if(window.__holdAppend)await new Promise(resolve=>window.__appendWaiters.push(resolve));
          if(window.__failAppend&&value.offset>0)throw {error:'Fixture append failed',code:'transfer-failed'};
          if(!transfer||value.offset!==transfer.received||value.bytes.length>256*1024)throw new Error('invalid fixture chunk');
          transfer.received+=value.bytes.length;return {received:transfer.received};
        }
        if(c==='import_finish'){const transfer=window.__transfers.get(args.args.transferId);if(!transfer||transfer.received!==transfer.size)throw new Error('incomplete fixture upload');window.__transfers.delete(args.args.transferId);return {id:'fixture-import',file:'fixture.png'};}
        if(c==='import_abort'){window.__transfers.delete(args.args.transferId);return {ok:true};}
        if(c==='theme_get')throw {error:'Fixture theme missing',code:'not-found',status:404};
        return {};
      };
      window.__panel=new CoolerPanel(invoke,true);const container=document.createElement('div');container.id='cooler-test';container.style.height='760px';container.innerHTML=window.__panel.markup();const settings=document.createElement('div');settings.id='cooler-settings-test';document.getElementById('pane').replaceChildren(settings,container);window.__panel.mount(container);window.__panel.mountSettings(settings);settings.querySelector('details').open=true;
      if(container.querySelector('#cooler-enabled')||container.querySelector('.device-header'))throw Error('Device enablement must live in General settings, outside the cooler workspace');
    });
    let releaseFrame,finishFrame;
    const frameReleased=new Promise(resolve=>finishFrame=resolve);
    await page.route('**/cooler/index.html',async route=>{await new Promise(resolve=>releaseFrame=resolve);await route.continue();finishFrame();});
    await page.locator('#cooler-enabled').check();await page.waitForSelector('#cooler-frame');
    await page.evaluate(()=>{window.__leftLoading=false;window.__panel.prepareLeave({navigation:true}).then(()=>window.__leftLoading=true);});
    await page.waitForTimeout(100);
    assert.equal(await page.evaluate(()=>window.__leftLoading),true,'navigation does not wait for the embedded bridge to load');
    await page.evaluate(()=>{window.__holdAppState=true;window.__appStateWaiters=[];});
    releaseFrame();await frameReleased;await page.unroute('**/cooler/index.html');
    await page.waitForFunction(()=>window.__panel.frameReady&&window.__appStateWaiters.length>0);
    await page.evaluate(()=>{window.__leftStartup=false;window.__panel.prepareLeave({navigation:true}).then(()=>window.__leftStartup=true);});
    await page.waitForFunction(()=>window.__leftStartup,{},{timeout:1000});
    assert.ok(await page.evaluate(()=>window.__appStateWaiters.length>0),'navigation completes while startup state remains unavailable');
    await page.evaluate(()=>{window.__holdAppState=false;window.__appStateWaiters.splice(0).forEach(resolve=>resolve());});
    await page.locator('#cooler-diagnostics').uncheck();
    await page.waitForFunction(()=>window.__coolerState.diagnosticsEnabled===false);
    assert.match(await page.locator('#cooler-health').textContent(),/off/);
    await page.locator('#cooler-diagnostics').check();
    await page.waitForFunction(()=>window.__coolerState.diagnosticsEnabled===true);
    await page.waitForFunction(()=>document.getElementById('cooler-frame')?.contentWindow?.location.pathname==='/cooler/index.html');
    const frame=page.frames().find(f=>f.url().includes('/cooler/index.html'));
    await page.waitForFunction(()=>window.__calls.some(c=>c.command==='cooler_call'&&c.args.command==='activity'));
    await page.locator('#cooler-reset').click();
    await page.waitForFunction(()=>document.getElementById('cooler-health')?.textContent.includes('answered after the USB reset'));
    assert.equal(await page.evaluate(()=>window.__calls.filter(c=>c.command==='cooler_call'&&c.args.command==='device_reconnect').length),1);
    assert.equal(await frame.locator('#cs-titlebar').count(),0,'embedded UI has no independent native titlebar');
    assert.equal(await frame.locator('#topbar > .appname').isVisible(),false,'workspace toolbar does not repeat app chrome');
    assert.equal(await frame.locator('#setRestart').isVisible(),false);
    await frame.locator('#navMine').click();assert.equal(await frame.locator('#mineView').isVisible(),true,'existing My themes view works');
    const imported=await frame.evaluate(async()=>{const {backend}=await import('./js/backend.js');return backend.importFile(new File([new Uint8Array([1,2,3])],'sample.png'));});
    assert.equal(imported.id,'fixture-import');
    const importedCall=await page.evaluate(()=>window.__calls.find(c=>c.args.command==='import_begin'));
    assert.deepEqual(importedCall.args.args,{name:'sample.png',size:3,kind:'original'});
    assert.deepEqual(await page.evaluate(()=>window.__calls.find(c=>c.args.command==='import_append').args.args.bytes),[1,2,3]);
    await frame.evaluate(async()=>{const {backend}=await import('./js/backend.js');await backend.themeAsset('original-fixture',new File([new Uint8Array([4,5])],'asset.png'));});
    assert.deepEqual(await page.evaluate(()=>window.__calls.find(c=>c.args.command==='import_begin'&&c.args.args.kind==='asset').args.args),{originalId:'original-fixture',name:'asset.png',size:2,kind:'asset'});
    const bigResult=await frame.evaluate(async()=>{
      const {backend}=await import('./js/backend.js');window.__fixtureSlices=[];
      const file={name:'large.mp4',size:32*1024*1024+1,arrayBuffer(){throw new Error('whole-file read is forbidden');},slice(start,end){window.__fixtureSlices.push([start,end]);return {arrayBuffer:async()=>new Uint8Array(end-start).buffer};}};
      const result=await backend.importFile(file);return {result,slices:window.__fixtureSlices};
    });
    assert.equal(bigResult.result.id,'fixture-import');assert.equal(bigResult.slices.length,129);
    assert.deepEqual(bigResult.slices[0],[0,256*1024]);assert.deepEqual(bigResult.slices.at(-1),[32*1024*1024,32*1024*1024+1]);
    assert.equal(await page.evaluate(()=>window.__transfers.size),0,'finished staged transfers are released');
    await page.evaluate(()=>window.__failAppend=true);
    assert.equal(await frame.evaluate(async()=>{const {backend}=await import('./js/backend.js');try{await backend.importFile({name:'failure.mp4',size:256*1024+1,slice(start,end){return {arrayBuffer:async()=>new Uint8Array(end-start).buffer};}});return false;}catch(e){return e.code==='transfer-failed';}}),true);
    assert.equal(await page.evaluate(()=>window.__transfers.size),0,'failed imports abort their staged file');
    assert.equal(await page.evaluate(()=>window.__calls.filter(c=>c.args.command==='import_abort').length),1);
    const begins=await page.evaluate(()=>window.__calls.filter(c=>c.args.command==='import_begin').length);
    assert.equal(await frame.evaluate(async()=>{const {backend}=await import('./js/backend.js');try{await backend.importFile({name:'too-large.mp4',size:2*1024*1024*1024+1});return false;}catch(e){return e.message.includes('2 GiB');}}),true);
    assert.equal(await page.evaluate(()=>window.__calls.filter(c=>c.args.command==='import_begin').length),begins,'oversized files fail before staging');
    await page.evaluate(()=>{window.__failAppend=false;window.__holdAppend=true;});
    await frame.evaluate(async()=>{const {backend}=await import('./js/backend.js');window.__fixtureUpload=backend.importFile(new File(['held'],'held.png'));});
    await page.waitForFunction(()=>window.__appendWaiters.length===1);
    await page.evaluate(()=>{window.__leftImport=false;window.__panel.prepareLeave({navigation:true}).then(()=>window.__leftImport=true);});
    await page.waitForTimeout(80);assert.equal(await page.evaluate(()=>window.__leftImport),false,'leaving waits for active file transfer');
    await page.evaluate(()=>{window.__holdAppend=false;window.__appendWaiters.splice(0).forEach(resolve=>resolve());});
    await page.waitForFunction(()=>window.__leftImport);
    assert.equal(await frame.evaluate(async()=>{try{await window.__UC_COOLER_BRIDGE__.invoke('win_close');return false;}catch{return true;}}),true,'arbitrary host commands are rejected');
    const before=await page.evaluate(()=>window.__calls.filter(c=>c.args.command==='live_set').length);
    await page.evaluate(()=>window.postMessage({channel:'uc-cooler-v1',type:'call',id:999,command:'live_set',args:{}},location.origin));
    await page.waitForTimeout(60);assert.equal(await page.evaluate(()=>window.__calls.filter(c=>c.args.command==='live_set').length),before,'wrong message source cannot invoke the cooler');
    assert.equal(await frame.locator('#lvSettle').inputValue(),'30','the settle wait after a Live stop defaults to 30 s (engine 4.23.0)');
    await frame.evaluate(()=>{const e=document.getElementById('lvSettle');e.value='0';e.dispatchEvent(new Event('change'));});
    await page.waitForFunction(()=>window.__calls.some(c=>c.args.command==='live_set'&&c.args.args?.patch?.settleAfterStopSeconds===0));
    await page.evaluate(()=>{window.__app.live.settings.settleAfterStopSeconds=45;});
    await frame.evaluate(()=>document.getElementById('lvSettle').dispatchEvent(new Event('change')));
    await frame.waitForFunction(()=>document.getElementById('lvSettle').value==='45');
    await page.evaluate(()=>{delete window.__app.live.settings.settleAfterStopSeconds;});
    await frame.evaluate(()=>{window.__fixtureEvent=null;const t=window.__UC_COOLER_BRIDGE__;return t.invoke('plugin:event|listen',{event:'cs:job',handler:t.transformCallback(e=>window.__fixtureEvent=e.payload)});});
    await page.evaluate(()=>window.__panel.event({event:'cs:job',payload:{id:'fixture-job',status:'done'}}));
    await frame.waitForFunction(()=>window.__fixtureEvent?.id==='fixture-job');
    assert.equal(await frame.evaluate(async()=>{try{await window.__UC_COOLER_BRIDGE__.invoke('theme_get',{originalId:'missing'});return false;}catch(e){return e.code==='not-found'&&e.status===404;}}),true,'structured engine errors reach the existing UI');
    assert.equal(await frame.evaluate(()=>window.__nativeBypassCalls),0,'Tauri iframe injection cannot replace or bypass the explicit UC bridge');
    const guidesBefore=await page.evaluate(()=>window.__calls.filter(c=>c.args.command==='calibration_set'&&c.args.args.guides).length);
    await frame.evaluate(async()=>{
      document.getElementById('gear').click();
      document.getElementById('setGuides').dispatchEvent(new FocusEvent('focusin',{bubbles:true}));
      await window.__UC_COOLER_FLUSH__({navigation:true});
    });
    await page.waitForTimeout(100);
    assert.equal(await page.evaluate(()=>window.__calls.filter(c=>c.args.command==='calibration_set'&&c.args.args.guides).length),guidesBefore,'navigation cancels a pending guide-preview start timer');
    await frame.locator('#drawerClose').click();
    await frame.evaluate(async()=>{const {backend}=await import('./js/backend.js');await backend.call('activity');});
    const idleCleanup=await page.evaluate(()=>window.__calls.filter(c=>['twin_watch','fit_cal','calibration_set'].includes(c.args.command)).length);
    for(let i=0;i<5;i++)await page.evaluate(()=>window.__panel.prepareLeave({navigation:true}));
    assert.equal(await page.evaluate(()=>window.__calls.filter(c=>['twin_watch','fit_cal','calibration_set'].includes(c.args.command)).length),idleCleanup,'repeated idle navigation does not queue redundant cleanup');
    await frame.evaluate(async()=>{const {backend}=await import('./js/backend.js');await backend.call('twin_watch',{on:true});await backend.call('fit_cal',{action:'start'});});
    await page.evaluate(()=>{window.__holdCleanup=true;window.__cleanupWaiters=[];window.__leftCleanup=false;window.__panel.prepareLeave({navigation:true}).then(()=>window.__leftCleanup=true);});
    await page.waitForFunction(()=>window.__cleanupWaiters.length===2);
    assert.equal(await page.evaluate(()=>window.__leftCleanup),true,'navigation does not await unavailable-device preview cleanup');
    await page.evaluate(()=>{window.__fullCleanupLeft=false;window.__panel.prepareLeave().then(()=>window.__fullCleanupLeft=true);});
    await page.waitForTimeout(80);
    assert.equal(await page.evaluate(()=>window.__fullCleanupLeft),false,'disable and Quit still await full cleanup');
    await page.evaluate(()=>{window.__holdCleanup=false;window.__cleanupWaiters.splice(0).forEach(resolve=>resolve());});
    await page.waitForFunction(()=>window.__fullCleanupLeft);
    await frame.evaluate(()=>{window.__UC_COOLER_FLUSH__=()=>new Promise(resolve=>window.__releaseFlush=resolve);});
    await page.evaluate(()=>{window.__left=false;window.__panel.prepareLeave().then(()=>window.__left=true);});
    await frame.waitForFunction(()=>typeof window.__releaseFlush==='function');assert.equal(await page.evaluate(()=>window.__left),false,'leaving waits for queued edits');
    await frame.evaluate(()=>window.__releaseFlush());await page.waitForFunction(()=>window.__left);
    await frame.evaluate(()=>{window.__UC_COOLER_FLUSH__=async()=>{throw new Error('save failed');};});
    assert.equal(await page.evaluate(async()=>{try{await window.__panel.prepareLeave({navigation:true});return false;}catch{return true;}}),true,'failed save blocks frame removal');
    await frame.evaluate(()=>{window.__UC_COOLER_FLUSH__=async()=>{};});
    await page.locator('#cooler-enabled').uncheck();await page.waitForFunction(()=>!document.getElementById('cooler-frame'));
    assert.deepEqual(errors,[]);
    console.log('PASS: headless embedded CoolerScreen UI; loading-frame and held-startup navigation; nonblocking held preview cleanup without idle RPC flooding; full shutdown cleanup guard; enable/disable; existing themes navigation; bounded chunked imports/assets beyond 32 MiB; failed transfer abort and active-import leave guard; strict source/command bridge; worker events and errors; queued/failed save leave guard. No native app/device/account/OS effects.');
  }finally{await browser?.close();await new Promise(resolve=>server.close(resolve));}
})().catch(error=>{console.error(error);process.exitCode=1;});
