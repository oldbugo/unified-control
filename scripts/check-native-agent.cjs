// End-to-end MCP → request-only native proxy → same-user host pipe → editor.
// Debug-only, isolated UC_HOME, invisible window, fake devices, inactive accounts.
const {chromium}=require(process.env.UC_PLAYWRIGHT_MODULE||'playwright');
const {spawn}=require('node:child_process');
const fs=require('node:fs');
const path=require('node:path');
const os=require('node:os');
const net=require('node:net');
const assert=require('node:assert/strict');
const {pathToFileURL}=require('node:url');
const root=path.resolve(__dirname,'..');
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
async function wait(check,label,timeoutMs=15000){
  const end=Date.now()+timeoutMs;
  while(Date.now()<end){if(await check())return;await delay(50);}
  throw Error(`Native agent condition timed out: ${label}`);
}
async function run(){
  const exe=path.resolve(process.env.UC_TEST_EXE||'');
  if(!exe.toLowerCase().includes(`${path.sep}debug${path.sep}`)||path.basename(exe).toLowerCase()!=='unified-control.exe'||!fs.existsSync(exe))throw Error('UC_TEST_EXE must be the debug unified-control.exe; release/installed binaries are refused.');
  const home=fs.mkdtempSync(path.join(os.tmpdir(),'uc-native-agent-'));
  const port=await new Promise(resolve=>{const server=net.createServer();server.listen(0,'127.0.0.1',()=>{const port=server.address().port;server.close(()=>resolve(port));});});
  const env={...process.env,UC_HOME:home,UC_TEST_QUIET:'1',UC_MULTI:'1',UC_FAKE_DEVICE:'1',UC_TEST_TRAY_QUIT_MS:'90000',WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:`--remote-debugging-port=${port}`};
  const child=spawn(exe,[],{env,windowsHide:true,stdio:['ignore','pipe','pipe']});
  const exited=new Promise(resolve=>child.once('exit',(code,signal)=>resolve({code,signal})));
  let logs='';const capture=data=>{logs=(logs+data).slice(-2*1024*1024);};child.stdout.on('data',capture);child.stderr.on('data',capture);
  const clients=[];let page;
  const invoke=(command,args)=>page.evaluate(({command,args})=>window.__TAURI_INTERNALS__.invoke(command,args),{command,args});
  async function proxy(args){
    const process=spawn(exe,args,{env,windowsHide:true,stdio:['ignore','pipe','pipe']});
    let stdout='';process.stdout.on('data',data=>{stdout+=data;});process.stderr.resume();
    const code=await new Promise(resolve=>process.once('exit',resolve));return {code,stdout};
  }
  try{
    await wait(async()=>{if(child.exitCode!==null)throw Error('Quiet native app exited before WebView was ready');try{return (await fetch(`http://127.0.0.1:${port}/json/version`)).ok;}catch{return false;}},'debug WebView endpoint');
    const browser=await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
    await wait(()=>{page=browser.contexts().flatMap(context=>context.pages()).find(page=>/tauri/.test(page.url()));return !!page;},'UC WebView');
    const errors=[];page.on('pageerror',error=>errors.push(error.message));
    await wait(()=>page.evaluate(()=>document.getElementById('storage-status')?.textContent==='Changes saved in Unified Control'),'editor ready');
    const sdk=path.join(root,'companions/agent-mcp/node_modules/@modelcontextprotocol/client/dist');
    const {Client}=await import(pathToFileURL(path.join(sdk,'index.mjs')).href);
    const {StdioClientTransport}=await import(pathToFileURL(path.join(sdk,'stdio.mjs')).href);
    const entry=path.join(home,'native-mcp-entry.mjs');
    fs.writeFileSync(entry,`import{startStdio,createProcessRequester}from ${JSON.stringify(pathToFileURL(path.join(root,'companions/agent-mcp/server.mjs')).href)};startStdio({request:createProcessRequester(${JSON.stringify(exe)},['--agent-request'])});`);
    async function connect(){
      const client=new Client({name:'uc-native-fake-check',version:'1'});
      const transport=new StdioClientTransport({command:process.execPath,args:[entry],env,stderr:'pipe'});transport.stderr.resume();
      clients.push(client);await client.connect(transport);return client;
    }
    const client=await connect(),second=await connect();
    async function call(name,args={},target=client){return (await target.callTool({name:`uc_${name}`,arguments:args})).structuredContent;}
    const off=await call('get_status');assert.equal(off.ok,true);assert.equal(off.result.agentAccess,false);assert.equal(off.result.profiles,undefined);assert.equal(off.result.dock,undefined);assert.equal(off.result.cooler,undefined);
    assert.equal((await call('get_layout')).error.code,'agent-disabled');
    await page.locator('#page-search').fill('MCP');await page.locator('#page-search').press('Enter');await page.locator('#agent-access').check();
    await page.locator('#page-search').press('Escape');
    await wait(async()=>(await invoke('app_state')).agentAccess===true,'General access opt-in persisted');
    const status=await call('get_status');assert.equal(status.result.agentAccess,true);assert.equal(status.result.capabilities.physicalActions,false);
    const numerical=value=>value===null||typeof value==='number'||(value&&typeof value==='object'&&!Array.isArray(value)&&Object.values(value).every(numerical));
    assert.ok(status.result.dock.timing&&numerical(status.result.dock.timing),'MCP timing must contain numerical fields only');
    assert.equal(status.result.dock.timing.navigationsStarted,0);assert.equal(status.result.dock.enabled,false,'status must not enable the dock');
    assert.equal(status.result.cooler.enabled,false,'status must not enable the cooler');
    assert.equal(typeof status.result.cooler.diagnosticsEnabled,'boolean');
    assert.ok(Object.hasOwn(status.result.cooler.lastHealthSample,'pnpSampledAt'));
    assert.equal(status.result.cooler.lastHealthSample.message,undefined);assert.equal(status.result.cooler.pid,undefined);
    assert.deepEqual((await call('list_monitors')).result,[{key:'fake:display-1',label:'Simulated display'}]);
    await page.locator('#dock-enabled').check();
    await page.locator('[data-page="dock"]').waitFor({state:'visible'});
    await page.locator('[data-page="dock"]').click();await page.locator('[data-key="0"]').click();
    const original=(await call('get_layout')).result;
    const scope={profileId:original.profileId,pageId:original.pageId,key:1};
    const change=(revision,text)=>({expectedRevision:revision,ops:['a','b'].map(face=>({op:'set_face',...scope,face,value:{kind:'text',text}}))});
    const requested=change(original.revision,'Agent fixture');
    const preview=await call('preview_changes',requested);assert.equal(preview.ok,true);assert.equal(preview.result.saved,false);
    assert.equal((await call('get_layout')).result.revision,original.revision,'preview must not save');
    const applied=await call('apply_changes',requested);assert.equal(applied.ok,true);assert.equal(applied.result.saved,true);assert.equal(applied.result.physicalActionsExecuted,false);
    await wait(()=>page.evaluate(()=>!document.querySelector('.app').inert&&document.getElementById('face-text')?.value==='Agent fixture'),'accepted layout adopted in UI');
    const profileFile=path.join(home,'profiles',`profile-${original.profileId}.json`);
    let disk=JSON.parse(fs.readFileSync(profileFile,'utf8'));let savedPage=disk.pages.find(page=>page.id===original.pageId);
    assert.equal(savedPage.keys[0].a.text,'Agent fixture');assert.equal(savedPage.keys[0].b.text,'Agent fixture');
    assert.equal(savedPage.id,original.pageId);assert.equal(disk.id,original.profileId);
    const stale=await call('apply_changes',requested,second);assert.equal(stale.ok,false);assert.equal(stale.error.code,'revision_conflict');
    await wait(()=>page.evaluate(()=>!document.querySelector('.app').inert),'stale request release');
    const undone=await call('undo_changes',{expectedRevision:applied.result.revision,undoToken:applied.result.undoToken});assert.equal(undone.ok,true);
    await wait(()=>page.evaluate(()=>!document.querySelector('.app').inert),'undo UI release');
    assert.deepEqual((await call('get_layout')).result.keys,original.keys,'undo restores both faces');

    const latest=(await call('get_layout')).result;
    const again=await call('apply_changes',change(latest.revision,'Agent second'));assert.equal(again.ok,true);
    await wait(()=>page.evaluate(()=>!document.querySelector('.app').inert&&document.getElementById('face-text')?.value==='Agent second'),'second accepted UI state');
    await page.locator('#face-text').fill('User draft');
    const focused=await call('apply_changes',change(again.result.revision,'Must not replace draft'));
    assert.equal(focused.ok,false);assert.equal(focused.error.code,'busy');assert.equal(await page.locator('#face-text').inputValue(),'User draft');
    await page.locator('[data-key="1"]').click();
    await wait(async()=>(await call('get_layout')).result.keys[0].a.text==='User draft','user draft saved');
    const staleUndo=await call('undo_changes',{expectedRevision:again.result.revision,undoToken:again.result.undoToken});assert.equal(staleUndo.ok,false);assert.equal(staleUndo.error.code,'revision_conflict');
    await wait(()=>page.evaluate(()=>!document.querySelector('.app').inert),'stale undo UI release');
    const beforeInvalid=(await call('get_layout')).result;
    const invalid=await call('apply_changes',{expectedRevision:beforeInvalid.revision,ops:[{op:'set_face',...scope,face:'a',value:{kind:'text',text:'Partial must not save'}},{op:'rename_page',profileId:scope.profileId,pageId:'missing-page',name:'No'}]});
    assert.equal(invalid.ok,false);await wait(()=>page.evaluate(()=>!document.querySelector('.app').inert),'invalid batch release');
    const afterInvalid=(await call('get_layout')).result;assert.equal(afterInvalid.revision,beforeInvalid.revision);assert.deepEqual(afterInvalid.keys,beforeInvalid.keys,'invalid batch is atomic');

    // Hold a picture import in the client for longer than the host's 8s lease.
    // The release event must wait for flush+reload, rather than unlock stale UI.
    await page.locator('[data-key="0"]').click();await page.locator('#face-kind').selectOption('picture');
    await wait(async()=>(await call('get_layout')).result.keys[0].a.kind==='picture','picture presentation saved');
    const beforeImport=(await call('get_layout')).result;
    await page.evaluate(()=>{
      // Tauri seals its invoke property. Hold this disposable file's browser
      // read instead; assert the delay was reached before claiming coverage.
      const prototype=File.prototype,descriptor=Object.getOwnPropertyDescriptor(prototype,'arrayBuffer'),original=prototype.arrayBuffer;
      window.__agentHeldReads=0;
      window.__restoreAgentImport=()=>{if(descriptor)Object.defineProperty(prototype,'arrayBuffer',descriptor);else delete prototype.arrayBuffer;};
      Object.defineProperty(prototype,'arrayBuffer',{configurable:true,writable:true,value:function(){
        if(this.name!=='agent-fixture.png')return original.call(this);
        window.__agentHeldReads++;
        return new Promise((resolve,reject)=>{window.__finishAgentImport=()=>{window.__finishAgentImport=null;original.call(this).then(resolve,reject);};});
      }});
    });
    const png=await page.evaluate(()=>{const canvas=document.createElement('canvas');canvas.width=4;canvas.height=4;const ctx=canvas.getContext('2d');ctx.fillStyle='#3e99c4';ctx.fillRect(0,0,4,4);return canvas.toDataURL('image/png').split(',')[1];});
    await page.locator('#face-picture').setInputFiles({name:'agent-fixture.png',mimeType:'image/png',buffer:Buffer.from(png,'base64')});
    await wait(()=>page.evaluate(()=>typeof window.__finishAgentImport==='function'),'held picture import');
    assert.equal(await page.evaluate(()=>window.__agentHeldReads),1,'the fixture must actually intercept the file read');
    const pendingApply=call('apply_changes',change(beforeImport.revision,'Must not apply after timeout'));
    await wait(()=>page.evaluate(()=>document.querySelector('.app').inert),'external write editor lease');
    assert.equal((await call('get_status')).result.agentBusy,true,'the native host must own the active lease');
    assert.equal((await proxy(['--quit'])).code,2,'Quit must refuse a live external layout lease');assert.equal(child.exitCode,null);
    const simultaneous=await call('apply_changes',change(beforeImport.revision,'Parallel must not apply'),second);assert.equal(simultaneous.ok,false);assert.equal(simultaneous.error.code,'busy');
    const expired=await pendingApply;assert.equal(expired.ok,false);assert.equal(expired.error.code,'busy');
    assert.equal(await page.evaluate(()=>document.querySelector('.app').inert),true,'timed-out lease must stay inert while client mutation is pending');
    await page.evaluate(()=>{window.__finishAgentImport();window.__restoreAgentImport();});
    await wait(()=>page.evaluate(()=>!document.querySelector('.app').inert),'late import flush and authoritative reload');
    const afterImport=(await call('get_layout')).result;assert.ok(afterImport.keys[0].a.asset);assert.notEqual(afterImport.keys[0].a.text,'Must not apply after timeout');
    disk=JSON.parse(fs.readFileSync(profileFile,'utf8'));savedPage=disk.pages.find(page=>page.id===original.pageId);assert.equal(savedPage.keys[0].a.asset,afterImport.keys[0].a.asset);
    assert.ok(fs.existsSync(path.join(home,'assets',`${afterImport.keys[0].a.asset}.png`)));
    const recovered=await call('apply_changes',change(afterImport.revision,'Recovered agent'));assert.equal(recovered.ok,true);
    await wait(()=>page.evaluate(()=>!document.querySelector('.app').inert&&document.getElementById('face-text')?.value==='Recovered agent'),'writes recover after timeout');
    const dock=await invoke('dock_state');assert.equal(dock.usageReaderRunning,false);assert.equal(dock.enabled,true,'MCP preserves the device explicitly enabled in General');assert.equal(dock.actions,0);
    await invoke('win_close');assert.equal((await call('get_status')).ok,true,'MCP remains available while editor hides to tray');
    await client.close();await second.close();assert.equal(child.exitCode,null,'disconnecting MCP clients must leave UC running');
    const closedClientStatus=await proxy(['--agent-request']);
    // This stdin-less helper has no request; it must return a bounded error and
    // cannot show a window or affect the host.
    assert.notEqual(closedClientStatus.code,0);assert.equal(child.exitCode,null);
    assert.deepEqual(errors,[]);
    console.log('PASS: real MCP SDK → native proxy/pipe → quiet fake UC; access opt-in; previews/atomic durable edits/UI adoption; two-client stale and busy guards; undo; focused draft preservation; 8s lease timeout waits for import/reload; Quit refuses live lease; recovery; tray operation; client disconnect preserves host. No physical device/account/input/audio/display effects.');
  }finally{
    for(const client of clients){try{await client.close();}catch{}}
    if(page){try{await page.evaluate(()=>{window.__finishAgentImport?.();window.__restoreAgentImport?.();});}catch{}}
    if(child.exitCode===null){await delay(150);await proxy(['--quit']);}
    const result=await Promise.race([exited,delay(12000).then(()=>null)]);
    fs.writeFileSync(path.join(home,'test-output.log'),logs);
    if(!result)throw Error('Quiet native UC did not finish cooperative Quit; safety timer remains active.');
    assert.equal(result.code,0,'quiet native UC must exit cleanly');
  }
}
run().catch(error=>{console.error(error);process.exitCode=1;});
