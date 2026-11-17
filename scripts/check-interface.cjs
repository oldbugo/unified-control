const { chromium } = require(process.env.UC_PLAYWRIGHT_MODULE || 'playwright');
const http = require('node:http');
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const root = path.resolve(__dirname, '../public');
const output = process.env.UC_PREVIEW_OUTPUT || path.join(require('node:os').tmpdir(), 'uc-interface-preview');
fs.mkdirSync(output, {recursive:true});
const server = http.createServer((req,res) => {
  const file = path.resolve(root, '.' + new URL(req.url,'http://localhost').pathname.replace(/\/$/,'/index.html'));
  if (!file.startsWith(path.resolve(root) + path.sep)) {res.writeHead(403);return res.end();}
  fs.readFile(file,(err,data)=>{res.writeHead(err?404:200,{'Content-Type':file.endsWith('.js')?'text/javascript':file.endsWith('.css')?'text/css':'text/html'});res.end(err?'Not found':data);});
});
(async()=>{
  await new Promise(r=>server.listen(0,'127.0.0.1',r));
  let browser;
  try {
    browser = await chromium.launch({headless:true});
    const page = await browser.newPage({viewport:{width:1440,height:1000}});
    const errors=[];page.on('pageerror',e=>errors.push(e.message));
    const base = `http://127.0.0.1:${server.address().port}`;
    await page.goto(base);
    assert.equal(await page.locator('.nav-item').count(),8); // Home, cooler, dock, Vibe Key, dial, General, About, Input tester
    await page.locator('[data-page="dock"]').click();
    assert.equal(await page.locator('[data-key]').count(),14);
    assert.equal(await page.locator('.allowance-values').first().textContent(),'——');
    await page.locator('[data-key="6"]').click();
    await page.locator('#face-kind').selectOption('text');
    await page.locator('#face-text').fill('Focus');
    await page.locator('[data-edit-face="b"]').click();
    await page.locator('#face-kind').selectOption('text');
    await page.locator('#face-text').fill('Break');
    await page.locator('#action-kind').selectOption('hotkey');
    await page.locator('#action-value').fill('Ctrl + Shift + M');
    await page.locator('[data-do="preview-flip"]').first().click();
    assert.match(await page.locator('[data-key="6"]').textContent(),/Focus/);
    await page.locator('[data-do="preview-flip"]').first().click();
    assert.match(await page.locator('[data-key="6"]').textContent(),/Break/);
    await page.reload();
    await page.locator('[data-key="6"]').click();
    await page.locator('[data-edit-face="b"]').click();
    assert.equal(await page.locator('#face-text').inputValue(),'Break');
    assert.equal(await page.locator('#action-value').inputValue(),'Ctrl + Shift + M');
    await page.locator('#face-text').fill('Undo this change');
    await page.locator('[data-do="undo"]').click();
    await page.locator('[data-key="6"]').click();
    await page.locator('[data-edit-face="b"]').click();
    assert.equal(await page.locator('#face-text').inputValue(),'Break');
    await page.locator('#face-kind').selectOption('codex');
    assert.equal(await page.locator('#usage-view option').count(),16);
    await page.locator('#usage-view').selectOption('tokens');
    await page.locator('#face-kind').selectOption('claude');
    assert.equal(await page.locator('#usage-view option').count(),15);
    assert.equal(await page.locator('#usage-view').inputValue(),'rings');
    await page.locator('#face-kind').selectOption('picture');
    const picture=await page.evaluate(()=>{const canvas=document.createElement('canvas');canvas.width=4;canvas.height=4;return canvas.toDataURL('image/png').split(',')[1];});
    await page.locator('#face-picture').setInputFiles({name:'fixture.png',mimeType:'image/png',buffer:Buffer.from(picture,'base64')});
    await page.waitForFunction(()=>document.querySelector('[data-key="6"] img')?.naturalWidth===4);
    await page.reload();
    await page.locator('[data-key="6"]').click();
    await page.locator('[data-edit-face="b"]').click();
    await page.waitForFunction(()=>document.querySelector('[data-key="6"] img')?.naturalWidth===4);
    await page.locator('#face-kind').selectOption('text');
    await page.locator('#action-kind').selectOption('launch');
    await page.locator('#action-value').fill('C:\\Example\\App.exe');
    await page.locator('#launch-args').fill('--name\nvalue with spaces\n$(literal)');
    const launch=await page.evaluate(()=>JSON.parse(JSON.parse(localStorage.getItem('uc.interface-prototype.v1')).profiles[0].pages[0].keys[6].value));
    assert.deepEqual(launch,{path:'C:\\Example\\App.exe',args:['--name','value with spaces','$(literal)']});
    await page.locator('#face-text').fill('<img src=x onerror=alert(1)>');
    assert.equal(await page.locator('[data-key="6"] img').count(),0);
    await page.locator('#face-text').fill('Break');
    await page.locator('[data-do="add-page"]').click();
    assert.equal(await page.locator('[data-dock-page]').count(),2);
    await page.locator('[data-do="add-profile"]').click();
    assert.equal(await page.locator('#profile-picker option').count(),2);
    await page.locator('#profile-picker').selectOption('0');
    await page.locator('[data-dock-page="0"]').click();
    await page.locator('[data-edit-face="a"]').click();
    for (const [w,h] of [[1440,1000],[1100,800],[800,650]]) {
      await page.setViewportSize({width:w,height:h});
      await page.locator('#pane').evaluate(el=>el.scrollTop=0);
      await page.screenshot({path:path.join(output,`uc-dock-${w}.png`),fullPage:true});
      assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>window.innerWidth),false,`horizontal overflow at ${w}`);
    }
    await page.setViewportSize({width:1440,height:1000});
    await page.locator('[data-page="home"]').click();
    await page.screenshot({path:path.join(output,'uc-home.png'),fullPage:true});
    await page.locator('[data-page="cooler"]').click();
    assert.match(await page.locator('#pane').textContent(),/integrated controls are available in the Windows app/);
    await page.screenshot({path:path.join(output,'uc-cooler.png'),fullPage:true});
    assert.equal(await page.locator('[data-page="dial"]').isVisible(),false,'unimplemented Dial is absent from the workspace');
    await page.locator('[data-page="general"]').click();
    assert.equal(await page.locator('#autostart-enabled').isDisabled(),true);
    await page.locator('[data-page="about"]').click();
    const modelCheck=await page.evaluate(async()=>{
      const {toNative,fromNative,ProfileWriter}=await import('/js/profile-model.js');
      const draft=JSON.parse(localStorage.getItem('uc.interface-prototype.v1'));
      draft.profile=0; draft.page=0;
      draft.profiles[0].pages[0].keys[0].action='page';
      draft.profiles[0].pages[0].keys[0].value='1';
      const native=toNative(draft), roundtrip=fromNative(native);
      const target=native.profiles[0].pages[1].id;
      const called=[], resolvers=[], statuses=[];
      const writer=new ProfileWriter((cmd,args)=>{called.push(args.snapshot.n);return new Promise(resolve=>resolvers.push(resolve));},status=>statuses.push(status));
      writer.enqueue({n:1}); writer.enqueue({n:2}); writer.enqueue({n:3});
      resolvers.shift()(); await Promise.resolve(); await Promise.resolve();
      resolvers.shift()(); await Promise.resolve(); await Promise.resolve();
      let fail=true;
      const retry=new ProfileWriter(()=>fail?Promise.reject(new Error('disk')):Promise.resolve(),()=>{});
      retry.enqueue({n:4}); await Promise.resolve(); await Promise.resolve();
      const retained=retry.failed && retry.pending.n===4;
      fail=false; await retry.drain();
      return {stable:toNative(draft).profiles[0].id===native.profiles[0].id,
        target:native.profiles[0].pages[0].keys[0].value===target,
        roundtrip:roundtrip.profiles[0].pages[0].keys[0].value,
        called,statuses,retained,retried:!retry.failed && !retry.pending};
    });
    assert.equal(modelCheck.stable,true);
    assert.equal(modelCheck.target,true);
    assert.equal(modelCheck.roundtrip,'1');
    assert.deepEqual(modelCheck.called,[1,3]);
    assert.equal(modelCheck.retained,true);
    assert.equal(modelCheck.retried,true);
    await page.evaluate(()=>localStorage.setItem('uc.interface-prototype.v1','{broken'));
    await page.reload();
    assert.match(await page.locator('#storage-status').textContent(),/could not be loaded/);
    assert.deepEqual(errors,[]);
    // Keyboard editing must retain the semantic control replaced by a pane
    // redraw. These pages are isolated browser drafts, never native profiles.
    for(const [width,height] of [[1440,1000],[1100,800],[800,650]]){
      const keyboardPage=await browser.newPage({viewport:{width,height}});
      const keyboardErrors=[];keyboardPage.on('pageerror',error=>keyboardErrors.push(error.message));
      await keyboardPage.goto(base);
      await keyboardPage.locator('[data-page="dock"]').click();
      await keyboardPage.locator('[data-key="6"]').focus();
      await keyboardPage.keyboard.press('Enter');
      assert.equal(await keyboardPage.locator('[data-key="6"]').evaluate(el=>el===document.activeElement),true,`key activation focus at ${width}`);
      await keyboardPage.locator('[data-edit-face="b"]').focus();
      await keyboardPage.keyboard.press('Enter');
      assert.equal(await keyboardPage.locator('[data-edit-face="b"]').evaluate(el=>el===document.activeElement),true,`face toggle focus at ${width}`);
      await keyboardPage.locator('#face-kind').focus();
      await keyboardPage.keyboard.press('Home');
      await keyboardPage.keyboard.press('ArrowDown');
      assert.equal(await keyboardPage.locator('#face-kind').inputValue(),'icon');
      assert.equal(await keyboardPage.evaluate(()=>{const draft=JSON.parse(localStorage.getItem('uc.interface-prototype.v1'));return !!draft.profiles[0].id&&!!draft.profiles[0].pages[0].id;}),true,'initial browser drafts acquire stable identities before undo checkpoints');
      assert.equal(await keyboardPage.locator('#face-kind').evaluate(el=>el===document.activeElement),true,`presentation focus at ${width}`);
      await keyboardPage.keyboard.press('Tab');
      assert.equal(await keyboardPage.locator('#face-text').evaluate(el=>el===document.activeElement),true,`next presentation field at ${width}`);
      await keyboardPage.locator('#action-kind').focus();
      await keyboardPage.keyboard.press('Home');
      await keyboardPage.keyboard.press('ArrowDown');
      assert.equal(await keyboardPage.locator('#action-kind').inputValue(),'hotkey');
      assert.equal(await keyboardPage.locator('#action-kind').evaluate(el=>el===document.activeElement),true,`action focus at ${width}`);
      await keyboardPage.keyboard.press('Tab');
      assert.equal(await keyboardPage.locator('#action-value').evaluate(el=>el===document.activeElement),true,`next action field at ${width}`);
      for(const index of [0,1]){
        await keyboardPage.locator('[data-do="preview-flip"]').nth(index).focus();
        await keyboardPage.keyboard.press('Enter');
        assert.equal(await keyboardPage.locator('[data-do="preview-flip"]').nth(index).evaluate(el=>el===document.activeElement),true,`duplicate flip control ${index} at ${width}`);
      }
      // A redraw must neither steal outside focus nor restore a removed field.
      await keyboardPage.locator('[data-page="dock"]').focus();
      await keyboardPage.locator('#profile-picker').evaluate(el=>el.dispatchEvent(new Event('change',{bubbles:true})));
      assert.equal(await keyboardPage.locator('[data-page="dock"]').evaluate(el=>el===document.activeElement),true);
      await keyboardPage.locator('[data-key="6"]').click();
      await keyboardPage.locator('#face-kind').selectOption('text');
      await keyboardPage.locator('#face-text').focus();
      await keyboardPage.locator('#face-kind').evaluate(el=>{el.value='clock';el.dispatchEvent(new Event('change',{bubbles:true}));});
      assert.equal(await keyboardPage.locator('#face-text').count(),0);
      assert.equal(await keyboardPage.evaluate(()=>document.activeElement===document.body),true);
      await keyboardPage.locator('#face-kind').selectOption('text');
      await keyboardPage.reload();
      await keyboardPage.locator('[data-key="6"]').click();
      await keyboardPage.locator('#face-text').fill(`Undo key seven ${width}`);
      await keyboardPage.locator('[data-do="undo"]').focus();
      await keyboardPage.keyboard.press('Enter');
      assert.equal(await keyboardPage.locator('.editor-head h2').textContent(),'Key 7',`undo keeps display context at ${width}`);
      assert.equal(await keyboardPage.locator('[data-key="6"]').evaluate(el=>el===document.activeElement),true,'last Undo becomes disabled; focus stays on the selected display');
      await keyboardPage.locator('[data-do="add-page"]').click();
      await keyboardPage.locator('[data-key="6"]').click();
      await keyboardPage.locator('#face-text').fill('New page edit');
      await keyboardPage.locator('[data-do="undo"]').click();
      assert.equal(await keyboardPage.locator('.editor-head h2').textContent(),'Key 7');
      await keyboardPage.locator('[data-do="undo"]').click();
      assert.equal(await keyboardPage.locator('[data-dock-page]').count(),1);
      assert.equal(await keyboardPage.locator('.editor-head h2').textContent(),'Key 1','removing the active page uses the existing selection fallback');
      assert.deepEqual(keyboardErrors,[]);
      await keyboardPage.close();
    }
    const guardPage=await browser.newPage();
    await guardPage.addInitScript(()=>{
      window.__TAURI_INTERNALS__={transformCallback:()=>1,invoke:async(command,args)=>{
        if(command==='plugin:event|listen' && args.event==='uc:prepare-quit') throw new Error('guard unavailable');
        if(command==='app_state') return {page:'dock',appVersion:'test',autostart:false};
        if(command==='profiles_state') return {state:{version:1,currentProfile:'p',currentPage:'pg',profiles:[{id:'p',name:'Profile',pages:[{id:'pg',name:'Main',keys:Array.from({length:14},()=>({a:{kind:'text',text:''},b:{kind:'text',text:''},action:'none',value:''}))}]}]}};
        return 0;
      }};
    });
    await guardPage.goto(base);
    await guardPage.waitForFunction(()=>document.getElementById('storage-status')?.textContent.includes('Layout editing is disabled'));
    assert.equal(await guardPage.locator('#face-text').count(),0);
    assert.equal(await guardPage.locator('[data-key]').count(),0);
    await guardPage.close();
    // Exercise the actual native-preview UI path with fixture-only IPC. A slow
    // decode must leave the previous decoded image visible; a polling tick with
    // identical bytes must not replace any image node or create another URL.
    const nativePreview=await browser.newPage();
    const nativeErrors=[];nativePreview.on('pageerror',error=>nativeErrors.push(error.message));
    await nativePreview.addInitScript(()=>{
      window.__previewRevision=0;window.__previewCalls=0;window.__decodeCalls=0;window.__holdPreviewDecode=false;window.__pendingPreviewDecodes=[];
      window.__previewUrls=new Set();window.__previewUrlCount=0;
      const createURL=URL.createObjectURL.bind(URL),revokeURL=URL.revokeObjectURL.bind(URL);
      URL.createObjectURL=blob=>{const url=createURL(blob);window.__previewUrls.add(url);window.__previewUrlCount++;return url;};
      URL.revokeObjectURL=url=>{window.__previewUrls.delete(url);return revokeURL(url);};
      const decode=HTMLImageElement.prototype.decode;
      HTMLImageElement.prototype.decode=async function(){
        window.__decodeCalls++;
        await decode.call(this);
        if(window.__holdPreviewDecode)await new Promise(resolve=>window.__pendingPreviewDecodes.push(resolve));
      };
      const frames=new Map();
      const dock={state:'connected',enabled:true,message:'Fixture dock connected',fake:true};
      const appState={page:'dock',appVersion:'fixture',autostart:false,dock};
      window.__TAURI_INTERNALS__={transformCallback:()=>1,invoke:async(command,args)=>{
        if(command==='app_state'||command==='settings_update')return appState;
        if(command==='cooler_controls_state'){
          window.__coolerChoiceReads=(window.__coolerChoiceReads||0)+1;
          if(window.__coolerChoicesFail)throw new Error('fixture unavailable');
          return {available:true,running:true,connected:true,themes:window.__coolerThemes||[{id:'theme-alpha',name:'Alpha'},{id:'theme-beta',name:'Beta <safe>'}],currentThemeId:'theme-alpha'};
        }
        if(command==='cooler_call'||command==='cooler_action')throw new Error('Editor must never execute a cooler action');
        if(command==='dock_state')return dock;
        if(command==='profiles_save'){window.__lastSavedSnapshot=structuredClone(args.snapshot);return 0;}
        if(command==='profiles_state')return {state:{version:1,currentProfile:'p',currentPage:'pg',profiles:[{id:'p',name:'Fixture',pages:[{id:'pg',name:'Main',keys:Array.from({length:14},(_,index)=>({a:{kind:index===0?'clock':'text',text:''},b:{kind:'text',text:''},action:'none',value:''}))}]}]}};
        if(command==='key_previews'){
          window.__previewCalls++;
          window.__lastPreviewFaces=structuredClone(args.faces);
          const text=args.faces.map(face=>face.text).join(''),frameKey=`${window.__previewRevision}/${JSON.stringify(args.faces)}`;
          if(!frames.has(frameKey)){
            const canvas=document.createElement('canvas');canvas.width=2;canvas.height=2;
            const context=canvas.getContext('2d');context.fillStyle=['#267fef','#ef7f26','#26ef7f'][window.__previewRevision%3];context.fillRect(0,0,2,2);
            context.fillStyle=`rgb(${text.length*37%256},90,140)`;context.fillRect(0,0,1,1);
            frames.set(frameKey,Array.from(Uint8Array.from(atob(canvas.toDataURL('image/png').split(',')[1]),char=>char.charCodeAt(0))));
          }
          return Array.from({length:14},(_,index)=>({index,png:frames.get(frameKey)}));
        }
        return 0;
      }};
    });
    await nativePreview.goto(base);
    await nativePreview.waitForFunction(()=>document.querySelectorAll('img.rendered-key').length===14 && [...document.querySelectorAll('img.rendered-key')].every(image=>image.complete&&image.naturalWidth===2));
    const firstPreviewCall=await nativePreview.evaluate(()=>{window.__firstPreviewImages=[...document.querySelectorAll('img.rendered-key')];window.__firstDecodeCount=window.__decodeCalls;window.__firstPreviewUrlCount=window.__previewUrlCount;return window.__previewCalls;});
    await nativePreview.waitForFunction(calls=>window.__previewCalls>=calls+2,firstPreviewCall);
    assert.equal(await nativePreview.evaluate(()=>[...document.querySelectorAll('img.rendered-key')].every((image,index)=>image===window.__firstPreviewImages[index]) && window.__decodeCalls===window.__firstDecodeCount && window.__previewUrlCount===window.__firstPreviewUrlCount && window.__previewUrls.size===0),true,'unchanged native frames must retain decoded image nodes without new URLs');
    await nativePreview.evaluate(()=>{window.__holdPreviewDecode=true;window.__previewRevision=1;});
    await nativePreview.waitForFunction(()=>window.__pendingPreviewDecodes.length>0);
    assert.equal(await nativePreview.evaluate(()=>[...document.querySelectorAll('img.rendered-key')].every((image,index)=>image===window.__firstPreviewImages[index]&&image.complete&&image.naturalWidth===2)),true,'slow replacement must not blank the displayed frame');
    await nativePreview.evaluate(()=>{window.__holdPreviewDecode=false;window.__pendingPreviewDecodes.splice(0).forEach(resolve=>resolve());});
    await nativePreview.waitForFunction(()=>[...document.querySelectorAll('img.rendered-key')].every((image,index)=>image!==window.__firstPreviewImages[index]&&image.complete&&image.naturalWidth===2));
    // Typing while previewing the other face must switch to the edited face
    // without rebuilding the native key or flashing the HTML fallback label.
    await nativePreview.locator('[data-key="1"]').click();
    await nativePreview.locator('[data-do="preview-flip"]').first().click();
    await nativePreview.waitForFunction(()=>document.querySelectorAll('img.rendered-key').length===14 && [...document.querySelectorAll('img.rendered-key')].every(image=>image.complete&&image.naturalWidth===2));
    await nativePreview.evaluate(()=>{window.__typingKey=document.querySelector('[data-key="1"]');window.__typingImage=window.__typingKey.firstChild;window.__holdPreviewDecode=true;});
    await nativePreview.locator('#face-text').fill('First');
    await nativePreview.locator('#face-text').fill('Latest preview label');
    await nativePreview.waitForFunction(()=>window.__pendingPreviewDecodes.length>0);
    assert.equal(await nativePreview.evaluate(()=>document.querySelector('[data-key="1"]')===window.__typingKey && window.__typingKey.firstChild===window.__typingImage && window.__typingImage.complete&&window.__typingImage.naturalWidth===2),true,'typing must retain the decoded native key rather than flash HTML');
    assert.equal(await nativePreview.locator('.workspace-toolbar .hint').textContent(),'Previewing Face A');
    assert.equal(await nativePreview.evaluate(()=>window.__lastSavedSnapshot.profiles[0].pages[0].keys[1].a.text),'Latest preview label');
    assert.equal(await nativePreview.evaluate(()=>window.__lastPreviewFaces[1].text),'Latest preview label');
    await nativePreview.evaluate(()=>{window.__holdPreviewDecode=false;window.__pendingPreviewDecodes.splice(0).forEach(resolve=>resolve());});
    await nativePreview.waitForFunction(()=>document.querySelector('[data-key="1"] img')!==window.__typingImage);
    assert.equal(await nativePreview.evaluate(()=>{const canvas=document.createElement('canvas');canvas.width=2;canvas.height=2;const context=canvas.getContext('2d');context.drawImage(document.querySelector('[data-key="1"] img'),0,0);return context.getImageData(0,0,1,1).data[0];}),'Latest preview label'.length*37%256,'only the latest typed label frame may replace the old image');
    await nativePreview.locator('[data-key="6"]').focus();
    await nativePreview.keyboard.press('Enter');
    assert.equal(await nativePreview.locator('[data-key="6"]').evaluate(el=>el===document.activeElement),true,'native fixture key activation retains focus');
    await nativePreview.locator('#face-text').fill('Native undo display');
    await nativePreview.locator('[data-do="undo"]').focus();
    await nativePreview.keyboard.press('Enter');
    assert.equal(await nativePreview.locator('.editor-head h2').textContent(),'Key 7','native stable IDs preserve the selected display on undo');
    assert.equal(await nativePreview.evaluate(()=>window.__lastSavedSnapshot.currentProfile==='p'&&window.__lastSavedSnapshot.currentPage==='pg'&&window.__lastSavedSnapshot.profiles[0].pages[0].keys[6].a.text===''),true);
    // Configuring cooler actions saves only typed inert data. Reading choices
    // never selects a theme or enables a device.
    await nativePreview.locator('#action-kind').selectOption('cooler');
    await nativePreview.waitForFunction(()=>window.__coolerChoiceReads===1);
    assert.deepEqual(await nativePreview.evaluate(()=>JSON.parse(window.__lastSavedSnapshot.profiles[0].pages[0].keys[6].value)),{op:'theme_next'});
    await nativePreview.locator('#cooler-operation').selectOption('theme_select');
    assert.equal(await nativePreview.locator('#cooler-theme').inputValue(),'');
    assert.deepEqual(await nativePreview.evaluate(()=>JSON.parse(window.__lastSavedSnapshot.profiles[0].pages[0].keys[6].value)),{op:'theme_next'},'theme selection requires an explicit choice');
    await nativePreview.locator('#cooler-theme').selectOption('theme-beta');
    assert.deepEqual(await nativePreview.evaluate(()=>JSON.parse(window.__lastSavedSnapshot.profiles[0].pages[0].keys[6].value)),{op:'theme_select',theme_id:'theme-beta'});
    await nativePreview.evaluate(()=>{window.__coolerThemes=[];});
    await nativePreview.locator('[data-do="refresh-cooler-controls"]').click();
    await nativePreview.waitForFunction(()=>window.__coolerChoiceReads===2);
    assert.equal(await nativePreview.locator('#cooler-theme').inputValue(),'theme-beta');
    assert.match(await nativePreview.locator('#cooler-theme option:checked').textContent(),/Unavailable theme/);
    await nativePreview.evaluate(()=>{window.__coolerChoicesFail=true;});
    await nativePreview.locator('[data-do="refresh-cooler-controls"]').click();
    await nativePreview.waitForFunction(()=>document.querySelector('#cooler-action-status')?.textContent.includes('could not be read'));
    assert.equal(await nativePreview.locator('#cooler-theme').inputValue(),'theme-beta');
    await nativePreview.locator('#cooler-operation').selectOption('live');
    for(const mode of ['on','off','toggle']){
      await nativePreview.locator('#cooler-live').selectOption(mode);
      assert.deepEqual(await nativePreview.evaluate(()=>JSON.parse(window.__lastSavedSnapshot.profiles[0].pages[0].keys[6].value)),{op:'live',mode});
    }
    await nativePreview.locator('#cooler-operation').selectOption('brightness_step');
    await nativePreview.locator('#cooler-number').fill('-20');await nativePreview.locator('#cooler-number').dispatchEvent('change');
    assert.deepEqual(await nativePreview.evaluate(()=>JSON.parse(window.__lastSavedSnapshot.profiles[0].pages[0].keys[6].value)),{op:'brightness_step',delta:-20});
    await nativePreview.locator('#cooler-number').fill('0');await nativePreview.locator('#cooler-number').dispatchEvent('change');
    assert.deepEqual(await nativePreview.evaluate(()=>JSON.parse(window.__lastSavedSnapshot.profiles[0].pages[0].keys[6].value)),{op:'brightness_step',delta:-20},'invalid zero step never saves');
    await nativePreview.locator('#cooler-operation').selectOption('brightness_set');
    await nativePreview.locator('#cooler-number').fill('100');await nativePreview.locator('#cooler-number').dispatchEvent('change');
    assert.deepEqual(await nativePreview.evaluate(()=>JSON.parse(window.__lastSavedSnapshot.profiles[0].pages[0].keys[6].value)),{op:'brightness_set',value:100});
    await nativePreview.locator('#cooler-operation').selectOption('theme_previous');
    assert.deepEqual(await nativePreview.evaluate(()=>JSON.parse(window.__lastSavedSnapshot.profiles[0].pages[0].keys[6].value)),{op:'theme_previous'});
    await nativePreview.evaluate(()=>{window.__oldPreviewKey=document.querySelector('[data-key="0"]');window.__oldPreviewImage=window.__oldPreviewKey.firstChild;window.__holdPreviewDecode=true;window.__previewRevision=2;});
    await nativePreview.waitForFunction(()=>window.__pendingPreviewDecodes.length>0);
    await nativePreview.locator('[data-page="home"]').click();
    await nativePreview.evaluate(()=>{window.__holdPreviewDecode=false;window.__pendingPreviewDecodes.splice(0).forEach(resolve=>resolve());});
    assert.equal(await nativePreview.locator('[data-key]').count(),0);
    assert.equal(await nativePreview.evaluate(()=>window.__oldPreviewKey.firstChild===window.__oldPreviewImage),true,'late decoded frames must not update detached keys after navigation');
    assert.deepEqual(nativeErrors,[]);
    await nativePreview.close();
    // Delivered physical pages are observed without saving them back to the
    // device. Delayed writes/imports and old polls must not discard editor work.
    const followPage=await browser.newPage();
    const followErrors=[];followPage.on('pageerror',error=>followErrors.push(error.message));
    await followPage.addInitScript(()=>{
      const keys=label=>Array.from({length:14},()=>({a:{kind:'text',text:label},b:{kind:'text',text:''},action:'none',value:''}));
      window.__profileSnapshot={version:1,currentProfile:'p',currentPage:'main',profiles:[{id:'p',name:'Fixture',pages:[{id:'main',name:'Main',keys:keys('Main')},{id:'test',name:'Test',keys:keys('Test')},{id:'other',name:'Other',keys:keys('Other')}]}]};
      window.__dock={state:'connected',enabled:true,fake:true,message:'Fixture connected',currentProfile:'p',currentPage:'main'};
      window.__followPreference=true;window.__appPage='dock';window.__saveCalls=0;window.__settingsPatches=[];
      window.__holdSaves=false;window.__failSaves=false;window.__saveWaiters=[];window.__holdAssets=false;window.__assetWaiters=[];
      window.__holdStatus=false;window.__statusWaiters=[];window.__statusCalls=0;window.__statusActive=0;window.__maxStatusActive=0;
      window.__holdPreferences=false;window.__preferenceWaiters=[];window.__callbacks=[];window.__quitReadyCalls=0;
      const appState=()=>({page:window.__appPage,followPhysicalPage:window.__followPreference,appVersion:'fixture',autostart:false,dock:structuredClone(window.__dock)});
      const canvas=document.createElement('canvas');canvas.width=2;canvas.height=2;canvas.getContext('2d').fillRect(0,0,2,2);
      const png=Array.from(Uint8Array.from(atob(canvas.toDataURL('image/png').split(',')[1]),c=>c.charCodeAt(0)));
      window.__TAURI_INTERNALS__={transformCallback:callback=>window.__callbacks.push(callback)-1,invoke:async(command,args)=>{
        if(command==='plugin:event|listen'){if(args.event==='uc:prepare-quit')window.__quitHandler=window.__callbacks[args.handler];return 0;}
        if(command==='editor_quit_ready'){window.__quitReadyCalls++;return 0;}
        if(command==='app_state')return appState();
        if(command==='settings_update'){
          window.__settingsPatches.push(structuredClone(args.patch));
          if(window.__holdPreferences&&typeof args.patch.followPhysicalPage==='boolean')await new Promise(resolve=>window.__preferenceWaiters.push(resolve));
          if(typeof args.patch.followPhysicalPage==='boolean')window.__followPreference=args.patch.followPhysicalPage;
          if(args.patch.page)window.__appPage=args.patch.page;
          return appState();
        }
        if(command==='profiles_state')return {state:structuredClone(window.__profileSnapshot)};
        if(command==='profiles_save'){
          window.__saveCalls++;
          if(window.__holdSaves)await new Promise(resolve=>window.__saveWaiters.push(resolve));
          if(window.__failSaves)throw new Error('Fixture save failed');
          window.__profileSnapshot=structuredClone(args.snapshot);return 0;
        }
        if(command==='asset_import'){
          if(window.__holdAssets)await new Promise(resolve=>window.__assetWaiters.push(resolve));
          return 'asset-fixture';
        }
        if(command==='dock_state'){
          window.__statusCalls++;window.__statusActive++;window.__maxStatusActive=Math.max(window.__maxStatusActive,window.__statusActive);
          const snapshot=structuredClone(window.__dock);
          try{if(window.__holdStatus)await new Promise(resolve=>window.__statusWaiters.push(resolve));return snapshot;}
          finally{window.__statusActive--;}
        }
        if(command==='key_previews')return Array.from({length:14},(_,index)=>({index,png}));
        return 0;
      }};
    });
    await followPage.goto(base);
    await followPage.waitForFunction(()=>document.getElementById('page-name')?.value==='Main'&&document.querySelectorAll('img.rendered-key').length===14);
    const nextFollowPoll=async()=>{const count=await followPage.evaluate(()=>window.__statusCalls);await followPage.waitForFunction(count=>window.__statusCalls>count,count);};
    await followPage.locator('#face-text').focus();
    await followPage.evaluate(()=>window.__dock.currentPage='test');
    await followPage.waitForFunction(()=>document.getElementById('page-name')?.value==='Test');
    assert.equal(await followPage.locator('[data-dock-page="1"]').getAttribute('aria-pressed'),'true');
    await followPage.evaluate(()=>window.__dock.currentPage='main');
    await followPage.waitForFunction(()=>document.getElementById('page-name')?.value==='Main');
    assert.equal(await followPage.evaluate(()=>window.__saveCalls),0,'physical page following must not save navigation back');
    await followPage.locator('[data-dock-page="1"]').click();
    await nextFollowPoll();
    assert.equal(await followPage.locator('#page-name').inputValue(),'Test','unchanged physical state must not undo manual editor selection');
    await followPage.evaluate(()=>window.__dock.currentPage='other');
    await followPage.waitForFunction(()=>document.getElementById('page-name')?.value==='Other');
    const manualSaveCount=await followPage.evaluate(()=>window.__saveCalls);
    await followPage.locator('#follow-physical-page').uncheck();
    await followPage.evaluate(()=>window.__dock.currentPage='test');
    await nextFollowPoll();
    assert.equal(await followPage.locator('#page-name').inputValue(),'Other');
    await followPage.locator('#follow-physical-page').check();
    await followPage.waitForFunction(()=>document.getElementById('page-name')?.value==='Test');
    assert.equal(await followPage.evaluate(()=>window.__followPreference),true);
    assert.equal(await followPage.evaluate(()=>window.__saveCalls),manualSaveCount);
    await followPage.evaluate(()=>window.__dock.currentPage='main');
    await followPage.waitForFunction(()=>document.getElementById('page-name')?.value==='Main');
    await followPage.evaluate(()=>window.__holdSaves=true);
    await followPage.locator('#face-text').fill('Pending label');
    await followPage.evaluate(()=>window.__dock.currentPage='test');
    await nextFollowPoll();
    assert.equal(await followPage.locator('#page-name').inputValue(),'Main','a pending text save defers physical following');
    assert.equal(await followPage.locator('#face-text').inputValue(),'Pending label');
    await followPage.evaluate(()=>{window.__holdSaves=false;window.__saveWaiters.splice(0).forEach(resolve=>resolve());});
    await followPage.waitForFunction(()=>window.__profileSnapshot.profiles[0].pages[0].keys[0].a.text==='Pending label');
    await nextFollowPoll();
    assert.equal(await followPage.locator('#page-name').inputValue(),'Main','saving must not end an active modified text gesture');
    await followPage.locator('#face-text').fill('Pending label continues');
    await followPage.waitForFunction(()=>window.__profileSnapshot.profiles[0].pages[0].keys[0].a.text==='Pending label continues');
    await followPage.locator('.page-heading h1').click();
    await followPage.waitForFunction(()=>document.getElementById('page-name')?.value==='Test');
    assert.equal(await followPage.evaluate(()=>window.__profileSnapshot.profiles[0].pages[0].keys[0].a.text),'Pending label continues');
    await followPage.locator('#face-kind').selectOption('picture');
    await followPage.waitForFunction(()=>window.__profileSnapshot.profiles[0].pages[1].keys[0].a.kind==='picture');
    await followPage.evaluate(()=>window.__holdAssets=true);
    await followPage.locator('#face-picture').setInputFiles({name:'fixture.png',mimeType:'image/png',buffer:Buffer.from(picture,'base64')});
    await followPage.waitForFunction(()=>window.__assetWaiters.length===1);
    await followPage.evaluate(()=>window.__dock.currentPage='main');
    await nextFollowPoll();
    assert.equal(await followPage.locator('#page-name').inputValue(),'Test','a pending picture import defers physical following');
    await followPage.evaluate(()=>{window.__holdAssets=false;window.__assetWaiters.splice(0).forEach(resolve=>resolve());});
    await followPage.waitForFunction(()=>document.getElementById('page-name')?.value==='Main');
    assert.equal(await followPage.evaluate(()=>window.__profileSnapshot.profiles[0].pages[1].keys[0].a.asset),'asset-fixture');
    await followPage.evaluate(()=>window.__failSaves=true);
    await followPage.locator('#face-text').fill('Retry retained');
    await followPage.waitForFunction(()=>document.getElementById('storage-status').textContent.includes('could not be saved'));
    await followPage.evaluate(()=>window.__dock.currentPage='test');await nextFollowPoll();
    assert.equal(await followPage.locator('#page-name').inputValue(),'Main','failed saves retain the editing page');
    await followPage.evaluate(()=>window.__failSaves=false);
    await followPage.locator('[data-do="retry-save"]').click();
    await followPage.waitForFunction(()=>document.getElementById('page-name')?.value==='Test');
    assert.equal(await followPage.evaluate(()=>window.__profileSnapshot.profiles[0].pages[0].keys[0].a.text),'Retry retained');
    await followPage.evaluate(()=>window.__dock.currentPage='main');
    await followPage.waitForFunction(()=>document.getElementById('page-name')?.value==='Main');
    await followPage.evaluate(()=>{window.__dock.state='disconnected';window.__dock.currentPage='test';});
    await nextFollowPoll();
    assert.equal(await followPage.locator('#page-name').inputValue(),'Main','disconnected state cannot change editor selection');
    await followPage.evaluate(()=>{window.__dock.state='connected';window.__dock.enabled=true;});
    await followPage.waitForFunction(()=>document.getElementById('page-name')?.value==='Test');
    const beforeSidebar=await followPage.evaluate(()=>window.__saveCalls);
    await followPage.locator('[data-page="home"]').click();
    await followPage.evaluate(()=>window.__dock.currentPage='other');await nextFollowPoll();
    assert.equal(await followPage.locator('[data-page="home"]').getAttribute('aria-current'),'page','status must not jump from Home into the dock');
    await followPage.locator('[data-page="dock"]').click();
    await followPage.waitForFunction(()=>document.getElementById('page-name')?.value==='Other');
    assert.equal(await followPage.evaluate(()=>window.__saveCalls),beforeSidebar,'following/sidebar selection must not write profiles');
    await followPage.evaluate(()=>{window.__holdStatus=true;window.__dock.currentPage='test';});
    await followPage.waitForFunction(()=>window.__statusWaiters.length===1);
    await followPage.waitForTimeout(1200);
    assert.equal(await followPage.evaluate(()=>window.__statusWaiters.length),1,'status polling must remain single-flight');
    await followPage.locator('[data-dock-page="0"]').click();
    await followPage.evaluate(()=>{window.__dock.currentPage='other';window.__holdStatus=false;window.__statusWaiters.splice(0).forEach(resolve=>resolve());});
    await nextFollowPoll();
    assert.equal(await followPage.locator('#page-name').inputValue(),'Main','a poll begun before manual selection must not pull the editor back');
    assert.equal(await followPage.evaluate(()=>window.__maxStatusActive),1);
    await followPage.evaluate(()=>window.__dock.currentPage='test');
    await followPage.waitForFunction(()=>document.getElementById('page-name')?.value==='Test');
    await followPage.evaluate(()=>window.__holdPreferences=true);
    await followPage.locator('#follow-physical-page').uncheck();
    await followPage.waitForFunction(()=>window.__preferenceWaiters.length===1);
    await followPage.evaluate(()=>{window.__quitPromise=window.__quitHandler();});
    assert.equal(await followPage.evaluate(()=>window.__quitReadyCalls),0,'Quit waits for the page-follow preference write');
    await followPage.evaluate(()=>{window.__holdPreferences=false;window.__preferenceWaiters.splice(0).forEach(resolve=>resolve());});
    await followPage.waitForFunction(()=>window.__quitReadyCalls===1);
    assert.equal(await followPage.evaluate(()=>window.__followPreference),false);
    assert.deepEqual(followErrors,[]);
    await followPage.close();
    console.log('PASS: enabled-device workspace; settings-managed devices; 14 displays; A/B flip; persistence; undo and display context; keyboard focus/tab order at 3 sizes; no outside-focus theft or removed-control restoration; picture reload; 16 usage choices; literal launch arguments; escaped text; profiles/pages; stable IDs and targets; serialized saves/coalescing/retry; corrupt draft recovery; 3 responsive sizes; native preview retention/decoded swap/typing stability; physical page follow/back/optout/manual/pending-edit/import/stale-poll guards; no page errors.');
    console.log(output);
  } finally {if(browser) await browser.close(); await new Promise(r=>server.close(r));}
})().catch(e=>{console.error(e);process.exitCode=1;});
