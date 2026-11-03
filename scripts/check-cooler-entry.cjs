// Actual embedded UI in a local headless fixture: no native app/devices/accounts.
const {chromium}=require(process.env.UC_PLAYWRIGHT_MODULE||'playwright');
const fs=require('node:fs'),path=require('node:path'),http=require('node:http'),assert=require('node:assert/strict');
const root=path.resolve(__dirname,'../public');
const server=http.createServer((req,res)=>{
  const file=path.resolve(root,'.'+new URL(req.url,'http://localhost').pathname);
  if(!file.startsWith(root+path.sep)){res.writeHead(403);return res.end();}
  fs.readFile(file,(error,data)=>{res.writeHead(error?404:200,{'Content-Type':file.endsWith('.js')?'text/javascript':file.endsWith('.css')?'text/css':'text/html'});res.end(error?'Not found':data);});
});
(async()=>{
  await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));let browser;
  try{
    browser=await chromium.launch({headless:true});
    const page=await browser.newPage({viewport:{width:1440,height:1000}});
    await page.route('**/entry-fixture.html',route=>route.fulfill({contentType:'text/html',body:'<!doctype html><html><body></body></html>'}));
    await page.goto(`http://127.0.0.1:${server.address().port}/entry-fixture.html`);
    await page.evaluate(()=>{
      window.__calls=[];window.__appReads=0;
      window.addEventListener('message',event=>{
        const message=event.data;if(event.origin!==location.origin||event.source!==document.querySelector('iframe')?.contentWindow||message?.channel!=='uc-cooler-v1'||message.type!=='call')return;
        window.__calls.push(message.command);
        let result={};
        if(message.command==='app_state'){
          window.__appReads++;
          const live=window.__appReads===1?window.__firstLive:window.__freshLive;
          result={appVersion:'fixture',fake:true,mask:{mode:'none',band:{},circle:{}},screenFit:{},screenGuides:{},weather:{},galleryDir:'Fixture',live,mediaBase:''};
        }else if(message.command==='originals_list')result={originals:[],longMissing:[],rev:1};
        else if(message.command==='sources_list')result={sources:[],ticked:[]};
        else if(message.command==='activity')result={jobs:[],rev:1,deviceBusy:false};
        else if(message.command==='device_check')result={_needsSetup:false};
        event.source.postMessage({channel:'uc-cooler-v1',type:'result',id:message.id,result},location.origin);
      });
      window.__open=async(first,fresh)=>{
        document.querySelector('iframe')?.remove();window.__calls=[];window.__appReads=0;
        window.__firstLive=first;window.__freshLive=fresh;
        const frame=document.createElement('iframe');frame.src='/cooler/index.html';frame.style.cssText='width:1400px;height:900px;border:0';document.body.append(frame);
      };
    });
    const open=async(first,fresh=first)=>{
      await page.evaluate(({first,fresh})=>window.__open(first,fresh),{first,fresh});
      const frame=page.frameLocator('iframe');await frame.locator('#navScreen').waitFor();
      await frame.locator('body').evaluate(async()=>{await(await import('./app.js')).initialized;});
      return frame;
    };
    const live=(inApp,running,extra={})=>({settings:{inApp},status:{running,...extra}});
    for(let visit=0;visit<3;visit++){
      await open(live(true,true,{connected:true}));
      assert.equal(await page.evaluate(()=>window.__calls.filter(command=>command==='device_check').length),0,'healthy Live entry never performs a firmware check');
    }
    for(const state of [live(true,true,{connected:false}),live(true,false),live(true,true,{paused:true}),live(false,true,{previewing:true}),live(false,true,{tuning:true})]){
      await open(state);assert.equal(await page.evaluate(()=>window.__calls.includes('device_check')),false,'Live startup/retry/holds and physical previews own their connection');
    }
    await open(live(false,false),live(true,true,{connected:true}));
    assert.equal(await page.evaluate(()=>window.__calls.includes('device_check')),false,'entry rechecks current worker state after hydration');
    assert.ok(await page.evaluate(()=>window.__appReads>=2),'entry policy uses a new worker app_state response');
    await open(live(true,true),live(false,false));
    assert.equal(await page.evaluate(()=>window.__calls.filter(command=>command==='device_check').length),1,'an actually-off screen still gets its initial check');
    await open(live(false,true,{twinning:true}));
    assert.equal(await page.evaluate(()=>window.__calls.filter(command=>command==='device_check').length),1,'an editor-only twin is not physical Live');
    await open({settings:{},status:{}});
    assert.equal(await page.evaluate(()=>window.__calls.includes('device_check')),false,'unknown state cannot trigger an automatic hardware check');
    const frame=await open(live(true,true,{connected:true}));
    await frame.locator('#pill').click();await frame.locator('#scrCheck').click();
    await page.waitForFunction(()=>window.__calls.includes('device_check'));
    assert.equal(await page.evaluate(()=>window.__calls.filter(command=>command==='device_check').length),1,'explicit Check remains available while Live is on');
    console.log('PASS: repeated embedded UI entry leaves Live connection alone; fresh worker startup/retry/hold/preview policy; Live-off and editor-only-twin checks; unknown-state refusal; explicit Check preserved. Headless fixtures only.');
  }finally{await browser?.close();await new Promise(resolve=>server.close(resolve));}
})().catch(error=>{console.error(error);process.exitCode=1;});
