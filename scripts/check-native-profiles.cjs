// Quiet debug-only integration check. Never targets an installed app or real data.
const {chromium}=require(process.env.UC_PLAYWRIGHT_MODULE || 'playwright');
const {spawn}=require('node:child_process');
const fs=require('node:fs');
const path=require('node:path');
const os=require('node:os');
const net=require('node:net');
const assert=require('node:assert/strict');
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
// Await the predicate's resolved value in Node. Some WebView/Playwright builds
// treat its Promise as truthy inside waitForFunction and do not poll false results.
async function waitForAsync(page,predicate,arg,timeoutMs=30000){
  const deadline=Date.now()+timeoutMs;
  const timeout=()=>new Error(`Async native condition did not become true within ${timeoutMs}ms: ${predicate.toString().slice(0,180)}`);
  while(Date.now()<deadline){
    let timer;
    try{
      const matched=await Promise.race([page.evaluate(predicate,arg),new Promise((_,reject)=>{timer=setTimeout(()=>reject(timeout()),Math.max(1,deadline-Date.now()));})]);
      if(matched)return;
    }finally{clearTimeout(timer);}
    await delay(Math.min(50,Math.max(0,deadline-Date.now())));
  }
  throw timeout();
}
async function run() {
  const exe=path.resolve(process.env.UC_TEST_EXE || '');
  if (!exe.toLowerCase().includes(`${path.sep}debug${path.sep}`) || path.basename(exe).toLowerCase()!=='unified-control.exe' || !fs.existsSync(exe)) {
    throw new Error('Set UC_TEST_EXE to the debug unified-control.exe; installed/release binaries are refused.');
  }
  const home=fs.mkdtempSync(path.join(os.tmpdir(),'uc-native-profiles-'));
  const port=await new Promise(resolve=>{const server=net.createServer();server.listen(0,'127.0.0.1',()=>{const port=server.address().port;server.close(()=>resolve(port));});});
  const env={...process.env,UC_HOME:home,UC_TEST_QUIET:'1',UC_MULTI:'1',UC_FAKE_DEVICE:'1',UC_TEST_TRAY_QUIT_MS:'45000',
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:`--remote-debugging-port=${port}`};
  const output=process.env.UC_PREVIEW_OUTPUT || path.join(os.tmpdir(),'uc-interface-preview');
  fs.mkdirSync(output,{recursive:true});
  const started=Date.now();
  const child=spawn(exe,[],{env,windowsHide:true,stdio:['ignore','pipe','pipe']});
  const exited=new Promise(resolve=>child.once('exit',(code,signal)=>resolve({code,signal})));
  let logs='';
  child.stdout.on('data',data=>{logs+=data;});child.stderr.on('data',data=>{logs+=data;});
  let connected=false;
  try {
    for (let n=0;n<100;n++) {
      try {const response=await fetch(`http://127.0.0.1:${port}/json/version`);if(response.ok){connected=true;break;}}catch{}
      if(child.exitCode!==null) throw new Error(`Quiet debug instance exited early (${child.exitCode}).`);
      await delay(200);
    }
    if(!connected) throw new Error('Quiet WebView debug endpoint did not become ready.');
    const browser=await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
    let page;
    for(let n=0;n<50;n++) {
      page=browser.contexts().flatMap(context=>context.pages()).find(page=>/tauri/.test(page.url()));
      if(page) break;
      await delay(100);
    }
    if(!page) throw new Error('Quiet UC WebView was not found.');
    const errors=[];page.on('pageerror',error=>errors.push(error.message));
    await page.waitForFunction(()=>document.getElementById('storage-status')?.textContent==='Changes saved in Unified Control');
    assert.deepEqual(await page.evaluate(()=>window.__TAURI_INTERNALS__.invoke('monitor_list')),
      [{key:'fake:display-1',label:'Simulated display'}], 'monitor discovery must be admitted through native IPC');
    await page.locator('[data-page="dock"]').click();
    await page.locator('[data-key="0"]').click();
    await page.locator('#face-kind').selectOption('text');
    await page.locator('#face-text').fill('Native A');
    await page.locator('[data-edit-face="b"]').click();
    await page.locator('#face-text').fill('Native B');
    await waitForAsync(page,async()=>{
      const data=await window.__TAURI_INTERNALS__.invoke('profiles_state');
      return data.state.profiles[0].pages[0].keys[0].b.text==='Native B';
    });
    await page.waitForFunction(()=>document.querySelectorAll('img.rendered-key').length===14 && [...document.querySelectorAll('img.rendered-key')].every(img=>img.naturalWidth>0));
    const widths=await page.locator('img.rendered-key').evaluateAll(images=>images.map(img=>[img.naturalWidth,img.naturalHeight]));
    assert.deepEqual(widths[0],[196,196]);assert.deepEqual(widths[13],[458,196]);
    // Import a locally generated fixture picture through the real native path.
    const picture=await page.evaluate(()=>{const canvas=document.createElement('canvas');canvas.width=4;canvas.height=4;const context=canvas.getContext('2d');context.fillStyle='#5aa9ff';context.fillRect(0,0,4,4);return canvas.toDataURL('image/png').split(',')[1];});
    await page.locator('#face-kind').selectOption('picture');
    await page.locator('#face-picture').setInputFiles({name:'fixture.png',mimeType:'image/png',buffer:Buffer.from(picture,'base64')});
    await waitForAsync(page,async()=>{const data=await window.__TAURI_INTERNALS__.invoke('profiles_state');return !!data.state.profiles[0].pages[0].keys[0].b.asset;});
    await page.locator('#action-kind').selectOption('hotkey');
    await page.locator('#action-value').fill('Ctrl + Shift + M');
    await page.locator('[data-key="1"]').click();
    await page.locator('#face-kind').selectOption('codex');
    await page.locator('#usage-view').selectOption('weekly-number');
    await waitForAsync(page,async()=>{const data=await window.__TAURI_INTERNALS__.invoke('profiles_state');return data.state.profiles[0].pages[0].keys[1].b.usageView==='weekly-number';});
    await page.evaluate(async()=>{
      const invoke=window.__TAURI_INTERNALS__.invoke;
      await invoke('dock_test_studio',{running:true});
      await invoke('dock_configure',{patch:{enabled:true,brightness:42}});
    });
    await waitForAsync(page,async()=>{const state=await window.__TAURI_INTERNALS__.invoke('dock_state');return state.state==='blocked';});
    await page.evaluate(()=>window.__TAURI_INTERNALS__.invoke('dock_test_studio',{running:false}));
    await waitForAsync(page,async()=>{const state=await window.__TAURI_INTERNALS__.invoke('dock_state');return state.state==='connected'&&state.framesSent>=14;});
    const readerRunning=await page.evaluate(async()=>{const state=await window.__TAURI_INTERNALS__.invoke('dock_state');return state.usageReaderRunning;});
    assert.equal(readerRunning,false,'fake app must never activate real account readers');
    await page.evaluate(async()=>{const invoke=window.__TAURI_INTERNALS__.invoke;for(const pressed of [true,true,false,true,false])await invoke('dock_test_button',{index:0,pressed});});
    await waitForAsync(page,async()=>{const state=await window.__TAURI_INTERNALS__.invoke('dock_state');return state.actions===2;});
    // Physical folder/Back updates the editor after the fake full-page transfer.
    // Observing the device must not save the navigation back into profile files.
    await page.locator('[data-do="add-page"]').click();
    await page.locator('#page-name').fill('Follow target');
    await page.locator('#action-kind').selectOption('back');
    await page.locator('[data-dock-page="0"]').click();
    await page.locator('[data-key="12"]').click();
    await page.locator('#action-kind').selectOption('folder');
    await page.locator('#action-value').selectOption('1');
    await waitForAsync(page,async()=>{const {state}=await window.__TAURI_INTERNALS__.invoke('profiles_state');return state.profiles[0].pages.length===2&&state.profiles[0].pages[0].keys[12].value===state.profiles[0].pages[1].id&&state.profiles[0].pages[1].keys[0].action==='back';});
    assert.equal(await page.locator('#follow-physical-page').isChecked(),true);
    const mainPage=await page.evaluate(async()=>{const {state}=await window.__TAURI_INTERNALS__.invoke('profiles_state');return state.currentPage;});
    await waitForAsync(page,async id=>(await window.__TAURI_INTERNALS__.invoke('dock_state')).currentPage===id,mainPage);
    await page.evaluate(async()=>{for(const pressed of [true,false])await window.__TAURI_INTERNALS__.invoke('dock_test_button',{index:12,pressed});});
    await page.waitForFunction(()=>document.querySelector('[data-dock-page="1"]')?.getAttribute('aria-pressed')==='true');
    assert.equal(await page.evaluate(async()=>(await window.__TAURI_INTERNALS__.invoke('profiles_state')).state.currentPage),mainPage,'following physical navigation must not save it back');
    await page.evaluate(async()=>{for(const pressed of [true,false])await window.__TAURI_INTERNALS__.invoke('dock_test_button',{index:0,pressed});});
    await page.waitForFunction(()=>document.querySelector('[data-dock-page="0"]')?.getAttribute('aria-pressed')==='true');
    await page.locator('#follow-physical-page').uncheck();
    await waitForAsync(page,async()=>(await window.__TAURI_INTERNALS__.invoke('app_state')).followPhysicalPage===false);
    await page.reload();
    await page.waitForFunction(()=>document.getElementById('follow-physical-page')?.checked===false);
    await page.locator('#follow-physical-page').check();
    await waitForAsync(page,async()=>(await window.__TAURI_INTERNALS__.invoke('app_state')).followPhysicalPage===true);
    await page.evaluate(()=>window.__TAURI_INTERNALS__.invoke('dock_configure',{patch:{enabled:false}}));
    await waitForAsync(page,async()=>{const state=await window.__TAURI_INTERNALS__.invoke('dock_state');return state.state==='disabled';});
    await page.locator('[data-key="0"]').click();
    const index=JSON.parse(fs.readFileSync(path.join(home,'profiles','index.json'),'utf8'));
    const profile=JSON.parse(fs.readFileSync(path.join(home,'profiles',`profile-${index.currentProfile}.json`),'utf8'));
    assert.equal(profile.pages[0].keys[0].a.text,'Native A');
    assert.equal(profile.pages[0].keys[0].b.text,'Native B');
    await page.reload();
    await page.waitForFunction(()=>document.querySelectorAll('[data-key]').length===14);
    await page.locator('[data-key="0"]').click();
    assert.equal(await page.locator('#face-text').inputValue(),'Native A');
    await page.locator('[data-edit-face="b"]').click();
    assert.equal(await page.locator('#face-text').inputValue(),'Native B');
    await page.locator('[data-page="cooler"]').click();
    await page.locator('[data-page="dock"]').click();
    await page.waitForFunction(()=>document.querySelectorAll('img.rendered-key').length===14);
    await page.screenshot({path:path.join(output,'uc-native-dock.png')});
    assert.deepEqual(errors,[]);
    // The host's real tray-Quit test hook fires at 45s. Hold the first edit in
    // the client and queue a newer one, so no backend job can hide this risk.
    await delay(Math.max(0,43500-(Date.now()-started)));
    await page.evaluate(picture=>{
      const T=window.__TAURI_INTERNALS__, original=T.invoke.bind(T);
      let delayed=false;
      T.invoke=(command,args)=>{
        if(command==='asset_import')return new Promise(resolve=>setTimeout(resolve,2500)).then(()=>original(command,args));
        if(command==='profiles_save' && !delayed) {delayed=true;return new Promise(resolve=>setTimeout(resolve,2500)).then(()=>original(command,args));}
        return original(command,args);
      };
      const input=document.getElementById('face-text');
      for(const value of ['Queued first','Queued last']) {input.value=value;input.dispatchEvent(new Event('input',{bubbles:true}));}
      const bytes=Uint8Array.from(atob(picture),c=>c.charCodeAt(0));
      const transfer=new DataTransfer();transfer.items.add(new File([bytes],'late-picture.png',{type:'image/png'}));
      const file=document.getElementById('face-picture');file.files=transfer.files;file.dispatchEvent(new Event('change',{bubbles:true}));
      // Normal window Close must preserve the JS queue while it hides to tray.
      return original('win_close');
    },picture);
    const quitResult=await Promise.race([exited,delay(10000).then(()=>null)]);
    assert.ok(quitResult,'tray Quit did not finish after the queued saves');
    assert.equal(quitResult.code,0);
    const finalProfile=JSON.parse(fs.readFileSync(path.join(home,'profiles',`profile-${index.currentProfile}.json`),'utf8'));
    assert.equal(finalProfile.pages[0].keys[0].b.text,'Queued last','closing and tray Quit must preserve the newest queued edit');
    assert.notEqual(finalProfile.pages[0].keys[0].b.asset,profile.pages[0].keys[0].b.asset,'Quit must also await a pending picture import');
    assert.ok(fs.existsSync(path.join(home,'assets',`${finalProfile.pages[0].keys[0].b.asset}.png`)));
    console.log('PASS: quiet native WebView; actual IPC/files; picture imports; A/B reload; 14 PNGs; Studio block/reconnect; deduplicated fake presses; delivered physical folder/Back follow without save-back; persisted follow preference; inactive account readers; disable; tray Quit preserves queued edits and pending import. No physical device opened.');
    console.log(`Isolated test data: ${home}`);
  } finally {
    if(child.exitCode===null) {
      const quit=spawn(exe,['--quit'],{env,windowsHide:true,stdio:'ignore'});
      await new Promise(resolve=>quit.once('exit',resolve));
    }
    const result=await Promise.race([exited,delay(6000).then(()=>null)]);
    fs.writeFileSync(path.join(home,'test-output.log'),logs);
    if(!result) throw new Error('Quiet test instance did not finish its clean quit; its 90-second safety timer remains active.');
    if(result.code!==0) throw new Error(`Quiet debug instance exit code ${result.code}.`);
  }
}
run().catch(error=>{console.error(error);process.exitCode=1;});
