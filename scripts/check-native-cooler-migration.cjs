// Isolated debug host + real worker IPC. Fake devices and disabled source plugins only.
const {chromium}=require(process.env.UC_PLAYWRIGHT_MODULE||'playwright');
const {spawn}=require('node:child_process');
const fs=require('node:fs'),path=require('node:path'),os=require('node:os'),net=require('node:net'),assert=require('node:assert/strict');
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
const write=(file,value)=>{fs.mkdirSync(path.dirname(file),{recursive:true});fs.writeFileSync(file,typeof value==='string'||Buffer.isBuffer(value)?value:JSON.stringify(value));};
const snapshot=root=>{
  const files={};
  const visit=(dir,prefix='')=>{for(const entry of fs.readdirSync(dir,{withFileTypes:true}).sort((a,b)=>a.name.localeCompare(b.name))){
    const relative=path.join(prefix,entry.name),file=path.join(dir,entry.name);
    assert.ok(!entry.isSymbolicLink(),'Fixture must contain no links');
    if(entry.isDirectory())visit(file,relative);else files[relative]=fs.readFileSync(file).toString('base64');
  }};visit(root);return files;
};
const finite=value=>{
  if(typeof value==='number')assert.ok(Number.isSafeInteger(value)&&value>=0,'Report counts must be finite nonnegative integers');
  else if(Array.isArray(value))value.forEach(finite);
  else if(value&&typeof value==='object')Object.values(value).forEach(finite);
};
(async()=>{
  const exe=path.resolve(process.env.UC_TEST_EXE||''),worker=path.resolve(process.env.UC_COOLER_WORKER_EXE||'');
  for(const file of [exe,worker])if(!file.toLowerCase().includes(`${path.sep}debug${path.sep}`)||!fs.existsSync(file))throw Error('Debug executables required; installed/release apps refused');
  const home=fs.mkdtempSync(path.join(os.tmpdir(),'uc-native-cooler-migration-'));
  const old=path.join(home,'test-empty-cooler-source'),gallery=path.join(home,'test-empty-gallery');
  const plugins=path.join(home,'test-empty-sources'),inherited=path.join(home,'inherited-sources');
  const target=path.join(home,'devices','cooler'),receipt=path.join(home,'cooler-migration-verification.json');
  const png=Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScLbtAAAAABJRU5ErkJggg==','base64');
  const clip=path.join(gallery,'fixture.png');write(clip,png);
  write(path.join(old,'settings.json'),{galleryDir:gallery,live:{enabled:false},sources:{enabled:{},options:{},ticked:['mine']}});
  write(path.join(old,'companion.json'),{fixture:true});
  write(path.join(old,'library','fixture01','original.json'),{
    id:'fixture01',name:'Migration fixture',file:clip,fileName:'fixture.png',extracted:false,
    fileSize:png.length,fileMtime:fs.statSync(clip).mtimeMs,created:'2026-01-01T00:00:00.000Z',
    duration:null,isImage:true,width:1,height:1,codec:'png',fps:0,assets:'ready',assetsError:null,
    proxy:null,missing:null,sourceChanged:false
  });
  write(path.join(old,'library','fixture01','theme.json'),{widgets:[{kind:'clock'}]});
  write(path.join(old,'library','fixture01','theme-card.png'),png);
  const plugin=name=>({format:1,name,allowedHosts:['example.invalid'],list:{url:'https://example.invalid/list',itemsPath:'items',fields:{id:'id',title:'title'}},download:{url:'https://example.invalid/{id}.mp4'}});
  write(path.join(plugins,'owned-only.json'),plugin('Owned fixture'));
  write(path.join(inherited,'inherited-only.json'),plugin('Inherited fixture'));
  const originals={old:snapshot(old),gallery:snapshot(gallery),plugins:snapshot(plugins),inherited:snapshot(inherited)};
  const unchanged=()=>{for(const [key,dir] of Object.entries({old,gallery,plugins,inherited}))assert.deepEqual(snapshot(dir),originals[key],`${key} original fixture changed`);};
  const port=await new Promise(resolve=>{const server=net.createServer();server.listen(0,'127.0.0.1',()=>{const value=server.address().port;server.close(()=>resolve(value));});});
  const env={...process.env,UC_HOME:home,UC_TEST_QUIET:'1',UC_MULTI:'1',UC_FAKE_DEVICE:'1',UC_COOLER_WORKER_EXE:worker,COOLERSCREEN_SOURCES_DIR:inherited,WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:`--remote-debugging-port=${port}`};
  const child=spawn(exe,[],{env,windowsHide:true,stdio:'ignore'});let browser,page,call,jobs;
  const exit=new Promise((resolve,reject)=>{child.once('exit',resolve);child.once('error',reject);});
  const requestQuit=()=>new Promise((resolve,reject)=>{const process=spawn(exe,['--quit'],{env,windowsHide:true,stdio:'ignore'});process.once('error',reject);process.once('exit',resolve);});
  const poll=async(predicate,label,timeout=30000)=>{const deadline=Date.now()+timeout;while(Date.now()<deadline){if(await predicate())return;await delay(75);}throw Error(`Timed out: ${label}`);};
  try{
    await poll(async()=>{try{return (await fetch(`http://127.0.0.1:${port}/json/version`)).ok;}catch{return false;}},'debug endpoint');
    browser=await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
    await poll(()=>{page=browser.contexts().flatMap(context=>context.pages()).find(candidate=>/tauri/.test(candidate.url()));return !!page;},'host WebView');
    const errors=[];page.on('pageerror',error=>errors.push(error.message));
    await page.waitForFunction(()=>document.getElementById('storage-status')?.textContent==='Changes saved in Unified Control');
    call=(command,args={})=>page.evaluate(async({command,args})=>{try{return await window.__TAURI_INTERNALS__.invoke(command,args);}catch(error){throw Error(JSON.stringify(error));}},{command,args});
    jobs=async()=>(await call('activity')).jobs.filter(job=>job.kind==='cooler-migration-check');
    await call('cooler_configure',{enabled:true});
    await poll(async()=>{const state=await call('cooler_state');return state.state==='running'&&!state.busy&&state.migration?.state==='complete';},'automatic migration verification');
    const report=(await call('cooler_state')).migration;
    assert.equal(report.verified,true);assert.equal(report.reportVersion,1);assert.equal(report.migrationVersion,2);assert.equal(report.originalKept,true);finite(report);
    assert.equal(report.library.originals,1);assert.equal(report.library.themes,1);assert.equal(report.gallery.targetFiles,1);
    assert.equal(report.library.missingFiles,0);assert.equal(report.gallery.missingFiles,0);assert.equal(report.references.oldDependencies,0);assert.equal(report.references.missing,0);
    assert.equal(report.library.changedPreviews,0);
    assert.equal(report.sourcePlugins.imported,true);assert.equal(report.sourcePlugins.copiedFilesAtImport,1);
    assert.ok(Number.isFinite(Date.parse(report.checkedAt)));assert.ok(!JSON.stringify(report).includes(home),'Status must not expose fixture paths');
    assert.deepEqual(fs.readFileSync(path.join(target,'Sources','owned-only.json')),fs.readFileSync(path.join(plugins,'owned-only.json')));
    assert.ok(!fs.existsSync(path.join(target,'Sources','inherited-only.json')));
    assert.equal(JSON.parse(fs.readFileSync(path.join(target,'migration.json'),'utf8')).version,2);
    const sources=await call('cooler_call',{command:'sources_list',args:{}});
    // Fake worker deliberately uses NoSources: it must not discover plugins or
    // make network requests. This fixture verifies the imported files only;
    // Command environment tests cover the worker's owned Sources override.
    assert.deepEqual(sources.sources,[],'Fake worker must retain its NoSources backend');
    unchanged();
    const firstJobs=(await jobs()).map(job=>job.id),receiptBefore=fs.readFileSync(receipt,'utf8');
    assert.equal(firstJobs.length,1,'Enabling schedules one automatic audit');
    for(let n=0;n<5;n++)assert.equal((await call('cooler_state')).migration.checkedAt,report.checkedAt);
    assert.deepEqual((await jobs()).map(job=>job.id),firstJobs,'Status reads must not launch more audits');
    assert.equal(fs.readFileSync(receipt,'utf8'),receiptBefore,'Status reads must not rewrite the receipt');
    await page.locator('.nav-item[data-page="general"]').click();
    const button=page.locator('#cooler-migration-verify');await button.waitFor({state:'visible'});
    await poll(()=>button.isEnabled(),'General migration state refresh');
    assert.match(await page.locator('#cooler-migration-status').textContent(),/Last verification/i,'Counts must carry a historical verification label');
    const regenerated=Buffer.concat([png,Buffer.from('regenerated preview fixture')]);
    fs.writeFileSync(path.join(target,'library','fixture01','theme-card.png'),regenerated);
    await button.click();
    await poll(async()=>{const current=(await call('cooler_state')).migration;return current.state==='complete'&&current.checkedAt!==report.checkedAt&&(await jobs()).length===2;},'explicit General verification');
    const checked=(await call('cooler_state')).migration;finite(checked);assert.equal(checked.verified,true);assert.equal(checked.library.changedPreviews,1);assert.equal(checked.library.changedMediaFiles,0);
    assert.deepEqual(fs.readFileSync(path.join(target,'library','fixture01','theme-card.png')),regenerated);
    const saved=JSON.parse(fs.readFileSync(receipt,'utf8'));assert.equal(saved.checkedAt,checked.checkedAt);assert.equal(saved.verified,true);assert.equal(saved.state,'complete');
    assert.deepEqual((await jobs()).filter(job=>!firstJobs.includes(job.id)).length,1,'General click adds one explicit audit');
    unchanged();assert.deepEqual(errors,[]);
    await poll(async()=>!(await call('cooler_state')).busy,'idle cooler');
    await call('cooler_configure',{enabled:false});
    assert.equal(await requestQuit(),0);assert.equal(await Promise.race([exit,delay(10000).then(()=>{throw Error('Quiet host did not quit');})]),0);
    console.log('PASS: automatic cooler migration audit; finite historical report; original and inherited files preserved; owned Sources copied; fake NoSources retained; cached status reads; General explicit verification and host receipt; cooperative Quit. Fake devices only; actual source catalog discovery not exercised.');
    console.log(`Isolated evidence: ${home}`);
  }catch(error){
    if(call&&page&&!page.isClosed()){
      const state=await call('cooler_state').catch(()=>null);
      const activity=await jobs().catch(()=>[]);
      const ui=await page.evaluate(()=>({buttonText:document.getElementById('cooler-migration-verify')?.textContent,buttonDisabled:document.getElementById('cooler-migration-verify')?.disabled,status:document.getElementById('cooler-migration-status')?.textContent})).catch(()=>null);
      console.error('Migration fixture failure:',JSON.stringify({home,migration:state?.migration,jobs:activity.map(job=>({id:job.id,state:job.state,status:job.status,code:job.code})),ui}));
    }
    throw error;
  }finally{
    if(child.exitCode===null){await requestQuit();await Promise.race([exit,delay(10000)]);}
    if(browser)await browser.close();
  }
})().catch(error=>{console.error(error);process.exitCode=1;});
