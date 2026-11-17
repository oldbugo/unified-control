// Native, quiet: a debug UC with the fake Vibe Key (UC_FAKE_DEVICE). Real IPC, real worker, encrypted fake.
// No window is shown, no focus is taken, no sound, and the real AU05 is never opened.
const fs=require('node:fs'),path=require('node:path'),os=require('node:os'),net=require('node:net'),assert=require('node:assert/strict');
const {spawn}=require('node:child_process');
const {chromium}=require(process.env.UC_PLAYWRIGHT_MODULE||'playwright');
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
async function until(f,what,ms=10000){const end=Date.now()+ms;for(;;){const v=await f();if(v)return v;if(Date.now()>end)throw new Error(`Timed out: ${what}`);await delay(100);}}
async function run(){
  const exe=path.resolve(process.env.UC_TEST_EXE||'');
  if(!exe.toLowerCase().includes(`${path.sep}debug${path.sep}`)||path.basename(exe).toLowerCase()!=='unified-control.exe'||!fs.existsSync(exe))
    throw new Error('Set UC_TEST_EXE to the debug unified-control.exe; installed/release binaries are refused.');
  const home=fs.mkdtempSync(path.join(os.tmpdir(),'uc-native-vibe-key-'));
  const port=await new Promise(resolve=>{const s=net.createServer();s.listen(0,'127.0.0.1',()=>{const p=s.address().port;s.close(()=>resolve(p));});});
  const env={...process.env,UC_HOME:home,UC_TEST_QUIET:'1',UC_MULTI:'1',UC_FAKE_DEVICE:'1',UC_TEST_TRAY_QUIT_MS:'60000',
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:`--remote-debugging-port=${port}`};
  const child=spawn(exe,[],{env,windowsHide:true,stdio:['ignore','pipe','pipe']});
  const exited=new Promise(resolve=>child.once('exit',(code,signal)=>resolve({code,signal})));
  let logs='';child.stdout.on('data',d=>{logs+=d;});child.stderr.on('data',d=>{logs+=d;});
  try{
    await until(async()=>{if(child.exitCode!==null)throw new Error(`exited early (${child.exitCode})`);try{return (await fetch(`http://127.0.0.1:${port}/json/version`)).ok;}catch{return false;}},'WebView debug endpoint',20000);
    const browser=await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
    const page=await until(()=>browser.contexts().flatMap(c=>c.pages()).find(p=>/tauri/.test(p.url())),'UC WebView');
    const errors=[];page.on('pageerror',e=>errors.push(e.message));
    const invoke=async(cmd,args={})=>{
      const r=await page.evaluate(async([c,a])=>{try{return {ok:await window.__TAURI_INTERNALS__.invoke(c,a)};}catch(e){return {err:e?.error||String(e)};}},[cmd,args]);
      if('err' in r)throw new Error(r.err);return r.ok;};
    await page.waitForFunction(()=>document.getElementById('storage-status')?.textContent==='Changes saved in Unified Control');
    let st=await invoke('au05_state');
    assert.equal(st.enabled,false,'off by default');assert.equal(st.state,'disabled');
    let fake=(await invoke('au05_test_control',{patch:{op:'state'}})).fake;
    assert.deepEqual(fake.received,[],'nothing is sent while disabled');
    assert.equal(await page.locator('[data-page="vibekey"]').isVisible(),false);

    await page.locator('[data-page="general"]').click();
    await page.locator('#au05-enabled').check();
    await page.locator('[data-page="vibekey"]').waitFor({state:'visible'});
    const saved=JSON.parse(fs.readFileSync(path.join(home,'settings.json'),'utf8'));
    assert.equal(saved.au05.enabled,true,'the switch is saved at once');
    st=await until(async()=>{const s=await invoke('au05_state');return s.micNr?s:null;},'first read of the fake key');
    assert.equal(st.state,'connected');assert.equal(st.fake,true);
    assert.equal(st.standbyS,600);assert.equal(st.battery.percent,30);
    assert.equal(st.keys[2].mode,'unknown','key typing is not read at connect');
    fake=(await invoke('au05_test_control',{patch:{op:'state'}})).fake;
    assert.ok(fake.received.every(p=>!p.startsWith('01 06 50')),'no cmd 0x50 at connect');

    await page.locator('[data-page="vibekey"]').click();
    const pick=id=>page.locator(`.au05-diagram .au05-callout[data-au05-zone="${id}"]`).click();
    await page.waitForSelector('.au05-diagram');
    await pick('k3');
    await page.locator('[data-au05-read="k3"]').click();
    await until(async()=>(await invoke('au05_state')).keys[2].mode!=='unknown','K3 read on request');
    // The offline layer: K1 types Ctrl + D on its own. K1 has no UC action, so the layer sync writes it now.
    await page.locator('[data-au05-layer-view="offline"]').click();
    await pick('k1');
    await page.locator('[data-au05-offline-mode="keys"][data-control="k1"]').click();
    const keys=page.locator('input[data-au05-offline-keys="k1"]');await keys.fill('Ctrl + D');await keys.press('Tab');
    await until(async()=>(await invoke('au05_state')).keys[0].label==='Ctrl + D','K1 typing read back');
    await page.locator('[data-au05-layer-view="live"]').click();
    await page.locator('select[data-au05-set="standbyTime"]').selectOption('900');
    await until(async()=>(await invoke('au05_state')).standbyS===900,'standby read back');
    assert.deepEqual((await invoke('au05_test_control',{patch:{op:'state'}})).fake.hotkeysClaimed,[],'no hotkey without an action');
    await pick('k3');
    await page.locator('[data-au05-palette][data-value="uc:hotkey"]').click();
    const value=page.locator('input[data-au05-action-value="k3"]');await value.fill('Ctrl + Enter');await value.press('Tab');
    await until(async()=>(await invoke('au05_state')).actions.k3.value==='Ctrl + Enter','K3 UC action saved');
    assert.equal((await invoke('au05_state')).keys[2].hotkey.state,'ready');
    // The live layer: K3 has a UC action, so the key is set to type its own code F22.
    await until(async()=>{const k=(await invoke('au05_state')).keys[2];return k.mode==='uc'&&(!k.job||k.job.state==='done');},'K3 types F22');

    await invoke('au05_test_control',{patch:{op:'click',control:'k3'}});
    st=await until(async()=>{const s=await invoke('au05_state');return s.events.find(e=>e.control==='K3'&&e.action)?s:null;},'K3 hotkey action');
    const ev=st.events.find(e=>e.control==='K3'&&e.action);
    assert.equal(ev.code,'F22');assert.equal(ev.action,'ran Ctrl + Enter');
    await page.waitForFunction(()=>/ran Ctrl \+ Enter/.test(document.getElementById('au05-events')?.textContent||''));
    assert.match(await page.locator('.au05-callout[data-au05-zone="k3"]').textContent(),/Ctrl \+ Enter/,'the diagram shows the assignment');

    fake=(await invoke('au05_test_control',{patch:{op:'state'}})).fake;
    assert.deepEqual(fake.forbidden,[],'every command was on the allowlist');
    assert.ok(fake.received.includes('01 06 50 00 00 01 02 03 01 02 07'),'K1 Ctrl + D frame as proven');
    assert.ok(fake.received.includes('01 01 2c 02 84 03'),'standby 900 s (the fake log drops trailing zero bytes)');
    assert.deepEqual(fake.typed,['F22'],'K3 typed only its own code');
    assert.deepEqual(fake.hotkeysClaimed,[0x85],'only F22 is claimed (fake desktop: nothing real)');
    assert.deepEqual(fake.actionsRun,['press Ctrl + Enter']);

    // A stalling key pauses sending; the app stays responsive.
    await invoke('au05_test_control',{patch:{op:'stallWrites',value:true}});
    // 0.37.0: a change made while another is still being sent waits its turn, then stalls; either way the
    // write has a time limit and sending pauses.
    const t0=Date.now();
    const waitState=async(f,ms=8000)=>{const end=Date.now()+ms;for(;;){const st=await invoke('au05_state');if(f(st)||Date.now()>end)return st;await new Promise(r=>setTimeout(r,100));}};
    await invoke('au05_change',{change:{setting:'standbyTime',value:600}}).catch(e=>assert.match(String(e?.error||e),/did not accept/));
    const stalled=await waitState(st=>st.state==='paused'&&st.changes?.standbyTime?.state==='failed');
    assert.ok(Date.now()-t0<8000,'the write had a time limit');
    assert.equal(stalled.state,'paused');
    await page.locator('select[data-au05-set="standbyTime"]').selectOption('300');
    await page.waitForFunction(()=>/paused|did not accept/i.test(document.getElementById('au05-note')?.textContent||''));
    assert.ok((await invoke('app_state')).appVersion,'the host still answers');
    await invoke('au05_test_control',{patch:{op:'stallWrites',value:false}});
    await page.getByRole('button',{name:'Reconnect'}).click();
    await invoke('au05_change',{change:{setting:'standbyTime',value:600}});
    assert.equal((await waitState(st=>st.standbyS===600)).standbyS,600);

    // A real-key-only refusal cannot be tested with the fake; check the fake accepts an unverified set.
    await invoke('au05_change',{change:{setting:'sleepTime',value:7200}});
    await until(async()=>(await invoke('au05_state')).sleepS===7200,'power-off read back');

    await page.locator('[data-page="general"]').click();await page.locator('#au05-enabled').uncheck();
    await until(async()=>(await invoke('au05_state')).state==='disabled','disable');
    assert.deepEqual((await invoke('au05_test_control',{patch:{op:'state'}})).fake.hotkeysClaimed,[],'disable releases the hotkeys');
    assert.deepEqual(errors,[]);
    console.log('PASS: quiet native Vibe Key; opt-in switch saved; fake key read; key typing and standby changes read back; UC action through the claimed F22 hotkey (fake desktop) and release on disable; allowlist only; stall pause and Reconnect; disable. No physical device opened.');
    console.log(`Isolated test data: ${home}`);
    await browser.close().catch(()=>{});
  }finally{
    if(child.exitCode===null){
      const end=Date.now()+6000;
      do{const quit=spawn(exe,['--quit'],{env,windowsHide:true,stdio:'ignore'});const code=await new Promise(r=>quit.once('exit',r));if(code!==2)break;await delay(75);}while(Date.now()<end);
    }
    const result=await Promise.race([exited,delay(8000).then(()=>null)]);
    fs.writeFileSync(path.join(home,'test-output.log'),logs);
    if(!result)throw new Error('Quiet test instance did not finish its clean quit; its safety timer remains active.');
    if(result.code!==0)throw new Error(`Quiet debug instance exit code ${result.code}.`);
  }
}
run().catch(error=>{console.error(error);process.exitCode=1;});
