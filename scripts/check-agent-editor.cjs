// Pure modules and headless production UI with fixture IPC only.
// Never starts a native app, reads accounts or connects to devices.
const fs=require('node:fs');
const path=require('node:path');
const assert=require('node:assert/strict');
const root=path.resolve(__dirname,'../public/js');
const load=name=>import('data:text/javascript;base64,'+Buffer.from(fs.readFileSync(path.join(root,name),'utf8')).toString('base64'));
const tick=()=>new Promise(resolve=>setImmediate(resolve));
function deferred(){let resolve,reject;const promise=new Promise((yes,no)=>{resolve=yes;reject=no;});return {promise,resolve,reject};}
function envelope(event,leaseId='lease-one',extra={}){return {event,payload:{leaseId,...extra}};}
function harness(AgentEditorLease,options={}){
  const log=[];let locked=false,accepted=null,ready=true;
  const lease=new AgentEditorLease({
    invoke:async(command,args)=>{log.push(['invoke',command,args]);if(options.invoke)return options.invoke(command,args);},
    busy:()=>options.busy?.()||false,
    flush:async()=>{log.push(['flush']);return options.flush?options.flush():true;},
    lock:value=>{locked=value;log.push(['lock',value]);},
    adopt:async value=>{log.push(['adopt',value]);if(options.adopt)await options.adopt(value);accepted=value;},
    reload:async()=>{log.push(['reload-start']);try{if(options.reload)accepted=await options.reload();else accepted={revision:'authoritative'};log.push(['reload-end']);}catch(error){ready=false;log.push(['editor-unavailable']);throw error;}},
    onError:error=>{log.push(['error',error]);},
  });
  return {lease,log,get locked(){return locked;},get accepted(){return accepted;},get ready(){return ready;}};
}
async function checkHeadless(){
  const {chromium}=require(process.env.UC_PLAYWRIGHT_MODULE||'playwright');
  const http=require('node:http');
  const publicRoot=path.resolve(root,'..');
  const server=http.createServer((req,res)=>{
    const file=path.resolve(publicRoot,'.'+new URL(req.url,'http://localhost').pathname.replace(/\/$/,'/index.html'));
    if(!file.startsWith(publicRoot+path.sep)){res.writeHead(403);return res.end();}
    fs.readFile(file,(error,data)=>{res.writeHead(error?404:200,{'Content-Type':file.endsWith('.js')?'text/javascript':file.endsWith('.css')?'text/css':'text/html'});res.end(error?'Not found':data);});
  });
  let browser;
  try{
    await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
    browser=await chromium.launch({headless:true});const page=await browser.newPage();const errors=[];
    page.on('pageerror',error=>errors.push(error.message));
    await page.addInitScript(()=>{
      const callbacks=new Map(),listeners=new Map();let next=0;
      const dock={enabled:false,state:'disabled',message:'Fixture dock'};
      window.__fixtureLayout={version:1,currentProfile:'fixture-profile',currentPage:'fixture-page',profiles:[{id:'fixture-profile',name:'Fixture',pages:[{id:'fixture-page',name:'Main',keys:Array.from({length:14},()=>({a:{kind:'text',text:''},b:{kind:'text',text:''},action:'none',value:''}))}]}]};
      window.__fixtureRevision='initial';window.__fixtureReadiness=[];window.__fixtureFailReload=false;window.__fixtureSaves=0;
      window.__emitAgent=message=>callbacks.get(listeners.get('uc:agent-event'))({payload:message});
      window.__TAURI_INTERNALS__={transformCallback:callback=>{callbacks.set(++next,callback);return next;},invoke:async(command,args)=>{
        if(command==='plugin:event|listen'){listeners.set(args.event,args.handler);return 1;}
        if(command==='app_state'||command==='settings_update')return {page:'dock',dock,fake:true,agentAccess:true,autostart:false,followPhysicalPage:true};
        if(command==='dock_state')return dock;
        if(command==='profiles_state'){if(window.__fixtureFailReload)throw {code:'fixture-failure'};return {state:structuredClone(window.__fixtureLayout),revision:window.__fixtureRevision};}
        if(command==='profiles_save'){window.__fixtureSaves++;assertRevision(args.expectedRevision);window.__fixtureLayout=structuredClone(args.snapshot);window.__fixtureRevision='user-'+window.__fixtureSaves;return {state:structuredClone(window.__fixtureLayout),revision:window.__fixtureRevision};}
        if(command==='agent_editor_ready'){window.__fixtureReadiness.push(structuredClone(args));return 0;}
        if(command==='key_previews')return [];
        return 0;
      }};
      function assertRevision(revision){if(revision!==window.__fixtureRevision)throw {code:'revision_conflict'};}
    });
    await page.goto(`http://127.0.0.1:${server.address().port}`);
    await page.waitForSelector('#face-text');
    await page.waitForFunction(()=>document.getElementById('storage-status')?.textContent==='Changes saved in Unified Control');
    await page.evaluate(()=>window.__emitAgent({event:'prepare',payload:{leaseId:'headless-success'}}));
    assert.equal(await page.locator('.app').evaluate(app=>app.inert),true);
    await page.evaluate(async()=>{
      window.__fixtureLayout.profiles[0].pages[0].keys[0].a.text='Agent edit';window.__fixtureRevision='agent-one';
      await window.__emitAgent({event:'committed',payload:{leaseId:'headless-success',state:structuredClone(window.__fixtureLayout),revision:window.__fixtureRevision}});
      await window.__emitAgent({event:'released',payload:{leaseId:'headless-success'}});
    });
    assert.equal(await page.locator('#face-text').inputValue(),'Agent edit');
    assert.equal(await page.locator('.app').evaluate(app=>app.inert),false);
    assert.equal(await page.evaluate(()=>window.__fixtureSaves),0,'adopting an agent edit must not save it back');
    console.log('PASS: headless app adopts the committed agent state, reloads and unlocks without save-back');

    await page.locator('#face-text').fill('User is typing');
    await page.evaluate(()=>window.__emitAgent({event:'prepare',payload:{leaseId:'headless-focused'}}));
    assert.equal(await page.locator('.app').evaluate(app=>app.inert),false);
    assert.equal(await page.locator('#face-text').inputValue(),'User is typing');
    assert.deepEqual(await page.evaluate(()=>window.__fixtureReadiness.at(-1)),{leaseId:'headless-focused',saved:false});
    console.log('PASS: headless app refuses an agent lease while the user is typing and keeps the draft');

    await page.locator('[data-page="dock"]').click();
    await page.evaluate(()=>window.__emitAgent({event:'prepare',payload:{leaseId:'headless-reload-error'}}));
    assert.equal(await page.locator('.app').evaluate(app=>app.inert),true);
    await page.evaluate(async()=>{window.__fixtureFailReload=true;await window.__emitAgent({event:'released',payload:{leaseId:'headless-reload-error'}});});
    assert.equal(await page.locator('.app').evaluate(app=>app.inert),false);
    assert.equal(await page.locator('#face-text').count(),0,'a failed authoritative reload must not reopen stale editable controls');
    assert.equal(await page.locator('[data-do="load-profiles"]').count(),1,'the user retains a reload route');
    await page.evaluate(()=>window.__fixtureFailReload=false);await page.locator('[data-do="load-profiles"]').click();
    await page.waitForSelector('#face-text');
    assert.equal(await page.locator('#face-text').inputValue(),'User is typing');
    assert.deepEqual(errors,[]);
    console.log('PASS: headless app replaces a failed reload with recovery controls and can reload the preserved saved state');
  }finally{if(browser)await browser.close();await new Promise(resolve=>server.close(resolve));}
}
async function run(){
  const {AgentEditorLease}=await load('agent-editor.js');
  const {ProfileWriter}=await load('profile-model.js');
  const cases=[];
  const test=(name,fn)=>cases.push([name,fn]);

  test('queued profile writes advance the expected revision between accepted saves',async()=>{
    const first=deferred(),calls=[],statuses=[];
    const writer=new ProfileWriter(async(command,args)=>{calls.push([command,structuredClone(args)]);return calls.length===1?first.promise:{revision:'revision-two'};},status=>statuses.push(status));
    writer.adoptRevision('revision-zero');
    writer.enqueue({fixture:'first'});
    const latest={fixture:'latest'};writer.enqueue(latest);latest.fixture='mutated outside writer';
    assert.equal(calls.length,1,'only one write can be in flight');
    first.resolve({revision:'revision-one'});
    assert.equal(await writer.flush(),true);
    assert.deepEqual(calls,[['profiles_save',{snapshot:{fixture:'first'},expectedRevision:'revision-zero'}],['profiles_save',{snapshot:{fixture:'latest'},expectedRevision:'revision-one'}]]);
    assert.equal(writer.revision,'revision-two');assert.equal(writer.pending,null);assert.equal(writer.failed,false);
    assert.deepEqual(statuses,['saving','saved']);
  });

  test('a revision conflict retains the newest queued draft and stops automatic retries',async()=>{
    const first=deferred(),calls=[],statuses=[];
    const writer=new ProfileWriter(async(command,args)=>{calls.push([command,args]);return first.promise;},(status,error)=>statuses.push([status,error?.code]));
    writer.adoptRevision('stale-revision');writer.enqueue({fixture:'older'});writer.enqueue({fixture:'latest'});
    first.reject({code:'revision_conflict',error:'Dock layout changed'});
    await writer.completion;await tick();
    assert.equal(calls.length,1);assert.deepEqual(writer.pending,{fixture:'latest'});
    assert.equal(writer.failed,true);assert.equal(writer.running,false);assert.equal(writer.revision,'stale-revision');
    assert.deepEqual(statuses.at(-1),['failed','revision_conflict']);
    assert.throws(()=>writer.adoptRevision('new-revision'),/Finish pending layout edits/,'revision adoption must not discard a failed draft');
  });

  test('an invalid older draft may yield to the latest valid queued draft',async()=>{
    const first=deferred(),calls=[];
    const writer=new ProfileWriter(async(command,args)=>{calls.push(args);return calls.length===1?first.promise:{revision:'accepted'};},()=>{});
    writer.adoptRevision('initial');writer.enqueue({name:''});writer.enqueue({name:'Valid name'});
    first.reject({code:'profiles',error:'Invalid name'});
    assert.equal(await writer.flush(),true);
    assert.equal(calls.length,2);assert.equal(calls[1].expectedRevision,'initial');assert.equal(calls[1].snapshot.name,'Valid name');
  });

  test('an editor lease waits for an import and its durable flush before acknowledging readiness',async()=>{
    const imported=deferred(),flushed=deferred();let importedFinished=false;
    const h=harness(AgentEditorLease,{flush:async()=>{await imported.promise;importedFinished=true;return flushed.promise;}});
    const prepare=h.lease.event(envelope('prepare'));await tick();
    assert.equal(h.locked,true);assert.equal(h.log.filter(item=>item[0]==='invoke').length,0);
    imported.resolve();await tick();assert.equal(importedFinished,true);assert.equal(h.log.filter(item=>item[0]==='invoke').length,0);
    flushed.resolve(true);await prepare;
    assert.deepEqual(h.log.find(item=>item[0]==='invoke'),['invoke','agent_editor_ready',{leaseId:'lease-one',saved:true}]);
    assert.equal(h.locked,true,'the host release, not successful flush, owns unlocking');
    await h.lease.event(envelope('released'));assert.equal(h.locked,false);
  });

  test('committed state is adopted and an authoritative release reload finishes before unlocking',async()=>{
    const adopted=deferred(),reloaded=deferred();
    const h=harness(AgentEditorLease,{adopt:()=>adopted.promise,reload:()=>reloaded.promise});
    await h.lease.event(envelope('prepare'));
    const committed=h.lease.event(envelope('committed','lease-one',{revision:'committed',state:{fixture:'agent edit'}}));
    const released=h.lease.event(envelope('released'));await tick();
    assert.equal(h.locked,true);assert.equal(h.log.some(item=>item[0]==='reload-start'),false,'release must wait for adoption');
    adopted.resolve();await committed;await tick();
    assert.equal(h.locked,true);assert.equal(h.log.some(item=>item[0]==='reload-start'),true);
    reloaded.resolve({revision:'latest-host-state'});await released;
    assert.equal(h.accepted.revision,'latest-host-state');assert.equal(h.locked,false);assert.equal(h.lease.current,null);
    assert.ok(h.log.findIndex(item=>item[0]==='reload-end')<h.log.findIndex(item=>item[0]==='lock'&&item[1]===false));
  });

  test('a focused edit refuses the lease without locking or flushing',async()=>{
    let focused=true;
    const h=harness(AgentEditorLease,{busy:()=>focused});
    await h.lease.event(envelope('prepare'));await h.lease.event(envelope('released'));
    assert.deepEqual(h.log,[['invoke','agent_editor_ready',{leaseId:'lease-one',saved:false}]]);
    assert.equal(h.lease.current,null);assert.equal(h.locked,false);
    focused=false;await h.lease.event(envelope('prepare','after-edit'));assert.equal(h.locked,true);
    await h.lease.event(envelope('released','after-edit'));
  });

  test('a real writer failure refuses the lease without retrying or flushing away the failed draft',async()=>{
    let saves=0;
    const writer=new ProfileWriter(async()=>{saves++;throw {code:'profiles'};},()=>{});
    writer.adoptRevision('fixture');writer.enqueue({fixture:'unsaved'});await writer.completion;
    const h=harness(AgentEditorLease,{busy:()=>writer.failed,flush:()=>writer.flush()});
    await h.lease.event(envelope('prepare'));await h.lease.event(envelope('released'));
    assert.deepEqual(h.log,[['invoke','agent_editor_ready',{leaseId:'lease-one',saved:false}]]);
    assert.equal(saves,1);assert.deepEqual(writer.pending,{fixture:'unsaved'});assert.equal(writer.failed,true);
  });

  test('a timeout release queued during a long flush waits and reloads accepted state',async()=>{
    const flushed=deferred();
    const h=harness(AgentEditorLease,{flush:()=>flushed.promise,invoke:async()=>{throw {code:'stale-lease'};},reload:async()=>({revision:'host-after-timeout'})});
    const prepare=h.lease.event(envelope('prepare'));await tick();
    const released=h.lease.event(envelope('released'));await tick();
    assert.equal(h.locked,true);assert.equal(h.log.some(item=>item[0]==='reload-start'),false);
    flushed.resolve(true);await prepare;await released;
    assert.equal(h.accepted.revision,'host-after-timeout');assert.equal(h.locked,false);
    assert.equal(h.log.filter(item=>item[0]==='error').length,1,'a rejected late ready acknowledgement is surfaced');
    assert.ok(h.log.findIndex(item=>item[0]==='invoke')<h.log.findIndex(item=>item[0]==='reload-start'));
  });

  for(const kind of ['false','rejected'])test(`a ${kind} flush preserves the failed draft when the lease is released`,async()=>{
    const draft={fixture:'unsaved user draft'};
    const h=harness(AgentEditorLease,{flush:async()=>{if(kind==='rejected')throw new Error('fixture failure');return false;},reload:async()=>{draft.fixture='would lose draft';return draft;}});
    await h.lease.event(envelope('prepare'));await h.lease.event(envelope('released'));
    assert.equal(draft.fixture,'unsaved user draft');assert.equal(h.log.some(item=>item[0]==='reload-start'),false);
    assert.deepEqual(h.log.find(item=>item[0]==='invoke'),['invoke','agent_editor_ready',{leaseId:'lease-one',saved:false}]);
    assert.equal(h.locked,false);assert.equal(h.lease.current,null);
  });

  test('stale lease events and another prepare cannot adopt or unlock an active lease',async()=>{
    const h=harness(AgentEditorLease);await h.lease.event(envelope('prepare'));
    await h.lease.event(envelope('committed','stale',{revision:'stale'}));await h.lease.event(envelope('released','stale'));
    await h.lease.event(envelope('prepare','second'));
    assert.equal(h.lease.current,'lease-one');assert.equal(h.locked,true);assert.equal(h.accepted,null);
    assert.deepEqual(h.log.at(-1),['invoke','agent_editor_ready',{leaseId:'second',saved:false}]);
    await h.lease.event(envelope('released'));assert.equal(h.locked,false);
  });

  test('a failed committed-state adoption remains locked until authoritative recovery',async()=>{
    const h=harness(AgentEditorLease,{adopt:async()=>{throw new Error('fixture adoption failure');}});
    await h.lease.event(envelope('prepare'));await h.lease.event(envelope('committed','lease-one',{revision:'new'}));
    assert.equal(h.locked,true);assert.equal(h.lease.current,'lease-one');assert.equal(h.log.filter(item=>item[0]==='error').length,1);
    await h.lease.event(envelope('released'));assert.equal(h.accepted.revision,'authoritative');assert.equal(h.locked,false);
  });

  test('a failed release reload disables layout editing before it releases the interaction lock',async()=>{
    // Mirrors app.js's reload catch: render the unavailable/reload-only view
    // before unlocking, preserving a route for the user to recover.
    const h=harness(AgentEditorLease,{reload:async()=>{throw new Error('fixture reload failure');}});
    await h.lease.event(envelope('prepare'));await h.lease.event(envelope('released'));
    assert.equal(h.ready,false);assert.equal(h.accepted,null);assert.equal(h.locked,false);
    assert.ok(h.log.findIndex(item=>item[0]==='editor-unavailable')<h.log.findIndex(item=>item[0]==='lock'&&item[1]===false));
    assert.equal(h.log.filter(item=>item[0]==='error').length,1);
  });

  for(const [name,fn] of cases){
    let timer;
    try{await Promise.race([fn(),new Promise((_,reject)=>{timer=setTimeout(()=>reject(new Error(`Timed out: ${name}`)),3000);})]);}
    finally{clearTimeout(timer);}
    console.log(`PASS: ${name}`);
  }
  console.log(`PASS: ${cases.length} production-module checks; fixture-only profile writes and lease events, no native/device access.`);
  await checkHeadless();
}
run().catch(error=>{console.error(error);process.exitCode=1;});
