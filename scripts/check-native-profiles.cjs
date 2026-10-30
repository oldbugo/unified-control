// Quiet debug-only integration check. Never targets an installed app or real data.
const {chromium}=require(process.env.UC_PLAYWRIGHT_MODULE || 'playwright');
const {spawn}=require('node:child_process');
const fs=require('node:fs');
const path=require('node:path');
const os=require('node:os');
const net=require('node:net');
const assert=require('node:assert/strict');
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
async function run() {
  const exe=path.resolve(process.env.UC_TEST_EXE || '');
  if (!exe.toLowerCase().includes(`${path.sep}debug${path.sep}`) || path.basename(exe).toLowerCase()!=='unified-control.exe' || !fs.existsSync(exe)) {
    throw new Error('Set UC_TEST_EXE to the debug unified-control.exe; installed/release binaries are refused.');
  }
  const home=fs.mkdtempSync(path.join(os.tmpdir(),'uc-native-profiles-'));
  const port=await new Promise(resolve=>{const server=net.createServer();server.listen(0,'127.0.0.1',()=>{const port=server.address().port;server.close(()=>resolve(port));});});
  const env={...process.env,UC_HOME:home,UC_TEST_QUIET:'1',UC_MULTI:'1',UC_FAKE_DEVICE:'1',UC_TEST_TRAY_QUIT_MS:'30000',
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
    await page.locator('[data-page="dock"]').click();
    await page.locator('[data-key="0"]').click();
    await page.locator('#face-kind').selectOption('text');
    await page.locator('#face-text').fill('Native A');
    await page.locator('[data-edit-face="b"]').click();
    await page.locator('#face-text').fill('Native B');
    await page.waitForFunction(async()=>{
      const data=await window.__TAURI_INTERNALS__.invoke('profiles_state');
      return data.state.profiles[0].pages[0].keys[0].b.text==='Native B';
    });
    await page.waitForFunction(()=>document.querySelectorAll('img.rendered-key').length===14 && [...document.querySelectorAll('img.rendered-key')].every(img=>img.naturalWidth>0));
    const widths=await page.locator('img.rendered-key').evaluateAll(images=>images.map(img=>[img.naturalWidth,img.naturalHeight]));
    assert.deepEqual(widths[0],[196,196]);assert.deepEqual(widths[13],[458,196]);
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
    // The host's real tray-Quit test hook fires at 30s. Hold the first edit in
    // the client and queue a newer one, so no backend job can hide this risk.
    await delay(Math.max(0,28500-(Date.now()-started)));
    await page.evaluate(()=>{
      const T=window.__TAURI_INTERNALS__, original=T.invoke.bind(T);
      let delayed=false;
      T.invoke=(command,args)=>{
        if(command==='profiles_save' && !delayed) {delayed=true;return new Promise(resolve=>setTimeout(resolve,2500)).then(()=>original(command,args));}
        return original(command,args);
      };
      const input=document.getElementById('face-text');
      for(const value of ['Queued first','Queued last']) {input.value=value;input.dispatchEvent(new Event('input',{bubbles:true}));}
      // Normal window Close must preserve the JS queue while it hides to tray.
      return original('win_close');
    });
    const quitResult=await Promise.race([exited,delay(10000).then(()=>null)]);
    assert.ok(quitResult,'tray Quit did not finish after the queued saves');
    assert.equal(quitResult.code,0);
    const finalProfile=JSON.parse(fs.readFileSync(path.join(home,'profiles',`profile-${index.currentProfile}.json`),'utf8'));
    assert.equal(finalProfile.pages[0].keys[0].b.text,'Queued last','closing and tray Quit must preserve the newest queued edit');
    console.log('PASS: quiet native WebView; real IPC profile files; A/B reload; 14 PNG previews; normal close preserves queued edits; tray Quit flushes newest edit before exit. No device opened.');
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
