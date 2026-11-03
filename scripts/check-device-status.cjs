// Pure summaries + headless fixture IPC. Never opens an app, device or account.
const fs=require('node:fs'),path=require('node:path'),http=require('node:http'),os=require('node:os'),assert=require('node:assert/strict');
const {chromium}=require(process.env.UC_PLAYWRIGHT_MODULE||'playwright');
const root=path.resolve(__dirname,'../public');
async function main(){
  const source=fs.readFileSync(path.join(root,'js/cooler-panel.js'),'utf8');
  const {coolerSummary}=await import('data:text/javascript;base64,'+Buffer.from(source).toString('base64'));
  const now=Date.UTC(2026,10,2,4),iso=new Date(now).toISOString();
  const sample={enabled:true,state:'running',diagnosticsEnabled:true,health:{state:'failed',code:'descriptor_failure',lastSampleAt:iso,pnpSampledAt:iso,pnp:{available:true,normalNodes:0,exactMatchedDescriptorCount:1}}};
  const summary=s=>coolerSummary(s,{now});
  assert.match(summary(sample).description,/Last connection check.*enumeration failure/);
  assert.equal(summary(sample).status,'Engine running');
  assert.equal(summary({...sample,state:'starting'}).status,'Engine starting');
  assert.equal(summary({...sample,state:'blocked'}).status,'Engine blocked');
  assert.equal(summary({...sample,enabled:false}).status,'Disabled');
  assert.match(summary({...sample,diagnosticsEnabled:false}).description,/diagnostics are off.*unknown/);
  assert.match(summary({...sample,health:null}).description,/Waiting.*unknown/);
  for(const timestamp of [null,'2026-02-30T00:00:00.000Z',new Date(now-60_001).toISOString(),new Date(now+1001).toISOString()])
    assert.match(summary({...sample,health:{...sample.health,lastSampleAt:timestamp}}).description,/out of date.*unknown/);
  assert.match(summary({...sample,health:{...sample.health,pnpSampledAt:new Date(now-90_001).toISOString()}}).description,/USB information.*out of date/);
  for(const code of ['__proto__','constructor','<img src=x onerror=alert(1)>'])
    assert.match(summary({...sample,health:{...sample.health,code}}).description,/unavailable.*unknown/);
  assert.match(coolerSummary(sample,{readError:true}).description,/could not be refreshed.*unknown/);
  const server=http.createServer((req,res)=>{
    const file=path.resolve(root,'.'+new URL(req.url,'http://localhost').pathname.replace(/\/$/,'/index.html'));
    if(!file.startsWith(root+path.sep)){res.writeHead(403);return res.end();}
    fs.readFile(file,(error,data)=>{res.writeHead(error?404:200,{'Content-Type':file.endsWith('.js')?'text/javascript':file.endsWith('.css')?'text/css':'text/html'});res.end(error?'Not found':data);});
  });
  let browser;
  try{
    await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
    browser=await chromium.launch({headless:true});
    const page=await browser.newPage({viewport:{width:1440,height:1000}}),errors=[];
    page.on('pageerror',error=>errors.push(error.message));
    await page.route('**/cooler/index.html',route=>route.fulfill({contentType:'text/html',body:'<!doctype html><title>Isolated cooler frame</title>'}));
    await page.addInitScript(()=>{
      const keys=Array.from({length:14},()=>({a:{kind:'text',text:''},b:{kind:'text',text:''},action:'none',value:''}));
      window.__dock={enabled:true,state:'connected',message:'Fixture dock connected'};
      window.__cooler={enabled:false,state:'disabled',diagnosticsEnabled:true};
      window.__failDock=false;window.__failCooler=false;window.__calls=[];
      const layout={version:1,currentProfile:'p',currentPage:'pg',profiles:[{id:'p',name:'Fixture',pages:[{id:'pg',name:'Main',keys}]}]};
      window.__TAURI_INTERNALS__={transformCallback:()=>1,invoke:async(command,args)=>{
        window.__calls.push(command);
        if(command==='app_state')return {appVersion:'fixture',page:'home',fake:true,dock:structuredClone(window.__dock),cooler:structuredClone(window.__cooler)};
        if(command==='profiles_state')return {state:layout,revision:'fixture'};
        if(command==='dock_state'){if(window.__failDock)throw Error('private-error');return structuredClone(window.__dock);}
        if(command==='cooler_state'){if(window.__failCooler)throw Error('private-error');return structuredClone(window.__cooler);}
        if(command==='plugin:event|listen')return 1;
        return {};
      }};
    });
    await page.goto(`http://127.0.0.1:${server.address().port}/`);
    const card=id=>page.locator(`.device-card[data-go="${id}"]`);
    await page.waitForFunction(()=>document.querySelector('.device-card[data-go="dock"] [data-device-status]')?.textContent==='connected');
    await card('dock').focus();
    await page.evaluate(()=>{window.__homeDock=document.querySelector('.device-card[data-go="dock"]');window.__dock={enabled:true,state:'error',message:'Dock disconnected <img id="unsafe" src=x>'};window.__cooler={enabled:true,state:'running',diagnosticsEnabled:true,health:{state:'failed',code:'descriptor_failure',lastSampleAt:new Date().toISOString(),pnpSampledAt:new Date().toISOString(),pnp:{available:true,normalNodes:0,exactMatchedDescriptorCount:1}}};});
    await page.waitForFunction(()=>document.querySelector('.device-card[data-go="dock"] [data-device-status]')?.textContent==='error'&&document.querySelector('.device-card[data-go="cooler"]')?.textContent.includes('enumeration failure'));
    assert.equal(await card('dock').evaluate(el=>el===window.__homeDock&&el===document.activeElement),true,'status changes retain node and focus');
    assert.equal(await page.locator('#unsafe').count(),0,'status text cannot become markup');
    await page.evaluate(()=>window.__dock.message='Dock connection lost.');
    await page.waitForFunction(()=>document.querySelector('.device-card[data-go="dock"] [data-device-description]')?.textContent==='Dock connection lost.');
    const output=path.join(os.tmpdir(),'uc-device-health-preview');fs.mkdirSync(output,{recursive:true});
    for(const [width,height] of [[1440,1000],[1100,800],[800,650]]){
      await page.setViewportSize({width,height});
      assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),'Home status fits supported width');
      await page.screenshot({path:path.join(output,`uc-home-health-${width}.png`),fullPage:true});
    }
    await page.setViewportSize({width:1440,height:1000});
    await page.evaluate(()=>{window.__failDock=true;window.__failCooler=true;});
    await page.waitForFunction(()=>[...document.querySelectorAll('.device-card[data-go="dock"] [data-device-status],.device-card[data-go="cooler"] [data-device-status]')].every(el=>el.textContent==='Status unavailable'));
    assert.ok(!(await card('cooler').textContent()).includes('private-error'));
    await page.evaluate(()=>{window.__failDock=false;window.__failCooler=false;window.__cooler.state='blocked';});
    await page.waitForFunction(()=>document.querySelector('.device-card[data-go="cooler"] [data-device-status]')?.textContent==='Engine blocked');
    assert.equal(await page.evaluate(()=>window.__calls.some(c=>['cooler_configure','settings_update','cooler_call'].includes(c))),false,'Home polling does not change configuration or call the worker');
    await page.locator('[data-page="general"]').click();
    assert.equal(await page.locator('#cooler-state').textContent(),'Engine blocked');
    const settingsWrites=await page.evaluate(()=>window.__calls.filter(c=>c==='settings_update').length);
    await page.evaluate(()=>{window.__cooler.state='running';window.__cooler.diagnosticsEnabled=false;});
    await page.waitForFunction(()=>document.getElementById('cooler-message')?.textContent.includes('diagnostics are off'));
    await page.evaluate(()=>{window.__cooler.diagnosticsEnabled=true;window.__cooler.health={state:'connected',code:'connected',lastSampleAt:new Date(Date.now()-61_000).toISOString()};});
    await page.waitForFunction(()=>document.getElementById('cooler-message')?.textContent.includes('out of date'));
    await page.evaluate(()=>window.__cooler.health.lastSampleAt=new Date().toISOString());
    await page.waitForFunction(()=>document.getElementById('cooler-message')?.textContent.includes('Last connection check: Live reported'));
    assert.equal(await page.locator('#cooler-state').textContent(),'Engine running');
    assert.equal(await page.evaluate(()=>window.__calls.some(c=>['cooler_configure','cooler_call'].includes(c))),false,'status checks do not configure or call the worker');
    assert.equal(await page.evaluate(()=>window.__calls.filter(c=>c==='settings_update').length),settingsWrites,'status polling adds no settings writes after intentional page navigation');
    assert.deepEqual(errors,[]);
    console.log('PASS: finite cached-health summaries; separate PnP freshness; current worker/disabled/blocked/off/unknown precedence; live Home polls, read failure/recovery, focus/node retention and escaped status; no configuration/device/account effects.');
    console.log(output);
  }finally{if(browser)await browser.close();await new Promise(resolve=>server.close(resolve));}
}
main().catch(error=>{console.error(error);process.exitCode=1;});
