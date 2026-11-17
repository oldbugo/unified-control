// Headless fixture only: the Input tester page. Never real devices, never a microphone, never sound.
// Part 1: pure logic (layout, combination, log columns, WAV). Part 2: the page against a fake host that feeds
// Raw-Input-shaped events. Part 3: browser preview (this window's DOM events). Optional screenshots: UC_PREVIEW_OUTPUT.
const fs=require('node:fs'),path=require('node:path'),http=require('node:http'),assert=require('node:assert/strict');
const {chromium}=require(process.env.UC_PLAYWRIGHT_MODULE||'playwright');
const root=path.resolve(__dirname,'../public'),out=process.env.UC_PREVIEW_OUTPUT||'';
const server=http.createServer((req,res)=>{
  const file=path.resolve(root,'.'+new URL(req.url,'http://localhost').pathname.replace(/\/$/,'/index.html'));
  if(!file.startsWith(root+path.sep)){res.writeHead(403);return res.end();}
  fs.readFile(file,(error,data)=>{res.writeHead(error?404:200,{'Content-Type':file.endsWith('.js')?'text/javascript':file.endsWith('.css')?'text/css':'text/html'});res.end(error?'Not found':data);});
});
const fixture=()=>{
  const keys=Array.from({length:14},()=>({a:{kind:'text',text:''},b:{kind:'text',text:''},action:'none',value:''}));
  const layout={version:1,currentProfile:'p',currentPage:'pg',profiles:[{id:'p',name:'Fixture',pages:[{id:'pg',name:'Main',keys}]}]};
  const devices=[
    {id:'kbd',kind:'keyboard',name:'Keyboard (simulated)',role:'Keyboard',vid:0x046D,pid:0xC33F,simulated:true},
    {id:'mouse',kind:'mouse',name:'Mouse (simulated)',role:'Mouse',vid:0x046D,pid:0xC08B,simulated:true},
    {id:'au05',kind:'keyboard',name:'AU05 (simulated)',role:'Keyboard',vid:0xFFF1,pid:0x00DD,simulated:true},
    {id:'au05-media',kind:'hid',name:'AU05 (simulated)',role:'Media keys',vid:0xFFF1,pid:0x00DD,simulated:true},
  ];
  const host={running:false,seq:0,events:[],counts:{},claimed:[0x85],logMoves:false,now:0};
  window.__host=host;window.__calls=[];
  window.__push=list=>{for(const e of list){host.seq++;host.now+=10;const ev={seq:host.seq,t:host.now,mods:[],...e};host.events.push(ev);
    const c=host.counts[e.device]||(host.counts[e.device]={down:0,up:0,repeat:0,buttons:0,wheel:0,moves:0,hid:0,last:null});c.last=host.now;
    if(e.kind==='key')c[e.action]++;if(e.kind==='button'&&e.action==='down')c.buttons++;if(e.kind==='wheel')c.wheel++;if(e.kind==='hid')c.hid++;}};
  const view=since=>({running:true,mode:'simulated',error:null,now:host.now,seq:host.seq,dropped:0,logMoves:host.logMoves,claimed:host.claimed,devices,counts:host.counts,events:host.events.filter(e=>e.seq>since)});
  window.__TAURI_INTERNALS__={transformCallback:()=>1,invoke:async(command,args)=>{
    if(command.startsWith('input_tester'))window.__calls.push({command,...args});
    if(command==='app_state')return {appVersion:'fixture',page:'home',fake:'1',dock:{enabled:false,state:'disabled'},cooler:{enabled:false,state:'disabled'}};
    if(command==='profiles_state')return {state:layout,revision:'fixture'};
    if(command==='dock_state')return {enabled:false,state:'disabled',message:'Dock disabled',fake:true};
    if(command==='cooler_state')return {enabled:false,state:'disabled',diagnosticsEnabled:true};
    if(command==='input_tester_start'){if(host.hidden)return {running:false,hidden:true};if(!host.running||args.fresh)Object.assign(host,{running:true,seq:0,events:[],counts:{},now:0});return view(0);}
    if(command==='input_tester_poll')return host.hidden?(host.running=false,{running:false,hidden:true}):host.running?view(args.since):{running:false};
    if(command==='input_tester_configure'){if('logMoves' in args.patch)host.logMoves=args.patch.logMoves;if(args.patch.clear)host.counts={};return view(host.seq);}
    if(command==='input_tester_stop'){host.running=false;return {running:false};}
    if(command==='input_tester_save_audio')return {path:'C:\\fixture\\Microphone test.wav'};
    if(command==='settings_update')return {appVersion:'fixture',page:args.patch.page||'home',fake:'1'};
    return {};
  }};
};
(async()=>{let browser;try{
  await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
  const url=`http://127.0.0.1:${server.address().port}/`;
  browser=await chromium.launch({headless:true});
  const context=await browser.newContext({viewport:{width:1440,height:1000}});
  await context.grantPermissions(['clipboard-read','clipboard-write'],{origin:url.slice(0,-1)});
  const page=await context.newPage(),errors=[];
  page.on('pageerror',e=>errors.push(e.message));
  await page.route('**/cooler/index.html',route=>route.fulfill({contentType:'text/html',body:'<!doctype html><title>Fixture cooler</title>'}));

  // ---------- part 1: pure logic ----------
  await page.goto(url+'css/tokens.css');
  const pure=await page.evaluate(async()=>{
    const m=await import('/js/input-tester-model.js');
    const ansi=m.layout({iso:false,numpad:true}),iso=m.layout({iso:true,numpad:false});
    const model=new m.InputModel();
    const ev=(code,action,extra={})=>({t:1,device:'kbd',kind:'key',action,code,name:code,mods:[],...extra});
    model.apply(ev('ControlLeft','down'));model.apply(ev('ShiftLeft','down',{mods:['Ctrl']}));
    const mods=[...model.combo];
    model.apply(ev('KeyD','down',{name:'D',mods:['Ctrl','Shift']}));
    const combo=[...model.combo];
    model.apply(ev('KeyD','repeat',{name:'D'}));model.apply(ev('KeyD','up'));
    model.apply({t:2,device:'media',kind:'hid',action:'down',code:'Hid0C:00E9',name:'Volume Up',mods:[]});
    const wav=m.encodeWav([new Float32Array([0,1,-1])],8000);
    const cols=m.logColumns({t:12.5,device:'au05',kind:'key',action:'down',code:'F22',name:'F22',vk:0x85,scan:0x6D,ext:false,mods:[],claimed:true},{t:10},()=> 'AU05');
    return {mainCols:ansi[0].cols,isoCols:iso[0].cols,blocks:ansi.map(b=>b.id),isoBlocks:iso.map(b=>b.id),
      enterKeys:iso[0].keys.filter(k=>k.code==='Enter').length,f24:m.drawnCodes().has('F24'),mods,combo,
      held:[...model.heldCodes()],pressed:[...model.pressed].sort(),counts:model.counts.kbd,
      riff:String.fromCharCode(...wav.slice(0,4)),wavLen:wav.length,samples:[...new Int16Array(wav.buffer.slice(44))],
      b64:m.toBase64(new Uint8Array([77,97,110])),cols,db:Math.round(m.dbfs(.5)),
      dom:m.domEvent({type:'keydown',code:'KeyA',key:'a',keyCode:65,repeat:false,ctrlKey:true,shiftKey:false,altKey:false,metaKey:false},5,new Set())};
  });
  assert.equal(pure.mainCols,60,'the main block is 15 key units (quarter-unit grid)');
  assert.equal(pure.isoCols,60,'ISO rows are as wide as ANSI rows');
  assert.deepEqual(pure.blocks,['main','nav','pad']);assert.deepEqual(pure.isoBlocks,['main','nav'],'the number pad can be hidden');
  assert.equal(pure.enterKeys,2,'ISO Enter spans two rows');
  assert.equal(pure.f24,true,'F13–F24 are drawn');
  assert.deepEqual(pure.mods,['Ctrl','Shift'],'modifiers alone are a combination');
  assert.deepEqual(pure.combo,['Ctrl','Shift','D']);
  assert.deepEqual(pure.held.sort(),['AudioVolumeUp','ControlLeft','ShiftLeft'],'a HID media usage lights the media key');
  assert.deepEqual(pure.pressed,['AudioVolumeUp','ControlLeft','KeyD','ShiftLeft']);
  assert.equal(pure.counts.down,3);assert.equal(pure.counts.repeat,1);assert.equal(pure.counts.up,1);
  assert.equal(pure.riff,'RIFF');assert.equal(pure.wavLen,50);assert.deepEqual(pure.samples,[0,32767,-32768]);
  assert.equal(pure.b64,'TWFu');assert.equal(pure.db,-6);
  assert.deepEqual(pure.cols,['12.5','+2.5','AU05','Key down','F22','0x85','0x6D','','','Claimed by Unified Control']);
  assert.equal(pure.dom.code,'KeyA');assert.deepEqual(pure.dom.mods,['Ctrl']);assert.equal(pure.dom.name,'A');

  // ---------- part 2: the page with a fake host ----------
  await page.addInitScript(fixture);
  await page.goto(url);
  await page.waitForFunction(()=>document.getElementById('storage-status')?.textContent==='Changes saved in Unified Control');
  const nav=page.locator('.nav-item[data-page="inputtester"]');
  assert.equal(await nav.isVisible(),true,'Input tester is always in the sidebar');
  assert.equal(await page.evaluate(()=>[...document.querySelectorAll('.nav-item')].at(-1).dataset.page),'inputtester','it is the last sidebar entry');
  assert.deepEqual(await page.evaluate(()=>window.__calls),[],'nothing listens before the page opens');
  await nav.click();
  await page.waitForSelector('#it-page');
  await page.waitForFunction(()=>document.querySelectorAll('.it-device').length===4);
  assert.equal((await page.evaluate(()=>window.__calls))[0].command,'input_tester_start');
  assert.match(await page.locator('#it-mode').textContent(),/Simulated devices/);
  assert.equal(await page.locator('[data-it-code="F22"]').count()>=2,true,'F22 is on the keyboard drawing and the Vibe Key card');
  assert.match(await page.locator('.it-device[data-dev="au05"]').textContent(),/AU05 \(simulated\)[\s\S]*FFF1:00DD/);

  // The Vibe Key's K3 types F22, which UC claims.
  await page.evaluate(()=>window.__push([{device:'au05',kind:'key',action:'down',code:'F22',name:'F22',vk:0x85,scan:0x6D,ext:false,claimed:true}]));
  await page.waitForFunction(()=>document.querySelector('.it-board-card [data-it-code="F22"]')?.classList.contains('held'));
  assert.match(await page.locator('#it-log-body tr').first().textContent(),/AU05 \(simulated\)Key downF220x850x6D.*Claimed by Unified Control/);
  assert.equal(await page.locator('.it-vibe-item[data-it-code="F22"]').getAttribute('class'),'it-vibe-item held');
  assert.match(await page.locator('.it-vibe-item[data-it-code="F22"]').textContent(),/Claimed by Unified Control/);
  assert.match(await page.locator('.it-vibe-item[data-it-code="F20"]').textContent(),/Not claimed/);
  await page.evaluate(()=>window.__push([{device:'au05',kind:'key',action:'up',code:'F22',name:'F22',vk:0x85,scan:0x6D,ext:false}]));
  await page.waitForFunction(()=>document.querySelector('.it-board-card [data-it-code="F22"]')?.classList.contains('was'));

  // A combination on the main keyboard, a mouse click, a wheel turn and a knob (HID volume).
  await page.evaluate(()=>window.__push([
    {device:'kbd',kind:'key',action:'down',code:'ControlLeft',name:'Left Ctrl',vk:0xA2,scan:0x1D,ext:false},
    {device:'kbd',kind:'key',action:'down',code:'ShiftLeft',name:'Left Shift',vk:0xA0,scan:0x2A,ext:false,mods:['Ctrl']},
    {device:'kbd',kind:'key',action:'down',code:'KeyD',name:'D',vk:0x44,scan:0x20,ext:false,mods:['Ctrl','Shift']},
    {device:'kbd',kind:'key',action:'repeat',code:'KeyD',name:'D',vk:0x44,scan:0x20,ext:false,mods:['Ctrl','Shift']},
    {device:'mouse',kind:'button',action:'down',code:'MouseLeft',name:'Left button'},
    {device:'mouse',kind:'button',action:'up',code:'MouseLeft',name:'Left button'},
    {device:'mouse',kind:'wheel',action:'turn',code:'WheelDown',name:'Wheel down',delta:-120},
    {device:'au05-media',kind:'hid',action:'down',code:'Hid0C:00E9',name:'Volume Up',bytes:'02 E9 00'},
    {device:'au05-media',kind:'hid',action:'up',code:'Hid0C:00E9',name:'Volume Up'},
  ]));
  await page.waitForFunction(()=>document.querySelectorAll('#it-log-body tr').length===11);
  assert.equal(await page.locator('#it-combo').getAttribute('aria-label'),'Last combination: Volume Up','the knob (HID) press is the newest combination');
  assert.equal(await page.locator('.it-board-card [data-it-code="KeyD"]').getAttribute('class'),'it-key held');
  assert.equal(await page.locator('[data-it-code="AudioVolumeUp"]').getAttribute('class'),'it-key it-flat was','the knob lit the media key');
  assert.match(await page.locator('#it-log-body tr').nth(5).textContent(),/Repeat/);
  assert.match(await page.locator('#it-log-body tr').nth(2).textContent(),/Wheel -120/);
  assert.match(await page.locator('[data-it-count="kbd"]').textContent(),/3 keys · 1 repeat/);
  assert.match(await page.locator('[data-it-count="mouse"]').textContent(),/1 click · 1 wheel/);
  // A combination with a letter.
  await page.evaluate(()=>window.__push([{device:'kbd',kind:'key',action:'down',code:'KeyE',name:'E',vk:0x45,scan:0x12,ext:false,mods:['Ctrl','Shift']}]));
  await page.waitForFunction(()=>document.getElementById('it-combo').getAttribute('aria-label')==='Last combination: Ctrl + Shift + E');
  assert.equal(await page.locator('#it-combo kbd').count(),3);

  // Filter by device.
  await page.locator('.it-device[data-dev="au05"]').click();
  assert.equal(await page.locator('#it-filter').inputValue(),'au05');
  assert.equal(await page.locator('#it-log-body tr').count(),2,'only the AU05 events');
  await page.locator('#it-filter').selectOption('');
  assert.equal(await page.locator('#it-log-body tr').count(),12);

  // Pause: the log stops, the drawing goes on.
  await page.locator('[data-it="pause"]').click();
  await page.evaluate(()=>window.__push([{device:'kbd',kind:'key',action:'down',code:'KeyQ',name:'Q',vk:0x51,scan:0x10,ext:false}]));
  await page.waitForFunction(()=>document.querySelector('.it-board-card [data-it-code="KeyQ"]')?.classList.contains('held'));
  assert.equal(await page.locator('#it-log-body tr').count(),12,'paused');
  assert.match(await page.locator('#it-log-note').textContent(),/1 new event not shown/);
  await page.locator('[data-it="pause"]').click();

  // Copy, mouse moves, clear.
  await page.locator('[data-it="copy"]').click();
  await page.waitForFunction(()=>/Copied 12 events/.test(document.getElementById('it-log-note').textContent));
  const clip=await page.evaluate(()=>navigator.clipboard.readText());
  assert.match(clip.split('\n')[0],/^Time \(ms\)\tGap \(ms\)\tDevice\tType\tKey\tVK/);
  assert.match(clip,/AU05 \(simulated\)\tKey down\tF22\t0x85\t0x6D\t\t\tClaimed by Unified Control/);
  await page.locator('#it-moves').check();
  await page.waitForFunction(()=>window.__host.logMoves===true);
  await page.locator('[data-it="clear"]').click();
  assert.equal(await page.locator('#it-log-body tr').count(),0);
  assert.equal(await page.locator('.it-key.was').count(),0,'Clear removes the pressed marks');
  assert.equal(await page.locator('#it-combo').getAttribute('aria-label'),'No combination yet');

  // A key the drawing does not show.
  await page.evaluate(()=>window.__push([{device:'au05-media',kind:'hid',action:'down',code:'Hid0C:00CF',name:'Voice Command',bytes:'02 CF 00'}]));
  await page.waitForFunction(()=>!document.getElementById('it-other-wrap').hidden);
  assert.equal(await page.locator('#it-other').textContent(),'Voice Command');

  // Typing on the test area presses nothing on the page (Space would click a focused button).
  await page.locator('#it-surface').click();
  const before=await page.evaluate(()=>window.__calls.length);
  await page.keyboard.press('Space');await page.keyboard.press('F11');
  assert.equal(await page.evaluate(()=>document.activeElement.id),'it-surface');

  // ---------- audio (simulated: no microphone, no sound) ----------
  await page.waitForFunction(()=>document.querySelectorAll('#it-mic-select option').length===3);
  assert.match(await page.locator('#it-mic-select').textContent(),/Microphone \(AU05\) \(simulated\)/);
  await page.locator('#it-mic-select').selectOption('fake-au05');
  await page.locator('[data-it="mic"]').click();
  await page.waitForFunction(()=>/48\.0 kHz/.test(document.querySelector('.it-format')?.textContent||''));
  const fmt=await page.locator('.it-format').textContent();
  assert.match(fmt,/1 \(mono\)/);assert.match(fmt,/16-bit/);assert.match(fmt,/Off \(raw sound\)/);
  await page.waitForFunction(()=>/Level -?\d+ dB · Peak/.test(document.getElementById('it-meter-text').textContent));
  assert.equal(await page.evaluate(()=>window.__ucInputTesterAudio.opened.id),'fake-au05');
  assert.equal(await page.locator('[data-it="save"]').isDisabled(),true,'nothing to save before a recording');
  await page.locator('[data-it="record"]').click();
  await page.waitForFunction(()=>!document.querySelector('[data-it="play"]').disabled);
  await page.locator('#it-out-select').selectOption('fake-headphones');
  await page.locator('[data-it="play"]').click();
  await page.waitForFunction(()=>window.__ucInputTesterAudio.played.some(p=>p.what==='recording'));
  assert.deepEqual(await page.evaluate(()=>window.__ucInputTesterAudio.played[0]),{what:'recording',outputId:'fake-headphones',frames:40000});
  assert.equal((await page.evaluate(()=>window.__calls)).some(c=>c.command==='input_tester_save_audio'),false,'nothing is saved without Save');
  await page.locator('[data-it="save"]').click();
  await page.waitForFunction(()=>/Saved: /.test(document.getElementById('it-audio-note').textContent));
  const save=(await page.evaluate(()=>window.__calls)).find(c=>c.command==='input_tester_save_audio');
  assert.match(save.wav,/^UklGR/,'a WAV file');assert.match(save.stamp,/^\d{4}-\d\d-\d\d-\d{6}$/);
  await page.locator('[data-it="tone"][data-side="left"]').click();
  await page.waitForFunction(()=>window.__ucInputTesterAudio.played.some(p=>p.what==='tone'));
  assert.deepEqual(await page.evaluate(()=>window.__ucInputTesterAudio.played.at(-1)),{what:'tone',side:'left',outputId:'fake-headphones'});

  if(out){fs.mkdirSync(out,{recursive:true});await page.evaluate(()=>document.getElementById('pane').scrollTop=0);await page.screenshot({path:path.join(out,'input-tester-wide.png'),fullPage:false});
    await page.evaluate(()=>{const p=document.getElementById('pane');p.scrollTop=document.querySelector('.it-log-card').offsetTop-24;});await page.screenshot({path:path.join(out,'input-tester-wide-2.png')});}

  // Narrow window: nothing spills sideways (the log scrolls inside its own box).
  await page.setViewportSize({width:420,height:900});
  await page.waitForTimeout(150);
  const spill=await page.evaluate(()=>{const p=document.getElementById('pane'),r=p.getBoundingClientRect().right;return {page:document.documentElement.scrollWidth-innerWidth,pane:p.scrollWidth-p.clientWidth,wide:[...p.querySelectorAll('*')].filter(e=>e.getBoundingClientRect().right>r+1&&!e.closest('.it-log-wrap')).slice(0,6).map(e=>`${e.tagName}.${e.className}#${e.id}`)};});
  assert.ok(spill.page<=0&&spill.pane<=0,`no sideways scrolling at 420 px: ${JSON.stringify(spill)}`);
  const board=await page.evaluate(()=>{const s=document.getElementById('it-surface').getBoundingClientRect(),m=document.querySelector('.it-block-main').getBoundingClientRect();return {s:s.right,m:m.right};});
  assert.ok(board.m<=board.s+1,'the keyboard fits its card');
  if(out){await page.evaluate(()=>document.getElementById('pane').scrollTop=0);await page.screenshot({path:path.join(out,'input-tester-narrow.png')});
    await page.evaluate(()=>{const p=document.getElementById('pane');p.scrollTop=document.getElementById('it-mic-card').offsetTop-24;});await page.screenshot({path:path.join(out,'input-tester-narrow-2.png')});}
  await page.setViewportSize({width:1440,height:1000});

  // Leaving the page stops listening and closes the microphone.
  await page.locator('.nav-item[data-page="general"]').click();
  await page.waitForFunction(()=>window.__calls.at(-1)?.command==='input_tester_stop');
  assert.equal(await page.evaluate(()=>window.__host.running),false);
  assert.equal(await page.evaluate(()=>window.__ucInputTesterAudio.opened),null,'the microphone is closed');
  // Coming back starts listening again; a host that stopped by itself is started again.
  await nav.click();
  await page.waitForFunction(()=>window.__calls.filter(c=>c.command==='input_tester_start').length===2);
  await page.evaluate(()=>{window.__host.running=false;});
  await page.waitForFunction(()=>window.__calls.filter(c=>c.command==='input_tester_start').length===3);
  assert.equal(await page.locator('#it-log-body tr').count(),0,'a fresh session');
  // Closed to the tray: the host stops listening and the page waits until the window is shown again.
  await page.evaluate(()=>{window.__host.hidden=true;});
  await page.waitForFunction(()=>/Paused while the window is hidden/.test(document.getElementById('it-mode').textContent));
  assert.equal(await page.evaluate(()=>window.__host.running),false);
  await page.evaluate(()=>{window.__host.hidden=false;});
  await page.waitForFunction(()=>/Simulated devices/.test(document.getElementById('it-mode').textContent));
  void before;

  // ---------- part 3: browser preview (no app) ----------
  const preview=await context.newPage();
  preview.on('pageerror',e=>errors.push(e.message));
  await preview.route('**/cooler/index.html',route=>route.fulfill({contentType:'text/html',body:'<!doctype html><title>Fixture cooler</title>'}));
  await preview.addInitScript(()=>{window.__UC_FAKE_AUDIO__=true;try{if(!sessionStorage.getItem('it-fresh')){localStorage.clear();sessionStorage.setItem('it-fresh','1');}}catch{}});
  await preview.goto(url);
  await preview.locator('.nav-item[data-page="inputtester"]').click();
  assert.match(await preview.locator('#it-mode').textContent(),/Browser preview/);
  await preview.locator('#it-surface').click({position:{x:5,y:5}});
  await preview.keyboard.down('Control');await preview.keyboard.down('Shift');await preview.keyboard.press('KeyD');
  await preview.keyboard.up('Shift');await preview.keyboard.up('Control');
  assert.equal(await preview.locator('#it-combo').getAttribute('aria-label'),'Last combination: Ctrl + Shift + D');
  assert.match(await preview.locator('#it-log-body').textContent(),/This window/);
  assert.match(await preview.locator('#it-log-body').textContent(),/Button down/);
  assert.equal(await preview.locator('.it-device').count(),1);
  await preview.locator('[data-it="numpad"]').click();
  assert.equal(await preview.locator('.it-block-pad').count(),0,'number pad hidden');
  await preview.locator('#it-layout').selectOption('iso');
  assert.equal(await preview.locator('.it-block-main [data-it-code="IntlBackslash"]').count(),1,'ISO key shown');
  await preview.reload();
  await preview.locator('.nav-item[data-page="inputtester"]').click();
  assert.equal(await preview.locator('.it-block-pad').count(),0,'the drawing choice is remembered');
  assert.equal(await preview.locator('#it-layout').inputValue(),'iso');

  assert.deepEqual(errors,[]);
  console.log('input tester checks passed');
}finally{await browser?.close();server.close();}})().catch(error=>{console.error(error);process.exitCode=1;});
