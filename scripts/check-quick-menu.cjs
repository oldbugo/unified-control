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
  {label:'Switch profile',type:'submenu'},{label:'Send to',type:'submenu'},{label:'Volume up',type:'action'},{label:'Dictate & send',type:'action'},{label:'Next track',type:'action'}];
const four=top.slice(0,4);
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
  four:{...base,title:['Quick menu'],depth:0,levels:[],rows:four,highlight:1},
  sub:{...base,title:['Quick menu','Claude sessions'],depth:1,levels:[{rows:lvl(top),chosen:2}],rows:groups,highlight:1},
  deep:{...base,title:['Quick menu','Claude sessions','Unified Control'],depth:2,levels:[{rows:lvl(top),chosen:2},{rows:lvl(groups),chosen:1}],rows:sessions,highlight:2},
  many:{...base,title:['Quick menu','Other sessions'],depth:1,levels:[{rows:lvl(top),chosen:2}],rows:many,highlight:20},
  three:{...base,title:['Quick menu','Switch profile'],depth:1,levels:[{rows:lvl(top),chosen:3}],rows:three,highlight:1},
  long:{...base,title:['Quick menu','Claude sessions','Unified Control'],depth:2,levels:[{rows:lvl(top),chosen:2},{rows:lvl(groups),chosen:1}],rows:long,highlight:2},
};

(async()=>{let browser;try{
  await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
  browser=await chromium.launch({headless:true});
  const page=await browser.newPage({viewport:{width:682,height:778}}),errors=[];
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
      angles4:d.slotAngles(4).map(a=>[a.start,a.mid,a.end]),angles8:d.slotAngles(8).map(a=>[a.start,a.mid,a.end]),
      off:[[32,1,0],[32,7,0],[32,8,0],[32,20,0],[32,31,0],[32,15,13],[32,14,13],[32,0,13],[8,7,0]].map(([c,h,p])=>d.windowOffset(c,h,p)),
      rows20:d.slotRows(32,13),rows4:d.slotRows(4,0),
      edges:[d.edges(4,0),d.edges(32,0),d.edges(32,13),d.edges(32,24)],
      steps:[d.step(32,0,-1),d.step(32,31,1),d.step(32,5,1)],
      radii:[1,2,3,4].map(n=>d.ringRadii(n)),
      sizes:[1,2,3].map(n=>d.dialSize(n)),
      scale:[d.fitScale(3,682,778),d.fitScale(3,500,500)],
      tapers:[2,3,4,8].map(n=>d.taper(n)),spiral:[d.spiralAt(4,-45),d.spiralAt(4,315),d.spiralAt(4,45)],gapPx:[60,225].map(r=>r*Math.sin(d.gapAngle(r)*Math.PI/180)),shades:[0,7].map(i=>d.shade(8,i)),
      fits:[d.boxFits(0,-140,60,20,{r0:60,r1:225,a0:-45,a1:45}),d.boxFits(0,-140,300,20,{r0:60,r1:225,a0:-45,a1:45}),d.boxFits(0,-62,20,10,{r0:60,r1:225,a0:-45,a1:45})],
      short:[d.shortForm('Unified Control: fix the Vibe Key reads',['Unified Control: dial','Unified Control: notes']),d.shortForm('Session 12 dock layout',['Session 1 vault','Session 2 wake'])],
    };
  });
  assert.deepEqual(g.angles4,[[-45,0,45],[45,90,135],[135,180,225],[225,270,315]],'4 rows share the whole ring: 90° each, row 0 at 12 o\'clock');
  assert.deepEqual(g.angles8[1],[22.5,45,67.5],'8 rows: 45° each, clockwise');
  assert.equal(g.angles8[7][2],337.5,'no gap: the last segment meets row 0');
  assert.deepEqual(g.off,[0,0,1,13,24,13,13,13,0],'the window moves only at its edges, never past the ends');
  assert.deepEqual(g.rows20,[0,14,15,16,17,18,19,20],'Back stays on top, 7 rows move');
  assert.deepEqual(g.rows4,[0,1,2,3]);
  assert.deepEqual(g.edges,[{moreBefore:false,moreAfter:false},{moreBefore:false,moreAfter:true},{moreBefore:true,moreAfter:true},{moreBefore:true,moreAfter:false}],'"more" where rows are hidden');
  assert.deepEqual(g.steps,[{to:0,bump:true},{to:31,bump:true},{to:6,bump:false}],'no wrap-around; a bump at the ends');
  const outer=g.radii.slice(0,3).map(r=>r.at(-1)[1]);
  assert.deepEqual(outer,[225,254,305],'a bigger dial');
  for(let i=1;i<3;i++)assert.ok(outer[i]>outer[i-1],'each level larger');
  for(const set of g.radii)for(let i=1;i<set.length;i++)assert.ok(set[i][0]>set[i-1][1],'rings never overlap');
  assert.deepEqual(g.radii.slice(0,3).map(set=>set.at(-1)[1]-set.at(-1)[0]),[165,128,141],'about 1.5 × the 0.35.0 band (110, 86, 94)');
  assert.deepEqual(g.radii[3],g.radii[2],'at most 3 rings');
  assert.deepEqual(g.sizes,[{width:522,height:618},{width:580,height:676},{width:682,height:778}],'the same sizes as quick_menu.rs dial_size');
  assert.equal(g.scale[0],1);assert.ok(g.scale[1]<0.8,'scaled down in a small window');
  for(const t of g.tapers){
    assert.ok(t[0]<=12&&t[0]>t[1],'Close/Back reaches furthest out');
    for(let i=1;i<t.length;i++)assert.ok(t[i]<t[i-1],`each segment a smidge smaller: ${t}`);
    const steps=t.slice(1).map((x,i)=>t[i]-x);
    assert.ok(Math.max(...steps)-Math.min(...steps)<1e-9,`one smooth spiral, the same step every slot: ${t}`);
  }
  assert.deepEqual(g.spiral.slice(0,2),[12,-36],'the spiral starts outside the band at Close/Back and ends inside it');
  assert.equal(g.spiral[2],12-48/4,'continuous: a segment edge sits where its neighbour ends');
  for(const w of g.gapPx)assert.ok(Math.abs(w-2.5)<1e-9,'parallel gaps: the same width at the hole and at the rim');
  assert.deepEqual(g.shades,[0,1],'heavier toward the end');
  assert.deepEqual(g.fits,[true,false,false],'a box fits inside a segment, not past its sides or its inner edge');
  assert.deepEqual(g.short[0].kept.map(i=>g.short[0].words[i]),['Vibe','Key','reads'],'the shared project prefix and filler words go');
  assert.deepEqual(g.short[1].kept.map(i=>g.short[1].words[i]),['12','dock','layout'],'"Session" shared by every sibling goes');

  const show=async(name,{fresh=false,extra={}}={})=>{
    await page.evaluate(([v,fresh])=>import('/js/quick-menu.js').then(m=>{if(fresh)m.render({open:false});m.render(v);}),[{...views[name],...extra},fresh]);
  };
  const ring=level=>page.locator(`.qm-ring[data-level="${level}"]`);
  // Every label upright (never turned), inside its segment (its drawn box, with a small margin), whole words,
  // no ellipsis, at least 11 px (8 px only for one long word).
  const labelsFit=async()=>page.evaluate(async()=>{
    const d=await import('/js/quick-menu-dial.js');
    return [...document.querySelectorAll('.qm-labels .qm-label')].map(t=>{
      const b=t.getBBox(),[r0,r1,a0,a1]=t.dataset.box.split(',').map(Number),m=t.getCTM();
      return {text:[...t.querySelectorAll('tspan:not(.qm-wdot)')].map(x=>x.textContent).join(' / ')||t.textContent,size:parseFloat(t.style.fontSize),short:t.dataset.short==='true',
        inside:d.boxFits(b.x+b.width/2,b.y+b.height/2,b.width,b.height,{r0,r1,a0,a1},1),upright:Math.abs(m.b)<1e-6&&Math.abs(m.c)<1e-6,lines:t.querySelectorAll(':scope > tspan').length};
    });
  });
  const checkFit=async name=>{
    const ls=await labelsFit();
    assert.ok(ls.length,`${name}: labels drawn`);
    assert.equal(await page.locator('textPath').count(),0,`${name}: no curved text`);
    for(const l of ls){
      assert.ok(l.inside,`${name}: "${l.text}" inside its segment`);
      assert.ok(l.upright,`${name}: "${l.text}" upright`);
      assert.ok(l.lines>=1&&l.lines<=3,`${name}: "${l.text}" on 1 to 3 lines`);
      assert.ok(!/…|\.\.\./.test(l.text),`${name}: no ellipsis in "${l.text}"`);
      assert.ok(l.size>=11||(l.size>=8&&!/\s/.test(l.text.replace(/ \/ /g,'').trim())),`${name}: readable size (${l.size}px "${l.text}")`);
    }
    return ls;
  };
  // The taper on the drawn ring: Close/Back furthest out, each next one a smidge smaller, the last a notch.
  const checkTaper=async name=>{
    const o=await page.evaluate(()=>[...document.querySelectorAll('.qm-ring[data-active="true"] .qm-seg:not([data-hl="true"])')].map(p=>Number(p.dataset.outer)));
    assert.ok(o.length>1,name);
    assert.equal(Math.max(...o),o[0],`${name}: Close/Back is the biggest (${o})`);
    assert.equal(Math.min(...o),o.at(-1),`${name}: the last is the smallest (${o})`);
    for(let i=1;i<o.length;i++)assert.ok(o[i]<=o[i-1],`${name}: never larger than the one before (${o})`);
  };

  fs.mkdirSync(shots,{recursive:true});
  await page.addStyleTag({content:'html{background:linear-gradient(135deg,#2c3e55,#5a6b80)}'});
  const shoot=async(name,rings)=>{
    const size=await page.evaluate(n=>import('/js/quick-menu-dial.js').then(d=>d.dialSize(n)),rings);
    await page.setViewportSize(size);
    await page.waitForTimeout(50);
    await page.screenshot({path:path.join(shots,`quick-menu-dial-${name}.png`)});
    await page.setViewportSize({width:682,height:778});
  };

  // 4 rows: they fill the ring (90° each), Close on top and largest, the last a notch smaller.
  await show('four',{fresh:true});
  assert.equal(await ring(0).locator('.qm-seg').count(),4);
  await checkTaper('four');
  await checkFit('four');
  await shoot('4-rows',1);
  // The top level, 8 rows: Close on top, the taper, the highlight filled and pushed out, dark text.
  await show('top',{fresh:true});
  assert.equal(await ring(0).locator('.qm-seg').count(),8);
  assert.equal(await ring(0).locator('.qm-seg[data-row="0"]').getAttribute('data-kind'),'nav','row 0 is Close');
  assert.equal(await page.locator('.qm-stop').count(),0,'no stick: the taper is the end cue');
  await checkTaper('top');
  assert.equal(await page.locator('.qm-slot[data-hl="true"]').getAttribute('data-row'),'2');
  assert.equal(await page.locator('.qm-slot[data-hl="true"]').getAttribute('transform'),'translate(0.00 0.00)','the highlight stays in line');
  const grown=await page.evaluate(()=>[...document.querySelectorAll('.qm-ring[data-active="true"] .qm-seg')].map(p=>[p.dataset.hl==='true',Number(p.dataset.outer)]));
  const hi=grown.findIndex(x=>x[0]);
  assert.ok(grown[hi][1]>grown[hi-1][1],'the highlight grows outward past the one before it');
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
  await shoot('8-rows',1);

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
  await page.mouse.move(341,70);await page.mouse.wheel(0,120);
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
  assert.match(await ring(0).getAttribute('transform'),/rotate\(-90\.00\)/,'the chosen segment (slot 2 of 8) is at 12 o\'clock');
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
  assert.equal(await ring(0).getAttribute('transform'),'rotate(-90.00)','in place at once');

  // 31 entries: 8 segments, a moving window, "more" fades, no stop (not at the end), the position.
  await show('many',{fresh:true});
  assert.equal(await ring(1).locator('.qm-seg').count(),8);
  assert.equal(await ring(1).locator('.qm-seg[data-row="0"]').count(),1,'Back stays');
  assert.equal(await ring(1).locator('.qm-slot[data-row="20"]').getAttribute('data-hl'),'true');
  await checkTaper('many');
  assert.match(await page.locator('.qm-pos').textContent(),/^20 \/ 31$/);
  // Back a few: the window stays; "more" fades at both edges.
  await show('many',{extra:{highlight:16}});
  assert.equal(await ring(1).locator('.qm-slot[data-row="20"]').count(),1,'the window did not move');
  assert.equal(await page.locator('.qm-seg.qm-more').count(),2,'"more" both ways');
  await checkFit('many');
  await shoot('31',2);
  await show('many',{extra:{highlight:31}});
  assert.equal(await ring(1).locator('.qm-slot[data-row="31"]').getAttribute('data-hl'),'true','the window follows to the last entry');
  assert.equal(await page.locator('.qm-seg.qm-more').count(),1,'"more" only before');

  // 3 entries (and Back): four quarter segments with the taper.
  await show('three',{fresh:true});
  assert.equal(await ring(1).locator('.qm-seg').count(),4);
  await checkTaper('three');
  await checkFit('three');
  await shoot('three',2);

  // Long names: whole words on up to 3 lines, auto-fitted, else a deliberate short form; the caption has it all.
  await show('long',{fresh:true});
  const ls=await checkFit('long');
  assert.ok(ls.some(l=>l.short),'a deliberate short form for names that do not fit');
  // 0.37.0: a single long word is never cut: smaller, or split with a hyphen.
  const word=ls.map(l=>l.text).join('|');
  assert.ok(/Supercalifragilisticexpialidocious|Supercali\S*- \/ \S*docious/.test(word),`the long word is whole: ${word}`);
  assert.ok(ls.filter(l=>l.short).every(l=>!/Unified/.test(l.text)),`the shared project prefix is dropped: ${ls.map(l=>l.text).join(' | ')}`);
  assert.match(await page.locator('#qm-caption').textContent(),/fix the Vibe Key settings reads after a stalled wake from switched off/,'the full name in the caption');
  await shoot('long',3);
  console.log('long labels:',ls.map(l=>`${l.text} (${l.size}px${l.short?', short':''})`).join(' | '));

  assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),'no overflow');
  assert.deepEqual(errors,[]);
  console.log(`Quick menu dial checks passed. Screenshots: ${shots}`);
}catch(e){console.error(e);process.exitCode=1;}finally{await browser?.close();server.close();}})();
