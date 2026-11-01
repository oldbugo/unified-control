// Headless fixture IPC only. Never invokes monitor discovery or display/audio APIs.
const {chromium}=require(process.env.UC_PLAYWRIGHT_MODULE||'playwright');
const http=require('node:http'),fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const root=path.resolve(__dirname,'../public');
const server=http.createServer((req,res)=>{
  const file=path.resolve(root,'.'+new URL(req.url,'http://localhost').pathname.replace(/\/$/,'/index.html'));
  if(!file.startsWith(root+path.sep)){res.writeHead(403);return res.end();}
  fs.readFile(file,(error,data)=>{res.writeHead(error?404:200,{'Content-Type':file.endsWith('.js')?'text/javascript':file.endsWith('.css')?'text/css':'text/html'});res.end(error?'Not found':data);});
});
(async()=>{
  let browser;
  try{
    await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
    browser=await chromium.launch({headless:true});const page=await browser.newPage();const errors=[];page.on('pageerror',error=>errors.push(error.message));
    await page.addInitScript(()=>{
      window.__monitorMode='known-error';window.__monitorCalls=0;window.__saves=0;
      const dock={enabled:false,state:'disabled',message:'Fixture dock'};
      window.__TAURI_INTERNALS__={transformCallback:()=>1,invoke:async(command,args)=>{
        if(command==='app_state'||command==='settings_update')return {page:'dock',dock,autostart:false,followPhysicalPage:true};
        if(command==='dock_state')return dock;
        if(command==='profiles_state')return {state:{version:1,currentProfile:'p',currentPage:'pg',profiles:[{id:'p',name:'Fixture',pages:[{id:'pg',name:'Main',keys:Array.from({length:14},(_,index)=>({a:{kind:'text',text:''},b:{kind:'text',text:''},action:index===0?'monitor':'none',value:''}))}]}]}};
        if(command==='key_previews')return [];
        if(command==='profiles_save'){window.__saves++;window.__saved=structuredClone(args.snapshot);return 0;}
        if(command==='monitor_list'){
          window.__monitorCalls++;
          if(window.__monitorMode==='known-error')throw {error:'Monitor Toggle resources are unavailable',status:400};
          if(window.__monitorMode==='unsafe-error')throw {error:'private account/path/device detail <img src=x onerror=alert(1)>'};
          return [{key:'fixture:display',label:'Fixture display'}];
        }
        return 0;
      }};
    });
    await page.goto(`http://127.0.0.1:${server.address().port}`);
    await page.waitForFunction(()=>document.getElementById('monitor-targets')?.textContent.includes('Monitor Toggle resources are unavailable'));
    assert.equal(await page.locator('[data-do="retry-monitors"]').count(),1);
    assert.equal(await page.evaluate(()=>window.__saves),0,'discovery failures must not save profile configuration');
    await page.evaluate(()=>window.__monitorMode='unsafe-error');await page.locator('[data-do="retry-monitors"]').click();
    await page.waitForFunction(()=>document.getElementById('monitor-targets')?.textContent.includes('Monitor information is unavailable.'));
    assert.equal(await page.locator('#monitor-targets').textContent().then(text=>text.includes('private account')),false);
    assert.equal(await page.locator('#monitor-targets img').count(),0);
    const before=await page.evaluate(()=>window.__monitorCalls);
    await page.evaluate(()=>window.__monitorMode='success');await page.locator('[data-do="retry-monitors"]').click();
    await page.waitForSelector('[data-monitor="fixture:display"]');
    assert.equal(await page.evaluate(()=>window.__monitorCalls),before+1,'Retry must make a fresh discovery request');
    await page.locator('[data-monitor="fixture:display"]').check();
    await page.waitForFunction(()=>window.__saved?.profiles[0].pages[0].keys[0].value.includes('fixture:display'));
    assert.deepEqual(errors,[]);
    console.log('PASS: known monitor discovery error surfaced; unknown details suppressed; Retry recovers; target selection saves. Fixture-only IPC, no display/audio action.');
  }finally{if(browser)await browser.close();await new Promise(resolve=>server.close(resolve));}
})().catch(error=>{console.error(error);process.exitCode=1;});
