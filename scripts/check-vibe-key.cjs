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
        if(args.patch.action)window.__au05.actions[args.patch.action.control]={kind:args.patch.action.kind,value:args.patch.action.value};
        hotkeys();return structuredClone(window.__au05);
      }
      if(command==='au05_change'){
        window.__writes.push({command,...args});
        const c=args.change;
        if(c.setting==='standbyTime')window.__au05.standbyS=c.value;
        if(c.setting==='lightRecord'){const f=['type','time','breatheLevel','breatheBrightness','alwaysOnBrightness'].indexOf(c.field);window.__au05.light.records[c.record][f]=c.value;}
        if(c.setting==='keyTyping'){const k=window.__au05.keys.find(k=>k.key===c.key);k.mode=c.mode;k.label=c.mode==='keys'?c.keys:c.mode==='uc'?k.ownCode:`Factory default (${k.factory})`;k.keys=c.mode==='keys'?c.keys:null;}
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
  await page.waitForSelector('.au05-diagram');
  for(const id of ['knobTurn','knobPress','k1','k2','k3'])
    assert.equal(await page.locator(`.au05-diagram .au05-zone[data-au05-zone="${id}"]`).count(),1,id);
  assert.match(await page.locator('.au05-callout[data-au05-zone="k3"]').textContent(),/Nothing/);
  assert.match(await page.locator('.au05-callout[data-au05-zone="knobTurn"]').textContent(),/Factory scroll/);
  assert.match(await page.locator('#au05-battery').textContent(),/30 %/);
  await pick('k3');
  await page.waitForSelector('select[data-au05-mode="k3"]');
  assert.equal(await page.locator('select[data-au05-mode="k3"]').inputValue(),'uc','K3 types its own code: a UC action');
  // A new key message lights up its zone.
  await page.evaluate(()=>window.__au05.events.unshift({at:Date.now()+1,control:'K1',pressed:true,code:'F20',action:null}));
  await page.waitForSelector('.au05-diagram .au05-flash[data-au05-zone="k1"]',{timeout:5000});
  // Keyboard: Enter on a zone selects it.
  await page.locator('.au05-zone[data-au05-zone="knobPress"]').focus();await page.keyboard.press('Enter');
  await page.waitForSelector('select[data-au05-mode="knobPress"]');
  assert.match(await page.locator('#au05-events').textContent(),/knob right down/);
  // Every setting can be changed on a real key; unchecked ones are labelled Untested.
  for(const name of ['sleepTime','lightMode','lightBrightness','micNr','hooksMode','standbyTime'])
    assert.equal(await page.locator(`select[data-au05-set="${name}"]`).isDisabled(),false,name);
  assert.equal(await page.locator('select[data-au05-mode="knobPress"]').isDisabled(),false);
  assert.ok(await page.locator('#au05-controls .chip', {hasText:'Untested'}).count()>=6);
  await page.locator('select[data-au05-record="1"][data-au05-field="time"]').selectOption('12');
  await page.waitForFunction(()=>window.__writes.some(w=>w.change?.setting==='lightRecord'&&w.change.record===1&&w.change.field==='time'&&w.change.value===12));
  await page.getByRole('button',{name:'Read all settings'}).click();
  await page.waitForFunction(()=>window.__writes.some(w=>w.command==='au05_read_all'));

  // K1 is not read yet: it offers "Read from key".
  await pick('k1');
  await page.locator('[data-au05-read="k1"]').click();
  await page.waitForFunction(()=>window.__writes.some(w=>w.command==='au05_read_key'&&w.key==='k1'));
  await page.waitForFunction(()=>document.querySelector('select[data-au05-mode="k1"]')?.value==='factory');
  assert.match(await page.locator('[data-au05-read="k1"]').textContent(),/Read again/);
  // K3: a UC action. Choosing it saves the action; the hotkey shows as ready and the label updates.
  await pick('k3');
  await page.locator('select[data-au05-action="k3"]').selectOption('claudeDictation');
  await page.waitForFunction(()=>window.__writes.some(w=>w.patch?.action?.kind==='claudeDictation'));
  await page.waitForFunction(()=>/Ready · F22/.test(document.querySelector('.au05-editor')?.textContent||''));
  await page.waitForFunction(()=>/Claude dictation/.test(document.querySelector('.au05-callout[data-au05-zone="k3"]')?.textContent||''));
  // K2: switch to a UC action (the key is set to type F21), then a custom key combination.
  await pick('k2');
  await page.locator('select[data-au05-mode="k2"]').selectOption('uc');
  await page.waitForFunction(()=>window.__writes.some(w=>w.change?.key==='k2'&&w.change.mode==='uc'));
  await page.waitForSelector('select[data-au05-action="k2"]');
  await page.locator('select[data-au05-action="k2"]').selectOption('hotkey');
  const combo=page.locator('input[data-au05-action-value="k2"]');await combo.waitFor();
  await combo.fill('Ctrl + Enter');await combo.press('Tab');
  await page.waitForFunction(()=>window.__writes.some(w=>w.patch?.action?.value==='Ctrl + Enter'));
  // K1: keys typed by the key itself.
  await pick('k1');
  await page.locator('select[data-au05-mode="k1"]').selectOption('keys');
  const keysInput=page.locator('input[data-au05-keys="k1"]');
  assert.equal(await keysInput.isVisible(),true);
  await keysInput.fill('Ctrl + D');await keysInput.press('Tab');
  await page.waitForFunction(()=>window.__writes.some(w=>w.change?.keys==='Ctrl + D'));
  // Switching K3 to factory default clears its UC action (releases the hotkey).
  await pick('k3');
  await page.locator('select[data-au05-mode="k3"]').selectOption('factory');
  await page.waitForFunction(()=>window.__writes.some(w=>w.patch?.action?.control==='k3'&&w.patch.action.kind==='none'));
  await page.locator('select[data-au05-set="standbyTime"]').selectOption('900');
  await page.waitForFunction(()=>window.__writes.some(w=>w.change?.setting==='standbyTime'));
  const changes=(await writes()).filter(w=>w.command==='au05_change').map(w=>w.change);
  assert.deepEqual(changes,[{setting:'lightRecord',record:1,field:'time',value:12},{setting:'keyTyping',key:'k2',mode:'uc'},
    {setting:'keyTyping',key:'k1',mode:'keys',keys:'Ctrl + D'},{setting:'keyTyping',key:'k3',mode:'factory'},{setting:'standbyTime',value:900}]);
  await page.getByRole('button',{name:'Reconnect'}).click();
  await page.waitForFunction(()=>window.__writes.some(w=>w.command==='au05_reconnect'));

  const output=path.join(os.tmpdir(),'uc-vibe-key-preview');fs.mkdirSync(output,{recursive:true});
  await pick('knobTurn');
  assert.match(await page.locator('.au05-editor').textContent(),/Factory scroll/);
  await pick('k3');
  await page.locator('select[data-au05-mode="k3"]').selectOption('uc');
  await page.waitForSelector('select[data-au05-action="k3"]');
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
