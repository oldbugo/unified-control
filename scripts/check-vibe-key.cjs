// Headless fixture only: the Vibe Key (AU05) switch, Home card and page. Never real hardware.
const fs=require('node:fs'),path=require('node:path'),http=require('node:http'),os=require('node:os'),assert=require('node:assert/strict');
const {chromium}=require(process.env.UC_PLAYWRIGHT_MODULE||'playwright');
const root=path.resolve(__dirname,'../public');
const server=http.createServer((req,res)=>{
  const file=path.resolve(root,'.'+new URL(req.url,'http://localhost').pathname.replace(/\/$/,'/index.html'));
  if(!file.startsWith(root+path.sep)){res.writeHead(403);return res.end();}
  fs.readFile(file,(error,data)=>{res.writeHead(error?404:200,{'Content-Type':file.endsWith('.js')?'text/javascript':file.endsWith('.css')?'text/css':'text/html'});res.end(error?'Not found':data);});
});
(async()=>{let browser;try{
  await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
  browser=await chromium.launch({headless:true});
  const page=await browser.newPage({viewport:{width:1440,height:1000}}),errors=[];
  page.on('pageerror',e=>errors.push(e.message));
  await page.route('**/cooler/index.html',route=>route.fulfill({contentType:'text/html',body:'<!doctype html><title>Fixture cooler</title>'}));
  await page.addInitScript(()=>{
    const keys=Array.from({length:14},()=>({a:{kind:'text',text:''},b:{kind:'text',text:''},action:'none',value:''}));
    const layout={version:1,currentProfile:'p',currentPage:'pg',profiles:[{id:'p',name:'Fixture',pages:[{id:'pg',name:'Main',keys}]}]};
    const none={kind:'none',value:''};
    // A real key (fake:false): only checked settings may be sent; the knob press typing is not checked yet.
    window.__au05={enabled:false,fake:false,state:'disabled',message:'Vibe Key is disabled',battery:null,standbyS:null,sleepS:null,light:null,micNr:null,
      keys:[],events:[],actions:{k1:none,k2:none,k3:none,knobPress:none},hotkeysAvailable:true,
      layer:{target:'live',offlineOnKey:true,offline:{k1:{factory:true,label:'Factory default (Win + H)'},k2:{factory:true,label:'Factory default (Enter)'},k3:{factory:true,label:'Factory default (Esc)'},
        knobPress:{factory:true,label:'Factory default (Backspace)'},knobRight:{factory:true,label:'Factory default (scroll down)'},knobLeft:{factory:true,label:'Factory default (scroll up)'}}},
      catalogue:{apps:[{id:'claude',name:'Claude',verified:true},{id:'chatgpt',name:'ChatGPT',verified:false}],actions:[
        {id:'claudeDictation',group:'Claude',label:'Start dictation',keys:'Ctrl + D',status:'verified',note:'Checked on this PC',offlineKeys:'Ctrl + D'},
        {id:'claudeDictateSend',group:'Claude',label:'Dictate & send',keys:'Ctrl + D, then Enter',status:'unverified',note:'New: first press starts dictation',offlineKeys:null},
        {id:'chatgptNewChat',group:'ChatGPT',label:'New chat',keys:'Ctrl + Shift + O',status:'unverified',note:'Third-party report',offlineKeys:'Ctrl + Shift + O'},
        {id:'volumeUp',group:'Windows / media',label:'Volume up',keys:'Volume up',status:'verified',note:'Standard media key'}]},
      checked:{standbyTime:true,keyTyping:true,sleepTime:false,lightMode:false,lightBrightness:false,lightRecord:false,hooksMode:false,micNr:false},
      lightRange:{modeMax:3,brightnessMax:10},micNrMax:3};
    const key=(key,name,mode,label,factory,ownCode,checked=true)=>({key,name,mode,label,keys:mode==='keys'?label:null,factory,ownCode,canSet:true,checked,hotkey:{state:'off'}});
    const hotkeys=()=>{for(const k of window.__au05.keys)k.hotkey={state:window.__au05.enabled&&window.__au05.actions[k.key].kind!=='none'?'ready':'off'};};
    const connected=()=>Object.assign(window.__au05,{state:'connected',message:'Connected',battery:{millivolts:3608,percent:30},standbyS:600,sleepS:3600,
      light:{mode:2,brightness:7,records:[[2,10,2,7,2],[2,10,2,7,2],[2,10,2,7,2],[1,10,2,7,2]]},micNr:{level:0,a:100,b:100},hooksMode:0,
      keys:[key('k1','K1','unknown',null,'Win + H','F20'),key('k2','K2','factory','Factory default (Enter)','Enter','F21'),
            key('k3','K3','uc','F22','Esc','F22'),key('knobPress','knob press','unknown',null,'Backspace','F19',false)],
      events:[{at:Date.now(),control:'knob right',pressed:true,code:'F23',action:null}]});
    window.__writes=[];
    window.__TAURI_INTERNALS__={transformCallback:()=>1,invoke:async(command,args)=>{
      if(command==='app_state')return {appVersion:'fixture',page:'home',fake:true,dock:{enabled:false,state:'disabled'},cooler:{enabled:false,state:'disabled'},au05:structuredClone(window.__au05)};
      if(command==='profiles_state')return {state:layout,revision:'fixture'};
      if(command==='dock_state')return {enabled:false,state:'disabled',message:'Dock disabled',fake:true};
      if(command==='cooler_state')return {enabled:false,state:'disabled',diagnosticsEnabled:true};
      if(command==='au05_state')return structuredClone(window.__au05);
      if(command==='au05_configure'){
        window.__writes.push({command,...args});
        if('enabled' in args.patch){window.__au05.enabled=args.patch.enabled;if(args.patch.enabled)connected();else window.__au05.state='disabled';}
        if(args.patch.action)window.__au05.actions[args.patch.action.control]={kind:args.patch.action.kind,value:args.patch.action.value,app:args.patch.action.app||''};
        if(args.patch.offline){const o=args.patch.offline;window.__au05.layer.offline[o.control]=o.mode==='keys'?{factory:false,keys:o.keys,label:o.keys}:{factory:true,label:'Factory default'};}
        if(args.patch.layer)window.__au05.layer.target=args.patch.layer;
        if('reopenWhereLeft' in args.patch){window.__au05.menu=window.__au05.menu||{};window.__au05.menu.reopenWhereLeft=args.patch.reopenWhereLeft;}
        if(args.patch.menuControls){window.__au05.menu=window.__au05.menu||{};window.__au05.menu.controls={...window.__au05.menu.controls,...args.patch.menuControls};}
        hotkeys();return structuredClone(window.__au05);
      }
      if(command==='au05_change'){
        window.__writes.push({command,...args});
        const c=args.change;
        if(c.setting==='standbyTime')window.__au05.standbyS=c.value;
        if(c.setting==='lightRecord'){const f=['type','time','breatheLevel','breatheBrightness','alwaysOnBrightness'].indexOf(c.field);window.__au05.light.records[c.record][f]=c.value;}
        if(c.setting==='retryJob'){const k=window.__au05.keys.find(k=>k.key===c.key);if(k)k.job=null;window.__au05.keySync={blocked:false,pending:false,waiting:false};}
        return structuredClone(window.__au05);
      }
      if(command==='au05_read_all'){window.__writes.push({command});return structuredClone(window.__au05);}
      if(command==='au05_read_key'){window.__writes.push({command,...args});const k=window.__au05.keys.find(k=>k.key===args.key);k.mode='factory';k.label=`Factory default (${k.factory})`;return structuredClone(window.__au05);}
      if(command==='au05_reconnect'){window.__writes.push({command});return structuredClone(window.__au05);}
      if(command==='settings_update')return {appVersion:'fixture',page:args.patch.page||'home',fake:true};
      return {};
    }};
  });
  await page.goto(`http://127.0.0.1:${server.address().port}/`);
  await page.waitForFunction(()=>document.getElementById('storage-status')?.textContent==='Changes saved in Unified Control');
  const nav=id=>page.locator(`.nav-item[data-page="${id}"]`);
  const writes=()=>page.evaluate(()=>window.__writes);
  assert.equal(await nav('vibekey').isHidden(),true,'the Vibe Key is hidden until enabled');
  assert.deepEqual(await writes(),[],'presentation enables nothing');

  await nav('general').click();
  assert.equal(await page.locator('#au05-enabled').isChecked(),false,'off by default');
  await page.locator('#au05-enabled').check();
  await page.waitForFunction(()=>!document.querySelector('[data-page="vibekey"]').hidden);
  assert.deepEqual((await writes())[0],{command:'au05_configure',patch:{enabled:true}});
  await nav('home').click();
  assert.match(await page.locator('.device-card[data-go="vibekey"]').textContent(),/battery 30 %/);

  await nav('vibekey').click();
  // The diagram is the main element: five zones, each with its current assignment.
  const pick=id=>page.locator(`.au05-diagram .au05-callout[data-au05-zone="${id}"]`).click();
  // The panel: a mode switch (UC action, Types keys, Factory) and action cards grouped by app.
  const mode=key=>page.locator(`[data-au05-mode="${key}"]`);
  const segment=(key,v)=>page.locator(`[data-au05-mode="${key}"] [data-au05-segment="${v}"]`).click();
  // The action palette is always on the page: a click sets the selected part (picked on the drawing first).
  const card=value=>page.locator(`[data-au05-palette][data-value="${value}"]`);
  const choose=async(key,value)=>{await pick(key);await card(value).click();};
  await page.waitForSelector('.au05-diagram');
  for(const id of ['knobTurn','knobPress','k1','k2','k3'])
    assert.equal(await page.locator(`.au05-diagram .au05-zone[data-au05-zone="${id}"]`).count(),1,id);
  assert.match(await page.locator('.au05-callout[data-au05-zone="k3"]').textContent(),/Nothing/);
  assert.match(await page.locator('.au05-callout[data-au05-zone="knobTurn"]').textContent(),/Factory scroll/);
  assert.match(await page.locator('#au05-battery').textContent(),/30 %/);
  await pick('k3');
  await page.waitForSelector('[data-au05-mode="k3"]');
  assert.equal(await mode('k3').getAttribute('data-value'),'uc:none','K3 types its own code: a UC action (Nothing)');
  assert.equal(await card('uc:none').getAttribute('aria-pressed'),'true','the selected part\'s action is marked in the palette');
  // Grouped catalogue: one card section per app, unverified marked with a small badge.
  assert.deepEqual(await page.locator('.au05-palette .au05-sec-title').allTextContents(),['Claude','ChatGPT','Windows & media','Unified Control','Knob turn']);
  assert.equal(await card('uc:chatgptNewChat').locator('.au05-unv').count(),1);
  assert.equal(await card('uc:claudeDictation').locator('.au05-unv').count(),0);
  assert.equal(await card('knob:scroll').getAttribute('aria-disabled'),'true','knob turn cards do not apply to a key');
  // A new key message lights up its zone.
  await page.evaluate(()=>window.__au05.events.unshift({at:Date.now()+1,control:'K1',pressed:true,code:'F20',action:null}));
  await page.waitForSelector('.au05-diagram .au05-flash[data-au05-zone="k1"]',{timeout:5000});
  // Keyboard: Enter on a zone selects it.
  await page.locator('.au05-zone[data-au05-zone="knobPress"]').focus();await page.keyboard.press('Enter');
  await page.waitForSelector('[data-au05-mode="knobPress"]');
  assert.match(await page.locator('#au05-events').textContent(),/knob right down/);
  // Every setting can be changed on a real key; unchecked ones are labelled Untested.
  for(const name of ['sleepTime','lightMode','lightBrightness','micNr','hooksMode','standbyTime'])
    assert.equal(await page.locator(`select[data-au05-set="${name}"]`).isDisabled(),false,name);
  assert.equal(await page.locator('[data-au05-mode="knobPress"] [data-au05-segment="uc"]').isDisabled(),false);
  assert.ok(await page.locator('#au05-controls .chip', {hasText:'Untested'}).count()>=6);
  await page.locator('select[data-au05-record="1"][data-au05-field="time"]').selectOption('12');
  await page.waitForFunction(()=>window.__writes.some(w=>w.change?.setting==='lightRecord'&&w.change.record===1&&w.change.field==='time'&&w.change.value===12));
  await page.getByRole('button',{name:'Read all settings'}).click();
  await page.waitForFunction(()=>window.__writes.some(w=>w.command==='au05_read_all'));

  // K1 is not read yet: it offers "Read from key".
  await pick('k1');
  await page.locator('[data-au05-read="k1"]').click();
  await page.waitForFunction(()=>window.__writes.some(w=>w.command==='au05_read_key'&&w.key==='k1'));
  await page.waitForFunction(()=>/On the key now: Win \+ H \(factory\)/.test(document.querySelector('.au05-editor')?.textContent||''));
  assert.match(await page.locator('[data-au05-read="k1"]').textContent(),/Read again/);
  // K3: a UC action. Choosing it saves the action; the hotkey shows as ready and the label updates.
  await pick('k3');
  await choose('k3','uc:claudeDictation');
  await page.waitForFunction(()=>window.__writes.some(w=>w.patch?.action?.kind==='claudeDictation'));
  await page.waitForFunction(()=>/Ready · F22/.test(document.querySelector('.au05-editor')?.textContent||''));
  await page.waitForFunction(()=>/Claude Start dictation/.test(document.querySelector('.au05-callout[data-au05-zone="k3"]')?.textContent||''));
  // K2: switch to a UC action (the key is set to type F21), then a custom key combination.
  await pick('k2');
  await choose('k2','uc:hotkey');
  // Choosing only saves the action; the app's layer sync sets the key (never the page).
  await page.waitForFunction(()=>window.__writes.some(w=>w.patch?.action?.control==='k2'&&w.patch.action.kind==='hotkey'));
  const combo=page.locator('input[data-au05-action-value="k2"]');await combo.waitFor();
  await combo.fill('Ctrl + Enter');await combo.press('Tab');
  await page.waitForFunction(()=>window.__writes.some(w=>w.patch?.action?.value==='Ctrl + Enter'));
  // The offline layer ("On its own"): K1 types Ctrl + D without Unified Control. The palette is not offered there.
  await page.locator('[data-au05-layer-view="offline"]').click();
  await pick('k1');
  assert.equal(await page.locator('.au05-palette').count(),0);
  await page.locator('[data-au05-offline-mode="keys"][data-control="k1"]').click();
  const keysInput=page.locator('input[data-au05-offline-keys="k1"]');
  assert.equal(await keysInput.isVisible(),true);
  await keysInput.fill('Ctrl + D');await keysInput.press('Tab');
  await page.waitForFunction(()=>window.__writes.some(w=>w.patch?.offline?.control==='k1'&&w.patch.offline.keys==='Ctrl + D'));
  await page.waitForFunction(()=>/Types Ctrl \+ D/.test(document.querySelector('.au05-callout[data-au05-zone="k1"]')?.textContent||''));
  // Only actions the key can store are offered; UC-only ones are greyed with "Needs Unified Control".
  assert.equal(await page.locator('[data-au05-offline-pick][data-control="k1"]').count(),2,'Ctrl + D and Ctrl + Shift + O');
  assert.match(await page.locator('.au05-needs-uc-list').textContent(),/Dictate & send[\s\S]*Needs Unified Control/);
  assert.equal(await page.locator('.au05-needs-uc-list [data-au05-offline-pick]').count(),0,'nothing UC-only can be picked');
  await page.locator('[data-au05-offline-pick="Ctrl + Shift + O"][data-control="k1"]').click();
  await page.waitForFunction(()=>window.__writes.some(w=>w.patch?.offline?.control==='k1'&&w.patch.offline.keys==='Ctrl + Shift + O'));
  // Store the offline layer now, and back.
  await page.locator('[data-au05-layer="offline"]').click();
  await page.waitForFunction(()=>window.__writes.some(w=>w.patch?.layer==='offline'));
  await page.locator('[data-au05-layer="live"]').click();
  await page.locator('[data-au05-layer-view="live"]').click();
  // Live layer: a plain-keystroke action offers "Also use on its own" (K1 has Start dictation, Ctrl + D).
  await pick('k1');
  await page.locator('[data-au05-palette][data-value="uc:claudeDictation"]').click();
  await page.waitForFunction(()=>window.__writes.some(w=>w.patch?.action?.control==='k1'&&w.patch.action.kind==='claudeDictation'));
  await page.locator('[data-au05-also="Ctrl + D"][data-control="k1"]').click();
  await page.waitForFunction(()=>window.__writes.filter(w=>w.patch?.offline?.control==='k1'&&w.patch.offline.keys==='Ctrl + D').length>=2);
  await page.waitForFunction(()=>/On its own it types Ctrl \+ D too/.test(document.querySelector('.au05-editor')?.textContent||''));
  // K3 back to "Same as on its own" (no UC action; its hotkey is released).
  await pick('k3');
  await segment('k3','offline');
  await page.waitForFunction(()=>window.__writes.some(w=>w.patch?.action?.control==='k3'&&w.patch.action.kind==='offline'));
  await page.locator('select[data-au05-set="standbyTime"]').selectOption('900');
  await page.waitForFunction(()=>window.__writes.some(w=>w.change?.setting==='standbyTime'));
  const changes=(await writes()).filter(w=>w.command==='au05_change').map(w=>w.change);
  assert.deepEqual(changes,[{setting:'lightRecord',record:1,field:'time',value:12},{setting:'standbyTime',value:900}],'the page writes device settings only; key typing comes from the layer sync');
  // A control not checked on the real key waits for "Set up"; a failed key change pauses key changes.
  await page.evaluate(()=>{window.__au05.keys.find(k=>k.key==='knobPress').job={state:'check',message:'Not checked on the real key yet: press Set up',label:'F19',read:false};window.__au05.keySync={blocked:true,pending:false,waiting:false};});
  await pick('knobPress');
  await page.waitForSelector('[data-au05-retry="knobPress"]',{timeout:5000});
  assert.match(await page.locator('.au05-layerbar').textContent(),/Key changes paused: use Reconnect/);
  assert.match(await page.locator('[data-au05-retry="knobPress"]').textContent(),/Set up/);
  assert.equal(await page.locator('.au05-callout[data-au05-zone="knobPress"] .au05-needs-check').count(),1,'the drawing marks the knob press');
  await page.locator('[data-au05-retry="knobPress"]').click();
  await page.waitForFunction(()=>window.__writes.some(w=>w.change?.setting==='retryJob'&&w.change.key==='knobPress'));
  // Drag and drop: a card dropped on K2 gives K2 that action; Undo restores the previous one (Ctrl + Enter).
  await pick('k1');
  await card('uc:volumeUp').dragTo(page.locator('.au05-diagram .au05-zone[data-au05-zone="k2"]'));
  await page.waitForFunction(()=>window.__writes.some(w=>w.patch?.action?.control==='k2'&&w.patch.action.kind==='volumeUp'));
  await page.waitForFunction(()=>/K2 · confirm: Volume up/.test(document.querySelector('#au05-toast:not([hidden])')?.textContent||''));
  assert.match(await page.locator('.au05-callout[data-au05-zone="k2"]').textContent(),/Volume up/);
  await page.locator('[data-au05-undo]').click();
  await page.waitForFunction(()=>{const w=window.__writes.filter(w=>w.patch?.action?.control==='k2');const last=w[w.length-1];return last.patch.action.kind==='hotkey'&&last.patch.action.value==='Ctrl + Enter';});
  // A knob turn card is refused by a key and accepted by the knob ring.
  const before=(await writes()).length;
  await card('knob:volume').dragTo(page.locator('.au05-diagram .au05-zone[data-au05-zone="k1"]'));
  await page.waitForTimeout(300);
  assert.equal((await writes()).length,before,'nothing is written for a drop a key does not accept');
  const ring=page.locator('.au05-diagram .au05-zone[data-au05-zone="knobTurn"]');const rb=await ring.boundingBox();
  await card('knob:volume').dragTo(ring,{targetPosition:{x:4,y:rb.height/2}});
  await page.waitForFunction(()=>window.__writes.some(w=>w.patch?.knobTurn==='uc'&&w.patch.knobTurnAction==='volume'));
  await page.getByRole('button',{name:'Reconnect'}).click();
  await page.waitForFunction(()=>window.__writes.some(w=>w.command==='au05_reconnect'));

  // The knob press menu role is shown (Select after the 0.31.0 migration) and can be changed.
  await page.evaluate(()=>{window.__au05.menu={controls:{k2:'select',k3:'back',knobPress:'select'},sessions:{source:'desktop',open:3,groups:2,groupsNote:null}};});
  await page.getByRole('button',{name:'Reconnect'}).click();
  const press=page.locator('select[data-qm-role="knobPress"]');
  await press.waitFor();
  assert.equal(await press.inputValue(),'select','the knob press selects by default');
  assert.match(await page.locator('.au05-claude').textContent(),/grouped by your Claude sidebar projects/);
  await press.selectOption('close');
  await page.waitForFunction(()=>window.__writes.some(w=>w.patch?.menuControls?.knobPress==='close'));
  await page.waitForFunction(()=>document.querySelector('select[data-qm-role="knobPress"]')?.value==='close');
  // "Reopen where I left off": on by default, can be turned off.
  const reopen=page.locator('input[data-qm-reopen]');
  assert.equal(await reopen.isChecked(),true,'reopen where I left off is on by default');
  await reopen.uncheck();
  await page.waitForFunction(()=>window.__writes.some(w=>w.patch?.reopenWhereLeft===false));
  await page.evaluate(()=>{window.__au05.menu.sessions={source:'desktop',open:3,groups:null,groupsNote:"Claude's sidebar groups were not found"};});
  await page.getByRole('button',{name:'Reconnect'}).click();
  await page.waitForFunction(()=>/groups the sessions by folder/.test(document.querySelector('.au05-claude')?.textContent||''));

  const output=path.join(os.tmpdir(),'uc-vibe-key-preview');fs.mkdirSync(output,{recursive:true});
  await pick('knobTurn');
  assert.match(await page.locator('.au05-editor').textContent(),/Same as on its own/,'the knob turn without UC codes types its offline layer');
  await pick('k3');
  await choose('k3','uc:volumeUp');
  await page.waitForFunction(()=>/Volume up/.test(document.querySelector('.au05-callout[data-au05-zone="k3"]')?.textContent||''));
  for(const [width,height]of[[1440,1000],[800,700],[600,700]]){
    await page.setViewportSize({width,height});
    assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),`no overflow at ${width}`);
    await page.screenshot({path:path.join(output,`uc-vibe-key-${width}.png`),fullPage:true});
  }
  await page.setViewportSize({width:1440,height:1000});
  await page.locator('.au05-records').screenshot({path:path.join(output,'uc-vibe-key-light-records.png')});
  await page.locator('.au05-workspace').screenshot({path:path.join(output,'uc-vibe-key-diagram.png')});
  await nav('general').click();await page.locator('#au05-enabled').uncheck();
  await page.waitForFunction(()=>document.querySelector('[data-page="vibekey"]').hidden);
  assert.deepEqual(errors,[]);
  console.log(`Vibe Key interface checks passed. Screenshots: ${output}`);
}finally{await browser?.close();server.close();}})().catch(error=>{console.error(error);process.exitCode=1;});
