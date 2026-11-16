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
    // A real key (fake:false): only the checked settings may be sent.
    window.__au05={enabled:false,fake:false,state:'disabled',message:'Vibe Key is disabled',battery:null,standbyS:null,sleepS:null,light:null,micNr:null,
      keys:[],events:[],actions:{k1:none,k2:none,k3:none,knobPress:none,knobRight:none,knobLeft:none},
      realKeyChecked:{standbyTime:true,keyTyping:true,sleepTime:false,lightMode:false,lightBrightness:false,micNr:false},
      lightRange:{modeMax:3,brightnessMax:10},micNrMax:3};
    const connected=()=>Object.assign(window.__au05,{state:'connected',message:'Connected',battery:{millivolts:3608,percent:30},standbyS:600,sleepS:3600,
      light:{mode:2,brightness:7},micNr:{level:0},
      keys:[{key:'k1',name:'K1',mode:'factory',label:'Factory default (Win + H)',keys:null,factory:'Win + H',harmless:'F20'},
            {key:'k2',name:'K2',mode:'factory',label:'Factory default (Enter)',keys:null,factory:'Enter',harmless:'F21'},
            {key:'k3',name:'K3',mode:'harmless',label:'F22',keys:'F22',factory:'Esc',harmless:'F22'}],
      events:[{at:Date.now(),control:'K3',pressed:true,code:'F22',action:null}]});
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
        return structuredClone(window.__au05);
      }
      if(command==='au05_change'){
        window.__writes.push({command,...args});
        const c=args.change;
        if(c.setting==='standbyTime')window.__au05.standbyS=c.value;
        if(c.setting==='keyTyping'){const k=window.__au05.keys.find(k=>k.key===c.key);k.mode=c.mode;k.label=c.mode==='keys'?c.keys:c.mode==='harmless'?k.harmless:`Factory default (${k.factory})`;k.keys=c.mode==='keys'?c.keys:null;}
        return structuredClone(window.__au05);
      }
      if(command==='au05_reconnect'){window.__writes.push({command});return structuredClone(window.__au05);}
      if(command==='settings_update')return {appVersion:'fixture',page:args.patch.page||'home',fake:true};
      return {};
    }};
  });
  await page.goto(`http://127.0.0.1:${server.address().port}/`);
  await page.waitForFunction(()=>document.getElementById('storage-status')?.textContent==='Changes saved in Unified Control');
  const nav=id=>page.locator(`.nav-item[data-page="${id}"]`);
  assert.equal(await nav('vibekey').isHidden(),true,'the Vibe Key is hidden until enabled');
  assert.deepEqual(await page.evaluate(()=>window.__writes),[],'presentation enables nothing');

  await nav('general').click();
  assert.equal(await page.locator('#au05-enabled').isChecked(),false,'off by default');
  await page.locator('#au05-enabled').check();
  await page.waitForFunction(()=>!document.querySelector('[data-page="vibekey"]').hidden);
  assert.deepEqual(await page.evaluate(()=>window.__writes[0]),{command:'au05_configure',patch:{enabled:true}});
  await nav('home').click();
  assert.match(await page.locator('.device-card[data-go="vibekey"]').textContent(),/battery 30 %/);

  await nav('vibekey').click();
  await page.waitForSelector('#au05-controls select[data-au05-type="k3"]');
  assert.match(await page.locator('#au05-battery').textContent(),/30 %/);
  assert.equal(await page.locator('select[data-au05-type="k3"]').inputValue(),'harmless');
  assert.match(await page.locator('#au05-events').textContent(),/K3 down/);
  // Unchecked settings are shown but cannot be sent to a real key.
  for(const name of ['sleepTime','lightMode','lightBrightness','micNr'])
    assert.equal(await page.locator(`select[data-au05-set="${name}"]`).isDisabled(),true,name);
  assert.equal(await page.locator('select[data-au05-set="standbyTime"]').isDisabled(),false);

  await page.locator('select[data-au05-set="standbyTime"]').selectOption('900');
  await page.waitForFunction(()=>window.__writes.some(w=>w.command==='au05_change'&&w.change.setting==='standbyTime'));
  await page.locator('select[data-au05-type="k1"]').selectOption('keys');
  const keysInput=page.locator('input[data-au05-keys="k1"]');
  assert.equal(await keysInput.isVisible(),true);
  await keysInput.fill('Ctrl + D');await keysInput.press('Tab');
  await page.waitForFunction(()=>window.__writes.some(w=>w.command==='au05_change'&&w.change.keys==='Ctrl + D'));
  await page.locator('select[data-au05-action="k3"]').selectOption('hotkey');
  await page.waitForFunction(()=>window.__writes.some(w=>w.patch?.action?.control==='k3'));
  const value=page.locator('input[data-au05-action-value="k3"]');
  await value.fill('Ctrl + Enter');await value.press('Tab');
  await page.waitForFunction(()=>window.__writes.some(w=>w.patch?.action?.value==='Ctrl + Enter'));
  const changes=await page.evaluate(()=>window.__writes.filter(w=>w.command==='au05_change').map(w=>w.change));
  assert.deepEqual(changes,[{setting:'standbyTime',value:900},{setting:'keyTyping',key:'k1',mode:'keys',keys:'Ctrl + D'}]);
  assert.match(await page.locator('#au05-note').textContent(),/Saved/);
  await page.getByRole('button',{name:'Reconnect'}).click();
  await page.waitForFunction(()=>window.__writes.some(w=>w.command==='au05_reconnect'));

  const output=path.join(os.tmpdir(),'uc-vibe-key-preview');fs.mkdirSync(output,{recursive:true});
  for(const [width,height]of[[1440,1000],[800,700],[600,700]]){
    await page.setViewportSize({width,height});
    assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),`no overflow at ${width}`);
    await page.screenshot({path:path.join(output,`uc-vibe-key-${width}.png`),fullPage:true});
  }
  await page.setViewportSize({width:1440,height:1000});
  await nav('general').click();await page.locator('#au05-enabled').uncheck();
  await page.waitForFunction(()=>document.querySelector('[data-page="vibekey"]').hidden);
  assert.deepEqual(errors,[]);
  console.log(`Vibe Key interface checks passed. Screenshots: ${output}`);
}finally{await browser?.close();server.close();}})().catch(error=>{console.error(error);process.exitCode=1;});
