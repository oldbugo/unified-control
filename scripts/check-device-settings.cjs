// Headless fixture only: device visibility/settings, never real hardware or accounts.
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
    window.__dock={enabled:false,state:'disabled',message:'Dock disabled',fake:true};
    window.__cooler={enabled:false,state:'disabled',diagnosticsEnabled:true};
    window.__failDock=false;window.__failCooler=false;window.__deviceWrites=[];
    const app=()=>({appVersion:'fixture',page:window.__page||'home',fake:true,dock:structuredClone(window.__dock),cooler:structuredClone(window.__cooler)});
    window.__TAURI_INTERNALS__={transformCallback:()=>1,invoke:async(command,args)=>{
      if(command==='app_state')return app();
      if(command==='profiles_state')return {state:layout,revision:'fixture'};
      if(command==='dock_state')return structuredClone(window.__dock);
      if(command==='cooler_state')return structuredClone(window.__cooler);
      if(command==='dock_configure'){
        window.__deviceWrites.push({device:'dock',enabled:args.patch.enabled});
        if(window.__failDock)throw Error('fixture failure');
        window.__dock.enabled=args.patch.enabled;window.__dock.state=args.patch.enabled?'connected':'disabled';window.__dock.message=args.patch.enabled?'Fixture dock connected':'Dock disabled';window.__dock.usageStatus={reader:{code:'ready'},providers:[{provider:'codex',selected:args.patch.enabled,status:'ok',message:'Verified fixture allowance.'}]};return structuredClone(window.__dock);
      }
      if(command==='cooler_configure'){
        window.__deviceWrites.push({device:'cooler',enabled:args.enabled});
        if(window.__failCooler)throw Error('fixture failure');
        window.__cooler.enabled=args.enabled;window.__cooler.state=args.enabled?'running':'disabled';return structuredClone(window.__cooler);
      }
      if(command==='settings_update'){window.__page=args.patch.page||window.__page;return app();}
      return {};
    }};
  });
  await page.goto(`http://127.0.0.1:${server.address().port}/`);
  await page.waitForFunction(()=>document.getElementById('storage-status')?.textContent==='Changes saved in Unified Control');
  const nav=id=>page.locator(`.nav-item[data-page="${id}"]`);
  const visible=()=>page.locator('.nav-item:not([hidden])');
  assert.equal(await visible().count(),4,'disabled/unimplemented devices are absent (Home, General, About, Input tester)');
  assert.equal(await page.locator('.device-card').count(),0);
  assert.equal(await page.locator('#devices-empty').isVisible(),true);
  assert.deepEqual(await page.evaluate(()=>window.__deviceWrites),[],'initial presentation enables nothing');
  const search=page.getByRole('searchbox',{name:'Find a page'});
  await search.fill('usage');assert.equal(await visible().count(),0,'search cannot reveal a disabled device');
  await search.press('Escape');await nav('general').click();
  await page.locator('#dock-enabled').check();
  await page.waitForFunction(()=>!document.querySelector('[data-page="dock"]').hidden);
  assert.equal(await nav('general').getAttribute('aria-current'),'page','enable keeps Settings open');
  await page.locator('#cooler-enabled').check();
  await page.waitForFunction(()=>!document.querySelector('[data-page="cooler"]').hidden);
  assert.equal(await visible().count(),6);
  assert.equal(await page.locator('.device-advanced #dock-usage-status').count(),0,'dock usage must not appear in cooler diagnostics');
  assert.equal(await page.locator('#dock-usage-status').count(),0,'General does not create dock workspace usage status');
  await page.waitForTimeout(200); // Allow the switch's visual transition before screenshots.
  const output=path.join(os.tmpdir(),'uc-device-settings-preview');fs.mkdirSync(output,{recursive:true});
  for(const [width,height]of[[1440,1000],[800,700],[600,700]]){
    await page.setViewportSize({width,height});
    assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),'settings does not overflow');
    await page.screenshot({path:path.join(output,`uc-general-${width}.png`),fullPage:true});
  }
  await page.setViewportSize({width:1440,height:1000});
  await nav('home').click();assert.equal(await page.locator('.device-card').count(),2);
  await page.screenshot({path:path.join(output,'uc-home-enabled.png'),fullPage:true});
  await nav('dock').click();assert.match(await page.locator('#dock-usage-status').textContent(),/Codex: Verified fixture allowance/);assert.equal(await page.locator('#dock-enabled').count(),0,'workspace contains no enable banner/toggle');
  assert.equal(await page.locator('.device-header').count(),0);
  await nav('cooler').click();assert.equal(await page.locator('.device-header').count(),0);
  assert.equal(await page.locator('#cooler-enabled').count(),0);
  assert.equal(await page.locator('#pane').evaluate(el=>getComputedStyle(el).padding),'0px');
  await nav('general').click();await page.locator('#dock-enabled').uncheck();
  await page.waitForFunction(()=>document.querySelector('[data-page="dock"]').hidden);
  await page.locator('#cooler-enabled').uncheck();
  await page.waitForFunction(()=>document.querySelector('[data-page="cooler"]').hidden);
  await page.evaluate(()=>{window.__failDock=true;window.__failCooler=true;});
  await page.locator('#dock-enabled').click();
  await page.waitForFunction(()=>!document.getElementById('dock-enabled').checked);
  await page.locator('#cooler-enabled').click();
  await page.waitForFunction(()=>!document.getElementById('cooler-enabled').checked);
  assert.equal(await visible().count(),4,'failed enable preserves hidden disabled devices');
  await search.fill('themes');assert.equal(await visible().count(),0);
  await search.press('Escape');await nav('home').click();
  assert.equal(await page.locator('.device-card').count(),0);
  assert.equal(await page.locator('#devices-empty').isVisible(),true);
  assert.deepEqual(errors,[]);
  console.log('PASS: General device toggles; enabled-only sidebar/Home/search; disabled/failed-enable guards; no startup enable; no workspace banners; full cooler pane; three-width settings and empty workspace. Fixtures only.');
  console.log(output);
}finally{await browser?.close();await new Promise(resolve=>server.close(resolve));}})().catch(e=>{console.error(e);process.exitCode=1;});
