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

  // ---- 1.0: the helix, the list style, the time bar. ----
  const h=await page.evaluate(async()=>{
    const d=await import('/js/quick-menu-dial.js');
    const at=k=>d.helixPlace(k);
    return {d:d.helixDiameter(),p0:at(0),p1:at(1),pm1:at(-1),p9:at(9),far:[at(-3),at(17)],
      edges:[d.helixEdge(d.HELIX_RO,-d.HELIX_STEP/2),d.helixEdge(d.HELIX_RO,d.HELIX_STEP/2)*Math.pow(d.HELIX_G,-1)],step:d.HELIX_STEP};
  });
  assert.equal(h.d,506,'the same size as quick_menu.rs style_size("dial")');
  assert.deepEqual([h.p0.rot,h.p0.scale,h.p0.opacity],[-0,1,1],'the highlight at 12 o clock, full size');
  assert.ok(h.p1.rot===-h.step&&h.p1.scale<1,'the next row anticlockwise and deeper');
  assert.ok(h.pm1.scale>1&&h.pm1.opacity<1,'the row before it larger, fading');
  assert.ok(Math.abs(h.edges[0]-h.edges[1])<1e-6,'one spiral: a segment edge meets the next one');
  assert.ok(h.p9.rot===-360&&h.p9.scale<0.7,'a full turn later, one band deeper');
  assert.deepEqual(h.far,[null,null],'drawn only a few rows back and 16 deep');

  const show=async(name,{fresh=false,extra={}}={})=>{
    await page.evaluate(([v,fresh])=>import('/js/quick-menu.js').then(m=>{if(fresh)m.render({open:false});m.render(v);}),[{...views[name],...extra},fresh]);
  };
  await page.addStyleTag({content:'html{background:linear-gradient(135deg,#2c3e55,#5a6b80)}'});
  const shoot=async(name,size={width:506,height:602})=>{
    await page.setViewportSize(size);
    await page.waitForTimeout(600);
    await page.screenshot({path:path.join(shots,`quick-menu-${name}.png`)});
    await page.setViewportSize({width:682,height:778});
  };
  const slot=row=>page.evaluate(r=>{const g=document.querySelector(`.qm-now .qm-slot[data-row="${r}"]`);return g?{t:g.getAttribute('transform'),shown:g.getAttribute('display')!=='none',op:Number(g.getAttribute('opacity'))}:null;},row);
  const labels=()=>page.evaluate(async()=>{
    const d=await import('/js/quick-menu-dial.js'),box=d.helixBox();
    return [...document.querySelectorAll('.qm-now .qm-label')].filter(t=>t.getAttribute('display')!=='none'&&t.closest('.qm-slot').getAttribute('display')!=='none').map(t=>{
      const b=t.getBBox(),m=t.getCTM();
      // In the page's own frame: the label's box at its resting angle (the label is turned back around the centre).
      const rot=Number(t.dataset.rot||0),r=((rot%360)+360)%360,a=r>180?r-360:r,[cx,cy]=d.centroid(box),[px,py]=d.centroid({...box,a0:box.a0+a,a1:box.a1+a});
      return {text:t.textContent,size:parseFloat(t.style.fontSize),inside:d.boxFits(b.x+b.width/2-cx+px,b.y+b.height/2-cy+py,b.width,b.height,{...box,a0:box.a0+a,a1:box.a1+a},1),
        upright:Math.abs(m.b)<1e-3&&Math.abs(m.c)<1e-3};
    });
  });
  const faces={faces:['Quick menu','Recent sessions'],faceIndex:0,face:'Quick menu',closeAfterMs:8000,leftMs:8000,style:'dial'};

  // The top level as a helix: every row drawn once, the highlight at the top, labels upright inside their segment.
  await show('top',{fresh:true,extra:faces});
  await page.waitForTimeout(300);
  assert.equal(await page.locator('.qm-now .qm-slot').count(),top.length,'one segment per row');
  assert.equal((await slot(2)).t,'rotate(0.00) scale(1.0000)','the highlight at the top');
  assert.match((await slot(3)).t,/^rotate\(-40\.00\) scale\(0\.95/,'the next one anticlockwise, smaller');
  const ls=await labels();
  assert.ok(ls.length>=6,'labels drawn');
  for(const l of ls){assert.ok(l.upright,`"${l.text}" upright`);assert.ok(l.inside,`"${l.text}" inside its segment`);}
  assert.equal(await page.locator('.qm-more, textPath, #qm-foot').count(),0,'no "more" fades, no end stop, no hint text');
  assert.match(await page.locator('#qm-caption').textContent(),/1\/2 Quick menu.*Claude sessions/,'the caption names the menu and the highlight');
  const bar=await page.evaluate(()=>getComputedStyle(document.getElementById('qm-bar')).transitionDuration);
  assert.equal(bar,'8s','the bar runs down over the time left');
  await shoot('helix-top');

  // A turn: the whole spiral turns and grows one step (mid-way every row is between places), then settles.
  await show('top',{extra:{...faces,highlight:3}});
  await page.waitForTimeout(60);
  const mid=await slot(3);
  assert.ok(!/^rotate\(0\.00\) scale\(1\.0000\)$/.test(mid.t)&&!/^rotate\(-40\.00\)/.test(mid.t),`moving mid-way: ${mid.t}`);
  await page.waitForTimeout(300);
  assert.equal((await slot(3)).t,'rotate(0.00) scale(1.0000)','settled with the new highlight at the top');
  assert.match((await slot(2)).t,/^rotate\(40\.00\) scale\(1\.05/,'the previous one moved out and clockwise');

  // Into a sub-menu: the old level drills a turn deeper and fades; the new one spins in from outside.
  const drills=await page.evaluate(()=>Number(document.getElementById('qm-dial').dataset.drills||0));
  await show('many',{extra:{...faces,title:['Quick menu','Claude sessions']}});
  await page.waitForTimeout(150);
  assert.equal(await page.locator('.qm-out').count(),1,'the old level is still spinning away');
  await page.waitForTimeout(500);
  assert.equal(await page.locator('.qm-out').count(),0,'and gone afterwards');
  assert.equal(await page.evaluate(()=>Number(document.getElementById('qm-dial').dataset.drills)),drills+1,'one drill');
  // 31 entries: no window, no "more": the spiral goes deeper (up to 16 rows after the highlight, 2 before).
  const shown=await page.evaluate(()=>[...document.querySelectorAll('.qm-now .qm-slot')].filter(g=>g.getAttribute('display')!=='none').map(g=>Number(g.dataset.row)));
  assert.deepEqual([Math.min(...shown),Math.max(...shown)],[18,31],`rows around the highlight: ${shown}`);
  await shoot('helix-many');

  // Mouse on the helix: a click chooses a segment; the wheel moves.
  await page.evaluate(()=>{window.__inputs=[];});
  await page.evaluate(()=>document.querySelector('.qm-now .qm-seg[data-row="21"]').dispatchEvent(new MouseEvent('click',{bubbles:true})));
  await page.mouse.move(341,120);await page.mouse.wheel(0,120);
  let inputs=await page.evaluate(()=>window.__inputs);
  assert.deepEqual(inputs,[{op:'pick',index:21},{op:'next'}],JSON.stringify(inputs));

  // The list style (K2's second menu): every row, the highlight in view, Close first.
  const recent=[{label:'Close',type:'close'},...many.slice(1).map(r=>({...r,detail:`${r.detail} · 5 min`}))];
  await page.evaluate(v=>import('/js/quick-menu.js').then(m=>m.render(v)),{...base,style:'list',title:['Recent sessions'],faces:['Quick menu','Recent sessions'],faceIndex:1,face:'Recent sessions',rows:recent,highlight:25,depth:0,levels:[]});
  await page.setViewportSize({width:440,height:560});
  await page.waitForTimeout(100);
  await page.evaluate(()=>document.querySelector('#qm-list [data-hl]').scrollIntoView({block:'nearest'}));
  assert.equal(await page.locator('.qm-row').count(),recent.length,'one row each');
  assert.equal(await page.locator('.qm-wheel').isVisible(),false,'no dial in the list style');
  const inView=await page.evaluate(()=>{const l=document.getElementById('qm-list'),r=l.querySelector('[data-hl]').getBoundingClientRect(),b=l.getBoundingClientRect();return r.top>=b.top-1&&r.bottom<=b.bottom+1;});
  assert.ok(inView,'the highlighted row is scrolled into view');
  assert.match(await page.locator('#qm-list-title').textContent(),/2\/2 Recent sessions/);
  await page.evaluate(()=>{window.__inputs=[];});
  await page.locator('.qm-row[data-row="24"]').click();
  assert.deepEqual(await page.evaluate(()=>window.__inputs),[{op:'pick',index:24}]);
  await shoot('list',{width:440,height:560});

  // Dragging (the 0.34.0 bug): a blur while the handle is held, while the window moves and for 300 ms after is
  // ignored; the idle close is held; afterwards a blur closes again.
  await show('top',{fresh:true,extra:faces});
  const handle=page.locator('#qm-handle');
  await page.evaluate(()=>{window.__inputs=[];});
  const box=await handle.boundingBox();
  await page.mouse.move(box.x+box.width/2,box.y+box.height/2);
  await page.mouse.down();
  const q=await page.evaluate(()=>import('/js/quick-menu.js').then(m=>{const r=[m.blurred()];m.moved();r.push(m.blurred());m.moved();return r;}));
  assert.deepEqual(q,[false,false],'no close during the drag');
  await page.mouse.up();
  assert.equal(await page.evaluate(()=>import('/js/quick-menu.js').then(m=>m.blurred())),false,'nor right after it');
  await page.waitForTimeout(800);
  inputs=await page.evaluate(()=>window.__inputs.map(i=>i.op||i.command));
  assert.ok(!inputs.includes('close'),`no close: ${inputs}`);
  assert.ok(inputs.filter(i=>i==='hold').length>=2,`the idle close is held: ${inputs}`);
  assert.equal(await page.evaluate(()=>import('/js/quick-menu.js').then(m=>m.blurred())),true,'a later blur closes');
  // Long names in the helix: inside, upright.
  await show('long',{fresh:true,extra:faces});
  await page.waitForTimeout(100);
  for(const l of await labels()){assert.ok(l.inside&&l.upright,`long: "${l.text}"`);}
  assert.match(await page.locator('#qm-caption').textContent(),/fix the Vibe Key settings reads after a stalled wake from switched off/,'the full name in the caption');
  await shoot('helix-long');

  assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),'no overflow');
  assert.deepEqual(errors,[]);
  console.log(`Quick menu checks passed. Screenshots: ${shots}`);
}catch(e){console.error(e);process.exitCode=1;}finally{await browser?.close();server.close();}})();
