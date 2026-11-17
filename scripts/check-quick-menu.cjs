// Headless fixture only: the Vibe Key quick menu dial (public/quick-menu.html, 0.34.0). Never a real window:
// a headless browser off the desktop. Screenshots go to UC_QM_SHOTS (default: %TEMP%/uc-quick-menu-preview).
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
  {label:'Switch profile',type:'submenu'},{label:'Send to',type:'submenu'},{label:'Volume up',type:'action'}];
const groups=[{label:'Back',type:'back'},{label:'Unified Control',type:'submenu',detail:'4 sessions'},{label:'Project Nocturne',type:'submenu',detail:'2 sessions'},
  {label:'Other sessions',type:'submenu',detail:'7 sessions'}];
const sessions=[{label:'Back',type:'back'},{label:'Open: Vibe Key AU05 connection setup',type:'claudeSession',detail:'Unified Control',working:true},
  {label:'Quick menu dial with rings and pie segments',type:'claudeSession',detail:'Unified Control'},{label:'Wake reads',type:'claudeSession',detail:'Unified Control'},
  {label:'Cooler freeze diagnostics',type:'claudeSession',detail:'CoolerScreen',working:true}];
const many=[{label:'Back',type:'back'},...Array.from({length:31},(_,i)=>({label:`Session ${i+1}: a fairly long working title`,type:'claudeSession',detail:i%2?'vault':'CoolerScreen',working:i===19}))];
const base={open:true,profile:'Claude',target:'Claude',note:null,closeAfterS:8,hints:'Turn: move · K2 select · K3 back · Knob press select'};
const views={
  top:{...base,title:['Quick menu'],depth:0,levels:[],rows:top,highlight:2},
  sub:{...base,title:['Quick menu','Claude sessions'],depth:1,levels:[{rows:top.map(({label,type})=>({label,type})),chosen:2}],rows:groups,highlight:1},
  deep:{...base,title:['Quick menu','Claude sessions','Unified Control'],depth:2,
    levels:[{rows:top.map(({label,type})=>({label,type})),chosen:2},{rows:groups.map(({label,type})=>({label,type})),chosen:1}],rows:sessions,highlight:2},
  many:{...base,title:['Quick menu','Other sessions'],depth:1,levels:[{rows:top.map(({label,type})=>({label,type})),chosen:2}],rows:many,highlight:20},
};

(async()=>{let browser;try{
  await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
  browser=await chromium.launch({headless:true});
  const page=await browser.newPage({viewport:{width:416,height:512}}),errors=[];
  page.on('pageerror',e=>errors.push(e.message));
  await page.addInitScript(()=>{
    window.__inputs=[];
    window.__TAURI_INTERNALS__={transformCallback:()=>1,invoke:async(command,args)=>{if(command==='au05_menu')window.__inputs.push(args.input);return null;}};
  });
  await page.goto(`http://127.0.0.1:${server.address().port}/quick-menu.html`);
  await page.waitForFunction(()=>document.getElementById('qm-dial'));

  // Geometry.
  const g=await page.evaluate(async()=>{
    const d=await import('/js/quick-menu-dial.js');
    return {
      angles6:d.slotAngles(6).map(a=>[a.start,a.mid,a.end]),
      angles12:d.slotAngles(12).map(a=>a.mid),
      off:[[32,1,0],[32,11,0],[32,12,0],[32,20,0],[32,31,0],[32,15,9],[32,10,9],[32,0,9],[8,7,0]].map(([c,h,p])=>d.windowOffset(c,h,p)),
      rows20:d.slotRows(32,9),rows5:d.slotRows(5,0),
      steps:[d.step(32,0,-1),d.step(32,31,1),d.step(32,5,1)],
      radii:[1,2,3,4].map(n=>d.ringRadii(n)),
      sizes:[1,2,3].map(n=>d.dialSize(n)),
      chars:d.shorten('Quick menu dial',6),
    };
  });
  assert.deepEqual(g.angles6[0],[-30,0,30],'slot 0 is centred at 12 o\'clock');
  assert.deepEqual(g.angles6[1],[30,60,90],'the next slot is clockwise');
  assert.equal(g.angles12.length,12);
  assert.deepEqual(g.off,[0,0,1,9,20,9,9,9,0],'the window moves only at its edges, never past the ends');
  assert.deepEqual(g.rows20,[0,10,11,12,13,14,15,16,17,18,19,20],'Back stays on top, 11 rows move');
  assert.deepEqual(g.rows5,[0,1,2,3,4]);
  assert.deepEqual(g.steps,[0,31,6],'no wrap-around');
  const outer=g.radii.slice(0,3).map(r=>r.at(-1)[1]);
  assert.deepEqual(outer,[120,150,188]);
  for(let i=1;i<3;i++)assert.ok(Math.abs(outer[i]/outer[i-1]-1.25)<0.01,'each level is about 1.25 × larger');
  for(const set of g.radii)for(let i=1;i<set.length;i++)assert.ok(set[i][0]>set[i-1][1],'rings never overlap');
  for(const set of g.radii)assert.ok(set.at(-1)[1]-set.at(-1)[0]>=56,'the active ring stays thick');
  assert.deepEqual(g.radii[3],g.radii[2],'at most 3 rings');
  assert.deepEqual(g.sizes,[{width:340,height:376},{width:340,height:436},{width:416,height:512}],'the same sizes as quick_menu.rs dial_size');
  assert.equal(g.chars,'Quick…');

  const show=async(name,{fresh=false}={})=>{
    await page.evaluate(([v,fresh])=>import('/js/quick-menu.js').then(m=>{if(fresh)m.render({open:false});m.render(v);}),[views[name],fresh]);
  };
  const ring=level=>page.locator(`.qm-ring[data-level="${level}"]`);

  // The top level: 6 segments, Close on top, the highlight filled and pushed out.
  await show('top',{fresh:true});
  assert.equal(await page.locator('.qm-ring').count(),1);
  assert.equal(await ring(0).locator('.qm-seg').count(),6);
  assert.equal(await ring(0).locator('.qm-seg[data-row="0"]').getAttribute('data-kind'),'nav','row 0 is Close');
  assert.equal(await page.locator('.qm-seg[data-hl="true"]').getAttribute('data-row'),'2');
  assert.notEqual(await page.locator('.qm-seg[data-hl="true"]').getAttribute('transform'),'translate(0.00 0.00)','pushed out');
  assert.match(await page.locator('#qm-caption').textContent(),/Claude sessions/);
  assert.match(await page.locator('#qm-foot').textContent(),/K2 select · K3 back/);
  const handle=page.locator('.qm-handle');
  assert.equal(await handle.getAttribute('data-tauri-drag-region'),'','the centre is the drag handle');
  assert.equal(await handle.evaluate(e=>getComputedStyle(e).cursor),'move');
  fs.mkdirSync(shots,{recursive:true});
  const backdrop=()=>page.addStyleTag({content:'html{background:linear-gradient(135deg,#2c3e55,#5a6b80)}'});
  await backdrop();
  // Each at the window's real size for its rings (top-aligned, as in the window).
  const shoot=async(name,rings)=>{
    const size=await page.evaluate(n=>import('/js/quick-menu-dial.js').then(d=>d.dialSize(n)),rings);
    await page.setViewportSize(size);
    await page.screenshot({path:path.join(shots,`quick-menu-dial-${name}.png`)});
    await page.setViewportSize({width:416,height:512});
  };
  await shoot('top',1);

  // Mouse: a segment click chooses it; the wheel moves; the handle does nothing.
  await page.evaluate(()=>{window.__inputs=[];});
  await page.locator('.qm-seg[data-row="3"]').click({force:true});
  await page.mouse.move(208,60);await page.mouse.wheel(0,120);
  await handle.click();
  const inputs=await page.evaluate(()=>window.__inputs.filter(i=>i.op!=='close'));
  assert.deepEqual(inputs,[{op:'pick',index:3},{op:'next'}],JSON.stringify(inputs));

  // Into Claude sessions: the parent turns its chosen segment to the top and stays dimmed inside the new ring.
  await show('sub');
  await page.waitForTimeout(350);
  assert.equal(await page.locator('.qm-ring').count(),2);
  assert.match(await ring(0).getAttribute('transform'),/rotate\(-120\.00\)/,'the chosen segment (slot 2 of 6) is at 12 o\'clock');
  assert.equal(await ring(0).getAttribute('data-active'),'false');
  assert.equal(await page.locator('#qm-labels').evaluate(e=>e.style.opacity),'1');
  assert.match(await page.locator('.qm-plabel').textContent(),/Claude/);
  await shoot('sub',2);
  // Deeper: three rings, the outer one the largest.
  await show('deep');
  await page.waitForTimeout(350);
  assert.equal(await page.locator('.qm-ring').count(),3);
  const boxes=await page.evaluate(()=>[...document.querySelectorAll('.qm-ring')].map(r=>r.getBBox().width));
  assert.ok(boxes[0]<boxes[1]&&boxes[1]<boxes[2],`each deeper ring is bigger: ${boxes}`);
  assert.match(await page.locator('#qm-caption').textContent(),/Quick menu dial with rings and pie segments.*Unified Control/,'the full name and project in the caption');
  await shoot('deep',3);
  // Back reverses it.
  await show('sub');
  await page.waitForTimeout(350);
  assert.equal(await page.locator('.qm-ring').count(),2,'the outer ring is gone');
  assert.match(await ring(1).getAttribute('transform'),/rotate\(0(\.00)?\) scale\(1\.0000\)|rotate\(0\.00\)/);

  // Reopened where it was left: drawn in place, no animation.
  await show('deep',{fresh:true});
  assert.equal(await page.locator('.qm-ring').count(),3);
  assert.equal(await ring(0).getAttribute('transform'),'rotate(-120.00)','in place at once');
  assert.equal(await page.locator('#qm-labels').evaluate(e=>e.style.opacity),'1');

  // 31 entries: 12 segments, a moving window, the position in the caption.
  await show('many',{fresh:true});
  assert.equal(await ring(1).locator('.qm-seg').count(),12);
  assert.equal(await ring(1).locator('.qm-seg[data-row="0"]').count(),1,'Back stays');
  assert.equal(await ring(1).locator('.qm-seg[data-row="20"]').getAttribute('data-hl'),'true');
  assert.match(await page.locator('.qm-pos').textContent(),/^20 \/ 31$/);
  await shoot('31',2);
  await show('many');
  await page.evaluate(v=>import('/js/quick-menu.js').then(m=>m.render({...v,highlight:31})),views.many);
  assert.equal(await ring(1).locator('.qm-seg[data-row="31"]').getAttribute('data-hl'),'true','the window follows to the last entry');
  assert.equal(await ring(1).locator('.qm-seg[data-row="1"]').count(),0);

  assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),'no overflow');
  assert.deepEqual(errors,[]);
  console.log(`Quick menu dial checks passed. Screenshots: ${shots}`);
}catch(e){console.error(e);process.exitCode=1;}finally{await browser?.close();server.close();}})();
