// Headless fixture only: the Vibe Key quick menu dial (public/quick-menu.html, 0.34.0/0.35.0). Never a real
// window: a headless browser off the desktop. Screenshots go to UC_QM_SHOTS (default: %TEMP%/uc-quick-menu-preview).
const fs=require('node:fs'),path=require('node:path'),http=require('node:http'),os=require('node:os'),assert=require('node:assert/strict');
const {chromium}=require(process.env.UC_PLAYWRIGHT_MODULE||'playwright');
const root=path.resolve(__dirname,'../public');
const shots=process.env.UC_QM_SHOTS||path.join(os.tmpdir(),'uc-quick-menu-preview');
const server=http.createServer((req,res)=>{
  const file=path.resolve(root,'.'+new URL(req.url,'http://localhost').pathname);
  if(!file.startsWith(root+path.sep)){res.writeHead(403);return res.end();}
  fs.readFile(file,(error,data)=>{res.writeHead(error?404:200,{'Content-Type':file.endsWith('.js')?'text/javascript':file.endsWith('.css')?'text/css':'text/html'});res.end(error?'Not found':data);});
});

const top=[{label:'Close',type:'close'},{label:'Start dictation',type:'action'},{label:'Claude sessions',type:'submenu',detail:''},
  {label:'Switch profile',type:'submenu'},{label:'Send to',type:'submenu'},{label:'Volume up',type:'action'},{label:'Dictate & send',type:'action'}];
const groups=[{label:'Back',type:'back'},{label:'Unified Control',type:'submenu',detail:'4 sessions'},{label:'Project Nocturne',type:'submenu',detail:'2 sessions'},
  {label:'Other sessions',type:'submenu',detail:'7 sessions'}];
const sessions=[{label:'Back',type:'back'},{label:'Open: Vibe Key AU05 connection setup',type:'claudeSession',detail:'Unified Control',working:true},
  {label:'Quick menu dial with rings and pie segments',type:'claudeSession',detail:'Unified Control'},{label:'Wake reads',type:'claudeSession',detail:'Unified Control'},
  {label:'Cooler freeze diagnostics',type:'claudeSession',detail:'CoolerScreen',working:true}];
const many=[{label:'Back',type:'back'},...Array.from({length:31},(_,i)=>({label:`Session ${i+1} ${['cooler freeze','dock layout','vault sync','wake reads','dial labels'][i%5]}`,type:'claudeSession',detail:i%2?'vault':'CoolerScreen',working:i===19}))];
const three=[{label:'Back',type:'back'},{label:'Claude',type:'profile',current:true},{label:'ChatGPT',type:'profile'},{label:'Media',type:'profile'}];
const long=[{label:'Back',type:'back'},
  {label:'Unified Control: add the quick menu dial with curved labels and an end stop for long lists',type:'claudeSession',detail:'Unified Control'},
  {label:'Unified Control: fix the Vibe Key settings reads after a stalled wake from switched off',type:'claudeSession',detail:'Unified Control',working:true},
  {label:'Unified Control: investigate CoolerScreen disconnects during sleep and resume',type:'claudeSession',detail:'Unified Control'},
  {label:'Unified Control: Supercalifragilisticexpialidocious',type:'claudeSession',detail:'Unified Control'},
  {label:'Unified Control: release notes',type:'claudeSession',detail:'Unified Control'}];
const base={open:true,profile:'Claude',target:'Claude',note:null,closeAfterS:8,hints:'Turn: move · K2 select · K3 back · Knob press select',bump:{dir:0,n:0}};
const lvl=rows=>rows.map(({label,type})=>({label,type}));
const views={
  top:{...base,title:['Quick menu'],depth:0,levels:[],rows:top,highlight:2},
  sub:{...base,title:['Quick menu','Claude sessions'],depth:1,levels:[{rows:lvl(top),chosen:2}],rows:groups,highlight:1},
  deep:{...base,title:['Quick menu','Claude sessions','Unified Control'],depth:2,levels:[{rows:lvl(top),chosen:2},{rows:lvl(groups),chosen:1}],rows:sessions,highlight:2},
  many:{...base,title:['Quick menu','Other sessions'],depth:1,levels:[{rows:lvl(top),chosen:2}],rows:many,highlight:20},
  three:{...base,title:['Quick menu','Switch profile'],depth:1,levels:[{rows:lvl(top),chosen:3}],rows:three,highlight:1},
  long:{...base,title:['Quick menu','Claude sessions','Unified Control'],depth:2,levels:[{rows:lvl(top),chosen:2},{rows:lvl(groups),chosen:1}],rows:long,highlight:2},
};

(async()=>{let browser;try{
  await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
  browser=await chromium.launch({headless:true});
  const page=await browser.newPage({viewport:{width:568,height:664}}),errors=[];
  page.on('pageerror',e=>errors.push(e.message));
  await page.addInitScript(()=>{
    window.__inputs=[];
    window.__TAURI_INTERNALS__={transformCallback:()=>1,invoke:async(command,args)=>{window.__inputs.push(command==='au05_menu'?args.input:{command});return null;}};
  });
  await page.goto(`http://127.0.0.1:${server.address().port}/quick-menu.html`);
  await page.waitForFunction(()=>document.getElementById('qm-dial'));
  await page.evaluate(()=>document.fonts.ready);

  // Geometry.
  const g=await page.evaluate(async()=>{
    const d=await import('/js/quick-menu-dial.js');
    return {
      STEP:d.STEP,STOP:d.STOP,STOP_ANGLE:d.STOP_ANGLE,
      angles8:d.slotAngles(8).map(a=>[a.start,a.mid,a.end]),
      off:[[32,1,0],[32,7,0],[32,8,0],[32,20,0],[32,31,0],[32,15,13],[32,14,13],[32,0,13],[8,7,0]].map(([c,h,p])=>d.windowOffset(c,h,p)),
      rows20:d.slotRows(32,13),rows4:d.slotRows(4,0),
      edges:[d.edges(4,0),d.edges(32,0),d.edges(32,13),d.edges(32,24)],
      steps:[d.step(32,0,-1),d.step(32,31,1),d.step(32,5,1)],
      radii:[1,2,3,4].map(n=>d.ringRadii(n)),
      sizes:[1,2,3].map(n=>d.dialSize(n)),
      scale:[d.fitScale(3,568,664),d.fitScale(3,500,500)],
      flips:[0,45,135,180,225,315].map(d.flipped),
      short:[d.shortForm('Unified Control: fix the Vibe Key reads',['Unified Control: dial','Unified Control: notes']),d.shortForm('Session 12 dock layout',['Session 1 vault','Session 2 wake'])],
    };
  });
  assert.equal(g.STEP,43,'8 segments of 43° and a 16° stop gap');
  assert.deepEqual(g.angles8[0],[-21.5,0,21.5],'slot 0 is centred at 12 o\'clock');
  assert.deepEqual(g.angles8[1],[21.5,43,64.5],'the next slot is clockwise');
  assert.ok(g.angles8[7][2]<=360+g.angles8[0][0]-16,'a gap is kept between the last slot and row 0, even when full');
  assert.ok(g.STOP_ANGLE<g.angles8[0][0]&&g.STOP_ANGLE>g.angles8[7][2]-360,'the stop is on row 0\'s anticlockwise side');
  assert.deepEqual(g.off,[0,0,1,13,24,13,13,13,0],'the window moves only at its edges, never past the ends');
  assert.deepEqual(g.rows20,[0,14,15,16,17,18,19,20],'Back stays on top, 7 rows move');
  assert.deepEqual(g.rows4,[0,1,2,3]);
  assert.deepEqual(g.edges,[{stop:true,moreBefore:false,moreAfter:false},{stop:false,moreBefore:false,moreAfter:true},
    {stop:false,moreBefore:true,moreAfter:true},{stop:true,moreBefore:true,moreAfter:false}],'the stop only at the true end; "more" otherwise');
  assert.deepEqual(g.steps,[{to:0,bump:true},{to:31,bump:true},{to:6,bump:false}],'no wrap-around; a bump at the ends');
  const outer=g.radii.slice(0,3).map(r=>r.at(-1)[1]);
  assert.deepEqual(outer,[170,210,260],'about 1.4 × the 0.34.0 dial');
  for(const set of g.radii)for(let i=1;i<set.length;i++)assert.ok(set[i][0]>set[i-1][1],'rings never overlap');
  for(const set of g.radii)assert.ok(set.at(-1)[1]-set.at(-1)[0]>=86,'the active ring stays thick');
  assert.deepEqual(g.radii[3],g.radii[2],'at most 3 rings');
  assert.deepEqual(g.sizes,[{width:388,height:484},{width:468,height:564},{width:568,height:664}],'the same sizes as quick_menu.rs dial_size');
  assert.equal(g.scale[0],1);assert.ok(g.scale[1]<0.8,'scaled down in a small window');
  assert.deepEqual(g.flips,[false,false,true,true,true,false],'bottom-half labels run the other way (never upside down)');
  assert.deepEqual(g.short[0].kept.map(i=>g.short[0].words[i]),['Vibe','Key','reads'],'the shared project prefix and filler words go');
  assert.deepEqual(g.short[1].kept.map(i=>g.short[1].words[i]),['12','dock','layout'],'"Session" shared by every sibling goes');

  const show=async(name,{fresh=false,extra={}}={})=>{
    await page.evaluate(([v,fresh])=>import('/js/quick-menu.js').then(m=>{if(fresh)m.render({open:false});m.render(v);}),[{...views[name],...extra},fresh]);
  };
  const ring=level=>page.locator(`.qm-ring[data-level="${level}"]`);
  // Every label inside its segment: each line no longer than its arc; no ellipsis anywhere; at least 11 px.
  const labelsFit=async()=>page.evaluate(()=>[...document.querySelectorAll('.qm-ring[data-active="true"] text')].map(t=>{
    const tp=t.querySelector('textPath'),p=document.querySelector(tp.getAttribute('href'));
    return {text:t.textContent,len:t.getComputedTextLength(),room:p.getTotalLength(),size:parseFloat(t.style.fontSize),short:t.dataset.short==='true'};
  }));
  const checkFit=async name=>{
    const ls=await labelsFit();
    assert.ok(ls.length,`${name}: labels drawn`);
    for(const l of ls){
      assert.ok(l.len<=l.room-8,`${name}: "${l.text}" fits its arc (${l.len.toFixed(0)} of ${l.room.toFixed(0)})`);
      assert.ok(!/…|\.\.\./.test(l.text),`${name}: no ellipsis in "${l.text}"`);
      assert.ok(l.size>=11,`${name}: readable size`);
    }
    return ls;
  };

  fs.mkdirSync(shots,{recursive:true});
  await page.addStyleTag({content:'html{background:linear-gradient(135deg,#2c3e55,#5a6b80)}'});
  const shoot=async(name,rings)=>{
    const size=await page.evaluate(n=>import('/js/quick-menu-dial.js').then(d=>d.dialSize(n)),rings);
    await page.setViewportSize(size);
    await page.waitForTimeout(50);
    await page.screenshot({path:path.join(shots,`quick-menu-dial-${name}.png`)});
    await page.setViewportSize({width:568,height:664});
  };

  // The top level: 7 segments, Close on top, the stop on its left, the highlight filled and pushed out, dark text.
  await show('top',{fresh:true});
  assert.equal(await ring(0).locator('.qm-seg').count(),7);
  assert.equal(await ring(0).locator('.qm-seg[data-row="0"]').getAttribute('data-kind'),'nav','row 0 is Close');
  assert.equal(await page.locator('.qm-stop').count(),1,'the end stop');
  assert.equal(await page.locator('.qm-slot[data-hl="true"]').getAttribute('data-row'),'2');
  assert.notEqual(await page.locator('.qm-slot[data-hl="true"]').getAttribute('transform'),'translate(0.00 0.00)','pushed out');
  const ink=await page.evaluate(()=>getComputedStyle(document.querySelector('.qm-label[data-hl="true"]')).fill);
  assert.match(ink,/rgb\(4, 18, 31\)/,`dark text on the accent (${ink})`);
  const topFit=await checkFit('top');
  assert.ok(topFit.every(l=>!l.short),'short names stay whole');
  assert.match(await page.locator('#qm-caption').textContent(),/Claude sessions/);
  assert.match(await page.locator('#qm-foot').textContent(),/K2 select · K3 back/);
  const handle=page.locator('#qm-handle');
  assert.equal(await handle.getAttribute('data-tauri-drag-region'),'','the centre is the drag handle');
  assert.equal(await handle.evaluate(e=>getComputedStyle(e).cursor),'move');
  assert.notEqual(await handle.evaluate(e=>getComputedStyle(e).backgroundColor),'rgba(0, 0, 0, 0)','a real fill: every point can be grabbed');
  await shoot('top',1);

  // A turn past an end: the highlight bumps (once per bump, not on opening).
  const bumps=()=>page.evaluate(()=>Number(document.getElementById('qm-dial').dataset.bumps||0));
  await show('top',{fresh:true,extra:{highlight:0,bump:{dir:-1,n:4}}});
  assert.equal(await bumps(),0,'no bump on opening');
  await show('top',{extra:{highlight:0,bump:{dir:-1,n:5}}});
  assert.equal(await bumps(),1,'a bump at Close');
  await show('top',{extra:{highlight:1,bump:{dir:-1,n:5}}});
  assert.equal(await bumps(),1,'a normal turn: no bump');

  // Mouse: a segment click chooses it; the wheel moves.
  await show('top',{fresh:true});
  await page.evaluate(()=>{window.__inputs=[];});
  await page.locator('.qm-seg[data-row="3"]').click({force:true});
  await page.mouse.move(284,70);await page.mouse.wheel(0,120);
  let inputs=await page.evaluate(()=>window.__inputs);
  assert.deepEqual(inputs,[{op:'pick',index:3},{op:'next'}],JSON.stringify(inputs));

  // Dragging (the 0.34.0 bug): a blur while the handle is held, while the window moves and for 300 ms after
  // is ignored; the idle close is held; afterwards a blur closes again.
  await page.evaluate(()=>{window.__inputs=[];});
  const box=await handle.boundingBox();
  await page.mouse.move(box.x+box.width/2,box.y+box.height/2);
  await page.mouse.down();
  const q=await page.evaluate(()=>import('/js/quick-menu.js').then(m=>{
    const r=[m.blurred()];m.moved();r.push(m.blurred());m.moved();return r;}));
  assert.deepEqual(q,[false,false],'no close during the drag');
  await page.mouse.up();
  assert.equal(await page.evaluate(()=>import('/js/quick-menu.js').then(m=>m.blurred())),false,'nor right after it');
  await page.waitForTimeout(800);
  inputs=await page.evaluate(()=>window.__inputs.map(i=>i.op||i.command));
  assert.ok(!inputs.includes('close'),`no close: ${inputs}`);
  assert.ok(inputs.filter(i=>i==='hold').length>=2,`the idle close is held: ${inputs}`);
  assert.equal(await page.evaluate(()=>import('/js/quick-menu.js').then(m=>m.blurred())),true,'a later blur closes');
  // A plain click on the handle: no close (its blur is ignored too).
  await page.evaluate(()=>{window.__inputs=[];});
  await handle.click();
  await page.evaluate(()=>import('/js/quick-menu.js').then(m=>m.blurred()));
  assert.ok(!(await page.evaluate(()=>window.__inputs.map(i=>i.op))).includes('close'),'a click on the handle does not close the menu');
  await page.waitForTimeout(2200);

  // Into Claude sessions: the parent turns its chosen segment to the top and stays dimmed inside the new ring.
  await show('top',{fresh:true});
  await show('sub');
  await page.waitForTimeout(350);
  assert.equal(await page.locator('.qm-ring').count(),2);
  assert.match(await ring(0).getAttribute('transform'),/rotate\(-86\.00\)/,'the chosen segment (slot 2) is at 12 o\'clock');
  assert.equal(await ring(0).getAttribute('data-active'),'false');
  assert.equal(await page.locator('#qm-labels').evaluate(e=>e.style.opacity),'1');
  assert.match(await page.locator('.qm-plabel').first().textContent(),/Claude/);
  await checkFit('sub');
  await shoot('sub',2);
  // Deeper: three rings, the outer one the largest.
  await show('deep');
  await page.waitForTimeout(350);
  assert.equal(await page.locator('.qm-ring').count(),3);
  const sizes=await page.evaluate(()=>[...document.querySelectorAll('.qm-ring')].map(r=>r.getBBox().width));
  assert.ok(sizes[0]<sizes[1]&&sizes[1]<sizes[2],`each deeper ring is bigger: ${sizes}`);
  await checkFit('deep');
  assert.match(await page.locator('#qm-caption').textContent(),/Quick menu dial with rings and pie segments.*Unified Control/,'the full name and project in the caption');
  await shoot('deep',3);
  // Back reverses it.
  await show('sub');
  await page.waitForTimeout(350);
  assert.equal(await page.locator('.qm-ring').count(),2,'the outer ring is gone');

  // Reopened where it was left: drawn in place, no animation.
  await show('deep',{fresh:true});
  assert.equal(await page.locator('.qm-ring').count(),3);
  assert.equal(await ring(0).getAttribute('transform'),'rotate(-86.00)','in place at once');

  // 31 entries: 8 segments, a moving window, "more" fades, no stop (not at the end), the position.
  await show('many',{fresh:true});
  assert.equal(await ring(1).locator('.qm-seg').count(),8);
  assert.equal(await ring(1).locator('.qm-seg[data-row="0"]').count(),1,'Back stays');
  assert.equal(await ring(1).locator('.qm-slot[data-row="20"]').getAttribute('data-hl'),'true');
  assert.equal(await page.locator('.qm-stop').count(),0,'no stop inside the list');
  assert.match(await page.locator('.qm-pos').textContent(),/^20 \/ 31$/);
  // Back a few: the window stays; "more" fades at both edges.
  await show('many',{extra:{highlight:16}});
  assert.equal(await ring(1).locator('.qm-slot[data-row="20"]').count(),1,'the window did not move');
  assert.equal(await page.locator('.qm-seg.qm-more').count(),2,'"more" both ways');
  await checkFit('many');
  await shoot('31',2);
  await show('many',{extra:{highlight:31}});
  assert.equal(await ring(1).locator('.qm-slot[data-row="31"]').getAttribute('data-hl'),'true','the window follows to the last entry');
  assert.equal(await page.locator('.qm-stop').count(),1,'the stop at the true end');
  assert.equal(await page.locator('.qm-seg.qm-more').count(),1,'"more" only before');

  // 3 entries: a large stop gap anticlockwise of Back.
  await show('three',{fresh:true});
  assert.equal(await ring(1).locator('.qm-seg').count(),4);
  assert.equal(await page.locator('.qm-stop').count(),1);
  await checkFit('three');
  await shoot('three',2);

  // Long names: whole words on up to 2 lines, auto-fitted, else a deliberate short form; the caption has it all.
  await show('long',{fresh:true});
  const ls=await checkFit('long');
  assert.ok(ls.some(l=>l.short),'a deliberate short form for names that do not fit');
  assert.ok(ls.filter(l=>l.short).every(l=>!/Unified/.test(l.text)),`the shared project prefix is dropped: ${ls.map(l=>l.text).join(' | ')}`);
  assert.match(await page.locator('#qm-caption').textContent(),/fix the Vibe Key settings reads after a stalled wake from switched off/,'the full name in the caption');
  await shoot('long',3);
  console.log('long labels:',ls.map(l=>`${l.text} (${l.size}px${l.short?', short':''})`).join(' | '));

  assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),'no overflow');
  assert.deepEqual(errors,[]);
  console.log(`Quick menu dial checks passed. Screenshots: ${shots}`);
}catch(e){console.error(e);process.exitCode=1;}finally{await browser?.close();server.close();}})();
