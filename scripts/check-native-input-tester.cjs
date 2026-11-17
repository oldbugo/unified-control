// Native, quiet: the Input tester in a debug UC with fake devices (UC_FAKE_DEVICE, UC_TEST_QUIET). Raw Input is never
// registered (simulated devices only); fake raw records go through the real decoder. No window, focus, sound or mic.
const fs=require('node:fs'),path=require('node:path'),os=require('node:os'),net=require('node:net'),assert=require('node:assert/strict');
const {spawn}=require('node:child_process');
const {chromium}=require(process.env.UC_PLAYWRIGHT_MODULE||'playwright');
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
async function until(f,what,ms=10000){const end=Date.now()+ms;for(;;){const v=await f();if(v)return v;if(Date.now()>end)throw new Error(`Timed out: ${what}`);await delay(100);}}
async function run(){
  const exe=path.resolve(process.env.UC_TEST_EXE||'');
  if(!exe.toLowerCase().includes(`${path.sep}debug${path.sep}`)||path.basename(exe).toLowerCase()!=='unified-control.exe'||!fs.existsSync(exe))
    throw new Error('Set UC_TEST_EXE to the debug unified-control.exe; installed/release binaries are refused.');
  const home=fs.mkdtempSync(path.join(os.tmpdir(),'uc-native-input-tester-'));
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
      const r=await page.evaluate(async([c,a])=>{try{return {ok:await window.__TAURI_INTERNALS__.invoke(c,a)};}catch(e){return {err:e?.error||e?.message||String(e)};}},[cmd,args]);
      if('err' in r)throw new Error(r.err);return r.ok;};
    await page.waitForFunction(()=>document.getElementById('storage-status')?.textContent==='Changes saved in Unified Control');
    assert.equal((await invoke('input_tester_poll',{since:0})).running,false,'nothing listens before the page opens');

    await page.locator('[data-page="inputtester"]').click();
    await page.waitForSelector('#it-page');
    await page.waitForFunction(()=>document.querySelectorAll('.it-device').length===4);
    assert.match(await page.locator('#it-mode').textContent(),/Simulated devices/);
    // Raw-Input-shaped records: Ctrl + Shift + D on the keyboard, F22 from the Vibe Key, a click and a knob turn.
    const key=(device,make,flags,vk)=>({type:'key',device,make,flags,vk});
    await invoke('input_tester_test_inject',{records:[
      key('fake-keyboard',0x1D,0,0x11),key('fake-keyboard',0x2A,0,0x10),key('fake-keyboard',0x20,0,0x44),key('fake-keyboard',0x20,0,0x44),
      key('fake-keyboard',0x20,1,0x44),key('fake-keyboard',0x2A,1,0x10),key('fake-keyboard',0x1D,1,0x11),
      key('fake-au05',0x6D,0,0x85),key('fake-au05',0x6D,1,0x85),
      {type:'mouse',device:'fake-mouse',buttons:1,data:0,dx:0,dy:0},{type:'mouse',device:'fake-mouse',buttons:2,data:0,dx:0,dy:0},
      {type:'mouse',device:'fake-mouse',buttons:0,data:0,dx:5,dy:2},
      {type:'hid',device:'fake-au05-media',usages:[[12,0xE9]],bytes:[2,0xE9,0]},{type:'hid',device:'fake-au05-media',usages:[],bytes:[2,0,0]},
    ]});
    await page.waitForFunction(()=>document.querySelectorAll('#it-log-body tr').length===13);
    const rows=await page.locator('#it-log-body tr').allTextContents();
    assert.match(rows[12],/Keyboard \(simulated\)Key downLeft Ctrl0xA20x1D/);
    assert.match(rows[9],/Repeat/);
    assert.match(rows[5],/AU05 \(simulated\)Key downF220x850x6D/);
    assert.match(rows[0],/HID releaseVolume Up/);
    assert.equal(await page.locator('[data-it-code="KeyD"].was').count()>=1,true);
    assert.equal(await page.locator('[data-it-code="AudioVolumeUp"]').getAttribute('class'),'it-key it-flat was');
    assert.match(await page.locator('[data-it-count="fake-keyboard"]').textContent(),/3 keys · 1 repeat/);
    assert.match(await page.locator('[data-it-count="fake-mouse"]').textContent(),/1 click · 1 move/,'moves are counted, not logged');
    // Mouse moves are logged when asked.
    await page.locator('#it-moves').check();
    await until(async()=>(await invoke('input_tester_poll',{since:0})).logMoves===true,'moves on');
    await invoke('input_tester_test_inject',{records:[{type:'mouse',device:'fake-mouse',buttons:0,data:0,dx:-3,dy:1}]});
    await page.waitForFunction(()=>/Move -3,1/.test(document.querySelector('#it-log-body tr')?.textContent||''));

    // Saving a recording writes only to this instance's folder.
    const wav=Buffer.alloc(44);wav.write('RIFF',0);wav.write('WAVE',8);
    const saved=await invoke('input_tester_save_audio',{wav:wav.toString('base64'),stamp:'2026-11-17-120000'});
    assert.ok(saved.path.startsWith(path.join(home,'input-tester')),saved.path);
    assert.ok(fs.existsSync(saved.path));

    // Leaving the page stops listening.
    await page.locator('[data-page="general"]').click();
    await until(async()=>(await invoke('input_tester_poll',{since:0})).running===false,'stopped on leave');
    const log=fs.readFileSync(path.join(home,'device.log'),'utf8');
    assert.doesNotMatch(log,/input tester: raw input on/,'a quiet fake instance never registers Raw Input');
    assert.deepEqual(errors,[]);
    console.log('PASS: quiet native Input tester; simulated devices only (no Raw Input registered); fake records decoded (keys, repeat, F22, mouse, HID knob); moves logged on request; recording saved only into the test folder; stops when the page closes.');
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
