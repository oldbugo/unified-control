// Vibe Key quick menu window (0.34.0: a dial; 0.35.0: bigger, a bump at the ends, dragging that no longer closes
// it; 0.37.0: the rows share the whole ring, a thicker band, a taper as the end cue, upright labels; 0.38.0: the
// taper is one smooth spiral, parallel-sided gaps; 1.1.1: several menus with a list style, the whole ring twists
// with each turn, a level change spins, a bar runs down to the idle close). Shows the
// core's menu view and sends input back (au05_menu). The core owns the state: knob turns, the knob press and
// the menu key arrive there directly; this page adds the keyboard (arrows, Enter, Backspace, Esc) and the mouse
// (click a segment, wheel over the ring).
//
// Each menu level is a thick ring of segments sharing the whole circle; row 0 (Close at the top, Back in a
// sub-menu) is at 12 o'clock, the largest segment, and the others follow clockwise, each a little smaller, the
// last a notch smaller: the step back up to row 0 is where the list starts and ends. Labels are always upright.
// Going into a level turns the chosen segment to the top of its (now dimmed) ring and the new level appears as a
// larger ring around it; at most 3 rings show. The filled hole in the middle is the drag handle.
import {PUSH,MAX_SLOTS,ringRadii,dialDiameter,fitScale,slotAngles,windowOffset,slotRows,edges,segmentPath,
  point,tangent,fitLabel,taper,shade,spiralAt,spiralPath,gapAngle,centroid} from './quick-menu-dial.js';

const T=window.__TAURI_INTERNALS__;
const send=(op,index)=>T?T.invoke('au05_menu',{input:index===undefined?{op}:{op,index}}).then(render).catch(()=>{}):Promise.resolve();
const esc=s=>String(s??'').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
const $=id=>document.getElementById(id);
const SEP='␟', ANIM_MS=320, BUMP_MS=120, GAP=1.2;
/** 1.1.1 (user feedback 2026-11-18: the old dial's design, turning to the right, with the whole circle moving):
 * each move twists the whole ring this far in the turn's direction (clockwise for the next entry) and it settles
 * back; a level change adds a spin of SPIN degrees to the grow or shrink. */
const TWIST=2.5, TWIST_MS=240, SPIN=70, BUMP_PX=4;
/** 1.2.1 (user feedback 2026-11-19: turning on against the end "just keeps shaking around"): twists and bumps that
 * follow each other quickly get weaker each time (the first full, then 55 %, 30 %...), and are full again after a
 * short pause. */
const SHAKE_DECAY=0.55, SHAKE_RESET_MS=600;
let shake={n:0,at:0};
function shakeStrength(){
  const now=performance.now();
  if(now-shake.at>SHAKE_RESET_MS)shake.n=0;
  shake={n:shake.n+1,at:now};
  const k=Math.pow(SHAKE_DECAY,shake.n-1);
  const d=document.getElementById('qm-dial');if(d)d.dataset.shake=k.toFixed(3);
  return k;
}
/** 1.2.0 (user request 2026-11-19): a list longer than the ring flows. Instead of dashed "more" edges, the tail
 * fades along the line and the next entry peeks out from under the top block (Close/Back); with entries before the
 * window the top block fades toward its other side and the previous entry peeks out there. When the window moves,
 * every block slides along the circle (anticlockwise going toward the tail), the one leaving tucks under the top
 * block and the new one comes out from under it. */
const RIM=7, STEP_DEG=360/MAX_SLOTS;
/** 1.2.1 (user feedback 2026-11-19: turned fast, "it almost happens instantly"): the ring chases its target instead
 * of restarting a fixed animation on every turn. It closes in with a time constant of FLOW_TAU ms, never slower
 * than FLOW_MIN and never faster than FLOW_MAX slots per second, so even fast turns are seen flowing. */
const FLOW_TAU=75, FLOW_MIN=4, FLOW_MAX=20;
const GLYPH={close:'✕',back:'‹'};

// Text widths for fitting the labels (the label font, semibold so the highlight fits too).
const ctx=document.createElement('canvas').getContext('2d');
const FONT=getComputedStyle(document.documentElement).getPropertyValue('--font').trim()||'"Segoe UI", system-ui, sans-serif';
export function measure(text,size){ctx.font=`600 ${size}px ${FONT}`;return ctx.measureText(text).width;}

// Per level (by its title path): the first row shown after row 0 when the list is longer than the ring.
const offsets=new Map();
// The last render: open, depth, each ring's model by level (for the animation) and the last bump seen.
let last={open:false,depth:0,rings:new Map(),bump:0};
let frame=0, ids=0;

/** The rings to draw (oldest first): rows, the selected row, the visible window, angles, radii, rotation. */
export function model(v){
  const rows=Array.isArray(v.rows)?v.rows:[];
  const levels=(Array.isArray(v.levels)?v.levels:[]).slice(-2);
  const title=Array.isArray(v.title)?v.title:['Quick menu'];
  const depth=Number.isInteger(v.depth)?v.depth:Math.max(title.length-1,0);
  const stack=[...levels.map(l=>({rows:Array.isArray(l.rows)?l.rows:[],sel:l.chosen|0,active:false})),{rows,sel:v.highlight|0,active:true}];
  const radii=ringRadii(stack.length);
  return stack.map((ring,k)=>{
    const level=depth-(stack.length-1-k);
    const key=title.slice(0,level+1).join(SEP);
    const count=ring.rows.length;
    const offset=windowOffset(count,ring.sel,offsets.get(key)||0);
    offsets.set(key,offset);
    const shown=slotRows(count,offset);
    const angles=slotAngles(shown.length);
    const slot=Math.max(shown.indexOf(ring.sel),0);
    return {...ring,level,count,offset,shown,angles,slot,edges:edges(count,offset),taper:taper(shown.length),r0:radii[k][0],r1:radii[k][1],rot:ring.active?0:-(angles[slot]?.mid||0)};
  });
}

const kind=r=>r?.type==='close'||r?.type==='back'?'nav':r?.type==='submenu'?'sub':'item';

/** An upright label (lines, size, centre from `fitLabel`), clipped to its segment. */
function labelSvg(fit,cls,attrs,clip){
  const top=fit.y-(fit.lines.length*fit.lh)/2+fit.lh/2;
  const spans=fit.lines.map((l,j)=>`<tspan x="${fit.x.toFixed(1)}" y="${(top+j*fit.lh).toFixed(1)}">${l}</tspan>`).join('');
  return `<text class="${cls}" style="font-size:${fit.size}px"${attrs} clip-path="url(#${clip})">${spans}</text>`;
}
/** A label's lines with the working dot as a marked-up span. */
function markup(fit,r){
  return {...fit,lines:fit.lines.map((l,j)=>{
    let s=esc(l);
    if(j===0&&r.working)s=s.replace(/^●/,'<tspan class="qm-wdot">●</tspan>');
    return s;
  })};
}

/** A segment's geometry: the outer edge (the spiral on the current ring, a circle on a parent), its path, and
 * the box labels fit in (the band at the segment's smallest outer radius, less the widest gap angle). */
function seg(m,i){
  const a=m.angles[i],n=m.shown.length,many=n>1;
  // The highlighted segment grows outward along the spiral (0.38.0: it no longer moves sideways, so its sides
  // stay lined up with its neighbours).
  const grow=m.active&&m.shown[i]===m.sel?PUSH:0;
  const outer=m.active?(x=>m.r1+spiralAt(n,x)+grow):(()=>m.r1);
  const d=m.active&&many?spiralPath(m.r0,outer,a.start,a.end):segmentPath(m.r0,m.r1,a.start+(many?GAP:0),a.end-(many?GAP:0));
  const gap=many?Math.max(GAP,m.active?gapAngle(m.r0):0):0;
  return {r0:m.r0,r1:Math.min(outer(a.start),outer(a.end)),a0:a.start+gap,a1:a.end-gap,mid:a.mid,d,mark:outer(a.mid)};
}

/** The ring (segments only; it turns as a whole when it becomes a parent). */
function ringSvg(m){
  if(m.active&&m.count>MAX_SLOTS)return longRingSvg(m);
  const parts=m.shown.map((row,i)=>{
    const g=seg(m,i),r=m.rows[row]||{};
    const hl=row===m.sel;
    const [dx,dy]=[0,0];
    const d=g.d;
    // The "more" fade on an edge segment whose neighbours in the list are not shown.
    const fade=!hl&&((i===m.shown.length-1&&m.edges.moreAfter)||(i===1&&m.edges.moreBefore));
    let fill='';
    if(fade){
      const toEnd=i===m.shown.length-1;
      const rm=(g.r0+g.r1)/2,[x0,y0]=point(rm,g.mid),[x1,y1]=point(rm,toEnd?g.a1:g.a0);
      fill=`<linearGradient id="qm-g${m.level}-${i}" gradientUnits="userSpaceOnUse" x1="${x0.toFixed(1)}" y1="${y0.toFixed(1)}" x2="${x1.toFixed(1)}" y2="${y1.toFixed(1)}"><stop offset="0" class="qm-fade0"/><stop offset="1" class="qm-fade1"/></linearGradient>`;
    }
    // The subtle shading toward the end of the list (heavier just before Back/Close).
    const sh=m.active?shade(m.shown.length,i):0;
    return `<g class="qm-slot" data-row="${row}"${hl?' data-hl="true"':''} transform="translate(${dx.toFixed(2)} ${dy.toFixed(2)})">${fill}<path class="qm-seg${fade?' qm-more':''}" data-kind="${kind(r)}" data-row="${row}" data-outer="${g.mark.toFixed(2)}"${hl?' data-hl="true"':''}${r.current?' data-current="true"':''}${fade?` fill="url(#qm-g${m.level}-${i})"`:''} style="--shade:${sh.toFixed(3)}" d="${d}"><title>${esc(r.label)}</title></path></g>`;
  }).join('');
  return `<g class="qm-ring" data-level="${m.level}" data-active="${m.active}" transform="rotate(${m.rot.toFixed(2)})">${parts}</g>`;
}

// ---- The flowing ring (1.2.0): long lists only. A row's place is a fraction `s` of a slot (row - window offset):
// 1..7 are the visible slots, below 1 it is tucked under the top block's clockwise side, above 7 under its other
// side. ----
const flowOpacity=s=>s<1?Math.max(0,Math.min(1,s)):s>MAX_SLOTS-1?Math.max(0,MAX_SLOTS-s):1;
function flowGeom(m,s,row){
  const mid=s*STEP_DEG,a0=mid-STEP_DEG/2,a1=mid+STEP_DEG/2;
  const grow=row===m.sel?PUSH:0;
  const outer=x=>m.r1+spiralAt(MAX_SLOTS,x)+grow;
  return {d:spiralPath(m.r0,outer,a0,a1),mark:outer(mid),a0,a1,mid,outer};
}
/** The box a label fits in at place s (as `seg`). */
function flowBox(m,s,row){
  const g=flowGeom(m,s,row),gap=Math.max(GAP,gapAngle(m.r0));
  return {r0:m.r0,r1:Math.min(g.outer(g.a0),g.outer(g.a1)),a0:g.a0+gap,a1:g.a1-gap};
}
const clampSlot=s=>Math.min(Math.max(s,1),MAX_SLOTS-1);
/** The rows a long ring draws: the window, plus rows passing through during a move (`m.transit`). */
const flowRows=m=>[...new Set([...m.shown.slice(1),...(m.transit||[])])];
const fadeGrad=(id,g,from,to,o0,o1)=>{
  const rm=(g.r0+g.r1)/2,[x0,y0]=point(rm,from),[x1,y1]=point(rm,to);
  return `<linearGradient id="${id}" gradientUnits="userSpaceOnUse" x1="${x0.toFixed(1)}" y1="${y0.toFixed(1)}" x2="${x1.toFixed(1)}" y2="${y1.toFixed(1)}"><stop offset="0" class="qm-fade0" style="stop-opacity:${o0}"/><stop offset="1" class="qm-fade1" style="stop-opacity:${o1}"/></linearGradient>`;
};

function longRingSvg(m){
  const parts=[];
  const gapA=Math.max(GAP,gapAngle(m.r0))+1;
  // The peeks: the next (or previous) entry as a thin rim standing out from under the top block.
  const peek=(row,a0,a1,side)=>`<g class="qm-peek" data-side="${side}" data-row="${row}"><path class="qm-seg qm-peek-seg" data-row="${row}" d="${spiralPath(m.r1-6,x=>m.r1+spiralAt(MAX_SLOTS,x)+RIM,a0,a1)}"><title>${esc(m.rows[row]?.label)}</title></path></g>`;
  if(m.edges.moreAfter)parts.push(peek(m.offset+MAX_SLOTS,-STEP_DEG/2+gapA,-2,'after'));
  if(m.edges.moreBefore)parts.push(peek(m.offset,2,STEP_DEG/2-gapA,'before'));
  for(const row of flowRows(m)){
    const s=row-m.offset,g=flowGeom(m,s,row),r=m.rows[row]||{},hl=row===m.sel,op=flowOpacity(s);
    // The tail fades along the line toward the top block when more entries follow.
    const tail=!hl&&m.edges.moreAfter&&s===MAX_SLOTS-1;
    const grad=tail?fadeGrad(`qm-t${m.level}`,{r0:m.r0,r1:m.r1},g.a0+4,g.a1,1,0.2):'';
    const sh=shade(MAX_SLOTS,clampSlot(s));
    parts.push(`<g class="qm-slot" data-row="${row}" data-s="${s}"${hl?' data-hl="true"':''} opacity="${op.toFixed(3)}"${op<=0?' display="none"':''}>${grad}<path class="qm-seg${tail?' qm-tail':''}" data-kind="${kind(r)}" data-row="${row}" data-outer="${g.mark.toFixed(2)}"${hl?' data-hl="true"':''}${r.current?' data-current="true"':''}${tail?` data-fill="url(#qm-t${m.level})"`:''} style="--shade:${sh.toFixed(3)}${tail?`;fill:url(#qm-t${m.level})`:''}" d="${g.d}"><title>${esc(r.label)}</title></path></g>`);
  }
  // The top block last, so the others slide under it; it fades toward its clockwise side when entries are before.
  const r0=m.rows[0]||{},hl0=m.sel===0,g0=flowGeom(m,0,0);
  const faded=!hl0&&m.edges.moreBefore;
  const grad0=faded?fadeGrad(`qm-b${m.level}`,{r0:m.r0,r1:m.r1},g0.a0+10,g0.a1,1,0.3):'';
  parts.push(`<g class="qm-slot" data-row="0" data-s="0"${hl0?' data-hl="true"':''}>${grad0}<path class="qm-seg${faded?' qm-topfade':''}" data-kind="${kind(r0)}" data-row="0" data-outer="${g0.mark.toFixed(2)}"${hl0?' data-hl="true"':''}style="--shade:0${faded?`;fill:url(#qm-b${m.level})`:''}" d="${g0.d}"><title>${esc(r0.label)}</title></path></g>`);
  return `<g class="qm-ring" data-level="${m.level}" data-active="true" data-flow="true" transform="rotate(0.00)">${parts.join('')}</g>`;
}

function longLabelsSvg(m){
  const siblings=m.rows.filter(r=>r.type!=='close'&&r.type!=='back').map(r=>r.label);
  return [0,...flowRows(m)].map(row=>{
    const s=row===0?0:row-m.offset,sc=row===0?0:clampSlot(s),r=m.rows[row]||{},hl=row===m.sel;
    const box=flowBox(m,sc,row),clip=`qm-c${++ids}`,glyph=GLYPH[r.type];
    const fit=fitLabel(r.label,{siblings:glyph?[]:siblings,box,measure,prefix:glyph||(r.working?'●':''),suffix:r.type==='submenu'?'›':''});
    const [cx,cy]=centroid(box),[px,py]=centroid(flowBox(m,s,row));
    const op=row===0?1:flowOpacity(s);
    const attrs=` data-kind="${kind(r)}" data-row="${row}"${hl?' data-hl="true"':''}${fit.short?' data-short="true"':''} data-box="${[box.r0,box.r1,box.a0,box.a1].map(n=>n.toFixed(2)).join(',')}"`;
    return `<g class="qm-lab" data-row="${row}" data-sc="${sc}"${hl?' data-hl="true"':''} opacity="${op.toFixed(3)}"${op<=0?' display="none"':''} transform="translate(${(px-cx).toFixed(2)} ${(py-cy).toFixed(2)})"><clipPath id="${clip}"><path d="${flowGeom(m,sc,row).d}"/></clipPath>${labelSvg(markup(fit,r),'qm-label',attrs,clip)}</g>`;
  }).join('');
}

/** The window moved: every row slides along the circle from where it is drawn now (`flowPos`, a fraction while
 * moving) toward the new offset, chasing it when turns come faster than the motion. */
let flowFrame=0, flowPos=null, flowKey='';
function placeFlow(m,p,parts){
  const {slots,labs,peeks}=parts;
  for(const g of slots){
    const row=Number(g.dataset.row),s=row-p,op=flowOpacity(s);
    if(op<=0){g.setAttribute('display','none');continue;}
    g.removeAttribute('display');
    g.querySelector('path').setAttribute('d',flowGeom(m,s,row).d);
    g.setAttribute('opacity',op.toFixed(3));
  }
  for(const l of labs){
    const row=Number(l.dataset.row),s=row-p,sc=Number(l.dataset.sc),op=flowOpacity(s);
    if(op<=0){l.setAttribute('display','none');continue;}
    l.removeAttribute('display');
    const [cx,cy]=centroid(flowBox(m,sc,row)),[px,py]=centroid(flowBox(m,s,row));
    l.setAttribute('transform',`translate(${(px-cx).toFixed(2)} ${(py-cy).toFixed(2)})`);
    l.setAttribute('opacity',(op*op).toFixed(3));
  }
  const settle=Math.max(0,1-Math.abs(m.offset-p));
  for(const k of peeks)k.setAttribute('opacity',settle.toFixed(3));
}
function flow(m,from){
  cancelAnimationFrame(flowFrame);
  const svg=$('qm-dial'),ring=svg.querySelector('.qm-ring[data-flow="true"]');
  if(!ring){flowPos=m.offset;return;}
  svg.dataset.flows=String(Number(svg.dataset.flows||0)+1);
  const parts={slots:[...ring.querySelectorAll('.qm-slot')].filter(g=>g.dataset.row!=='0'),
    labs:[...svg.querySelectorAll('.qm-labels .qm-lab')].filter(g=>g.dataset.row!=='0'),peeks:[...ring.querySelectorAll('.qm-peek')]};
  const fills=[...ring.querySelectorAll('[data-fill]')];
  for(const f of fills)f.style.fill='';
  flowPos=from;
  placeFlow(m,flowPos,parts); // drawn where it was at once: no flash of the end state
  let t0=performance.now();
  const tick=now=>{
    const dt=Math.min(now-t0,50);t0=now;
    const d=m.offset-flowPos,dist=Math.abs(d);
    const step=Math.min(dist,Math.max(dist*(1-Math.exp(-dt/FLOW_TAU)),FLOW_MIN*dt/1000),FLOW_MAX*dt/1000);
    flowPos+=Math.sign(d)*step;
    if(Math.abs(m.offset-flowPos)<1e-3)flowPos=m.offset;
    placeFlow(m,flowPos,parts);
    if(flowPos!==m.offset)flowFrame=requestAnimationFrame(tick);
    else for(const f of fills)f.style.fill=f.dataset.fill;
  };
  flowFrame=requestAnimationFrame(tick);
}

/** The current ring's labels: upright, never turned (they sit outside the turning ring group). */
function labelsSvg(m){
  if(m.count>MAX_SLOTS)return longLabelsSvg(m);
  const siblings=m.rows.filter(r=>r.type!=='close'&&r.type!=='back').map(r=>r.label);
  return m.shown.map((row,i)=>{
    const g=seg(m,i),r=m.rows[row]||{};
    const hl=row===m.sel;
    const [dx,dy]=[0,0];
    const clip=`qm-c${++ids}`;
    const glyph=GLYPH[r.type];
    const fit=fitLabel(r.label,{siblings:r.type==='close'||r.type==='back'?[]:siblings,box:g,measure,
      prefix:glyph||(r.working?'●':''),suffix:r.type==='submenu'?'›':''});
    const attrs=` data-kind="${kind(r)}" data-row="${row}"${hl?' data-hl="true"':''}${fit.short?' data-short="true"':''} data-box="${[g.r0,g.r1,g.a0,g.a1].map(n=>n.toFixed(2)).join(',')}"`;
    return `<g class="qm-lab" data-row="${row}"${hl?' data-hl="true"':''} transform="translate(${dx.toFixed(2)} ${dy.toFixed(2)})"><clipPath id="${clip}"><path d="${g.d}"/></clipPath>${labelSvg(markup(fit,r),'qm-label',attrs,clip)}</g>`;
  }).join('');
}

/** A parent ring's chosen entry, named upright at the top where it was turned to. */
function parentLabel(m){
  const r=m.rows[m.sel]||{};
  const half=180/Math.max(m.shown.length,1);
  const g={r0:m.r0,r1:m.r1,a0:-half+GAP,a1:half-GAP};
  const fit=fitLabel(r.label,{siblings:m.rows.map(x=>x.label),box:g,measure,max:13,min:10});
  const clip=`qm-c${++ids}`;
  return `<clipPath id="${clip}"><path d="${segmentPath(g.r0,g.r1,g.a0,g.a1)}"/></clipPath>${labelSvg(markup(fit,{}),'qm-plabel','',clip)}`;
}

function caption(v,m){
  const a=m.at(-1);
  const r=a.rows[a.sel]||{};
  const pos=a.count>8&&a.sel>0?`<span class="qm-pos">${a.sel} / ${a.count-1}</span>`:'';
  const dot=r.working?'<span class="qm-dot" aria-label="working"></span>':'';
  const detail=r.type==='submenu'&&!r.detail?'':r.detail||(r.current?'current':'');
  const faces=Array.isArray(v.faces)?v.faces:[];
  const face=faces.length>1?`<span class="qm-face">${(v.faceIndex|0)+1}/${faces.length} ${esc(v.face||'')}</span>`:'';
  $('qm-caption').innerHTML=face+(v.note?`<span class="qm-name qm-note">${esc(v.note)}</span>`:a.count?`${dot}<span class="qm-name">${esc(r.label)}</span>${detail?`<span class="qm-detail">${esc(detail)}</span>`:''}${pos}`
    :'<span class="qm-name">This menu is empty. Add entries on the Vibe Key page.</span>');
}

// The animation (about 200 ms): each ring goes from where it was (turn, size) to its new place; a new ring
// grows in, a ring that is no longer shown fades out.
function animate(groups,labels){
  cancelAnimationFrame(frame);
  if(!groups.length){labels.style.opacity='1';return;}
  labels.style.opacity='0';
  const t0=performance.now();
  const ease=t=>1-Math.pow(1-t,3);
  const tick=now=>{
    const t=Math.min((now-t0)/ANIM_MS,1),e=ease(t);
    for(const g of groups){
      const rot=g.from.rot+(g.to.rot-g.from.rot)*e, sc=g.from.scale+(g.to.scale-g.from.scale)*e, op=g.from.opacity+(g.to.opacity-g.from.opacity)*e;
      g.el.setAttribute('transform',`rotate(${rot.toFixed(2)}) scale(${sc.toFixed(4)})`);
      g.el.setAttribute('opacity',op.toFixed(3));
    }
    if(t<1)frame=requestAnimationFrame(tick);
    else{for(const g of groups)if(g.leaving)g.el.remove();labels.style.opacity='1';}
  };
  frame=requestAnimationFrame(tick);
}

/** The twist (1.1.1): the whole dial (rings and labels) turns TWIST degrees in the move's direction and settles. */
let twistFrame=0;
function twist(dir){
  cancelAnimationFrame(twistFrame);
  const k=shakeStrength();
  const els=[...$('qm-dial').querySelectorAll('.qm-rings,.qm-labels')];
  const t0=performance.now();
  $('qm-dial').dataset.twists=String(Number($('qm-dial').dataset.twists||0)+1);
  const tick=now=>{
    const t=Math.min((now-t0)/TWIST_MS,1);
    // Out quickly, back with a soft settle.
    const a=dir*TWIST*k*Math.sin(Math.PI*Math.min(t*1.6,1))*(1-t*0.35)*(t<1?1:0);
    for(const el of els)el.setAttribute('transform',`rotate(${a.toFixed(2)})`);
    if(t<1)twistFrame=requestAnimationFrame(tick);
  };
  twistFrame=requestAnimationFrame(tick);
}

/** The bump (120 ms): the highlighted segment nudges against the end it was turned into, then settles. */
function bump(dir){
  const g=$('qm-dial').querySelector('.qm-ring[data-active="true"] .qm-slot[data-hl="true"]');
  if(!g)return;
  const lab=$('qm-dial').querySelector('.qm-lab[data-hl="true"]');
  const row=Number(g.dataset.row);
  const m=last.rings.get(last.depth);
  const a=m?.angles[m.shown.indexOf(row)];
  if(!a)return;
  const [px,py]=[0,0],[tx,ty]=tangent(a.mid);
  const t0=performance.now(),amp=BUMP_PX*shakeStrength();
  $('qm-dial').dataset.bumps=String(Number($('qm-dial').dataset.bumps||0)+1);
  const tick=now=>{
    const t=Math.min((now-t0)/BUMP_MS,1),k=Math.sin(t*Math.PI)*amp*dir;
    const tr=`translate(${(px+tx*k).toFixed(2)} ${(py+ty*k).toFixed(2)})`;
    g.setAttribute('transform',tr);lab?.setAttribute('transform',tr);
    if(t<1)requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick);
}

/** The list style (1.1.0; 1.2.0, user request 2026-11-19: grouped by project, no Close row, bigger and easier to
 * follow). Rows that carry a `project` get a heading per project; the highlighted row stays near the middle. */
let listSig='', scrollFrame=0;
/** 1.2.1 (user feedback 2026-11-19: scrolling the list "feels a little bit laggy ... lagging behind"): no browser
 * smooth scrolling (it restarted on every step); the list follows the latest target quickly (time constant 45 ms)
 * and is only rebuilt when its rows change. */
function scrollListTo(list,want){
  cancelAnimationFrame(scrollFrame);
  let t0=performance.now();
  const tick=now=>{
    const dt=Math.min(now-t0,50);t0=now;
    const d=want-list.scrollTop;
    if(Math.abs(d)<1){list.scrollTop=want;return;}
    list.scrollTop+=d*(1-Math.exp(-dt/45));
    scrollFrame=requestAnimationFrame(tick);
  };
  scrollFrame=requestAnimationFrame(tick);
}
function listTarget(list){
  const el=list.querySelector('[data-hl]');if(!el)return null;
  // Keep the highlight near the middle (with its heading in view when it is the first of its project).
  const head=el.previousElementSibling?.classList.contains('qm-group')?el.previousElementSibling:null;
  const top=(head||el).offsetTop, want=top-list.clientHeight/2+el.offsetHeight;
  return Math.max(0,Math.min(want,list.scrollHeight-list.clientHeight));
}
function renderList(v){
  const rows=Array.isArray(v.rows)?v.rows:[];
  const sel=Math.max(0,Math.min(v.highlight|0,rows.length-1));
  const list=$('qm-list');
  const first=v.noClose?1:0;
  const sig=JSON.stringify([first,rows.map(r=>[r.label,r.detail,r.project,r.type,r.working])]);
  if(sig===listSig){
    // The same rows: move the highlight only (no rebuild), then scroll quickly to it.
    for(const e of list.querySelectorAll('[data-hl]')){e.removeAttribute('data-hl');e.removeAttribute('aria-selected');}
    const el=list.querySelector(`.qm-row[data-row="${sel}"]`);
    if(el){el.dataset.hl='true';el.setAttribute('aria-selected','true');}
    for(const g of list.querySelectorAll('.qm-group')){
      const a=Number(g.dataset.from),b=Number(g.dataset.to);
      if(sel>=a&&sel<b)g.dataset.current='true';else delete g.dataset.current;
    }
    const want=listTarget(list);if(want!==null)scrollListTo(list,want);
    listHeader(v,rows,sel,first);
    return;
  }
  listSig=sig;
  let html='',group=null;
  const count=rows.length-first;
  for(let i=first;i<rows.length;i++){
    const r=rows[i];
    const project=typeof r.project==='string'?r.project:null;
    if(project!==null&&project!==group){
      group=project;
      const n=rows.slice(i).filter(x=>x.project===project).length;
      const has=sel>=i&&sel<i+n;
      html+=`<li class="qm-group" role="presentation" data-from="${i}" data-to="${i+n}"${has?' data-current="true"':''}><span class="qm-group-name">${esc(project||'Other')}</span><span class="qm-group-count">${n}</span></li>`;
    }
    const glyph=GLYPH[r.type];
    const hl=i===sel?' data-hl="true" aria-selected="true"':'';
    const dot=r.working?'<span class="qm-dot" aria-label="working"></span>':'';
    html+=`<li class="qm-row${project!==null?' qm-in-group':''}" role="option" data-row="${i}" data-kind="${kind(r)}"${hl}>${glyph?`<span class="qm-glyph">${glyph}</span>`:dot}<span class="qm-row-text"><span class="qm-row-name">${esc(r.label)}</span>${r.detail?`<span class="qm-row-detail">${esc(r.detail)}</span>`:''}</span>${r.type==='submenu'?'<span class="qm-glyph">›</span>':''}</li>`;
  }
  list.innerHTML=html;
  // A new list: straight to the highlight (and again once laid out).
  const into=()=>{const want=listTarget(list);if(want!==null)list.scrollTop=want;};
  into();requestAnimationFrame(into);
  listHeader(v,rows,sel,first);
}
function listHeader(v,rows,sel,first){
  const count=rows.length-first;
  const faces=Array.isArray(v.faces)?v.faces:[];
  const face=faces.length>1?`<span class="qm-face">${(v.faceIndex|0)+1}/${faces.length} ${esc(v.face||'')}</span>`:'';
  const title=Array.isArray(v.title)?v.title:[];
  const crumb=title.length===1&&title[0]===v.face&&face?'':esc(title.join(' › '));
  const pos=count>1&&sel>=first?`<span class="qm-list-pos">${sel-first+1} / ${count}</span>`:'';
  $('qm-list-title').innerHTML=`${face}${crumb?`<span class="qm-crumb">${crumb}</span>`:''}${v.note?`<span class="qm-note">${esc(v.note)}</span>`:''}${pos}`;
}

/** The bar runs down from the time left to the idle close (restarted by every input). */
function timer(v){
  const bar=$('qm-bar');
  const total=v.closeAfterMs||(v.closeAfterS||8)*1000, left=Math.max(0,Math.min(v.leftMs??total,total));
  bar.style.transition='none';
  bar.style.transform=`scaleX(${(left/total).toFixed(4)})`;
  void bar.offsetWidth;
  bar.style.transition=`transform ${left}ms linear`;
  bar.style.transform='scaleX(0)';
}

export function render(v){
  if(!v||v.open===false){last={open:false,depth:0,rings:new Map(),bump:last.bump};cancelAnimationFrame(frame);cancelAnimationFrame(flowFrame);flowPos=null;flowKey='';listSig='';return;}
  const style=v.style==='list'?'list':'dial';
  $('qm').dataset.style=style;
  timer(v);
  const faceKey=`${v.faceIndex|0}`;
  if(style==='list'){renderList(v);last={open:false,depth:0,rings:new Map(),bump:v.bump?.n|0,face:faceKey};return;}
  // Another menu (K2): drawn fresh, without the level animation.
  if(last.face!==undefined&&last.face!==faceKey)last={...last,open:false};
  const before=last.open?last.hl:undefined;
  ids=0;
  const m=model(v);
  const d=dialDiameter(m.length);
  // A small work area: the whole dial is drawn smaller to fit the window.
  const k=fitScale(m.length,innerWidth||d,innerHeight||d+96);
  const svg=$('qm-dial');
  svg.setAttribute('viewBox',`${-d/2} ${-d/2} ${d} ${d}`);
  svg.setAttribute('width',(d*k).toFixed(0));svg.setAttribute('height',(d*k).toFixed(0));
  $('qm').style.setProperty('--dial',`${(d*k).toFixed(0)}px`);
  $('qm').style.setProperty('--k',k.toFixed(3));
  const depth=m.at(-1).level;
  // Animate only a change of level while open; reopening where it was left draws the rings in place.
  const moved=last.open&&depth!==last.depth;
  const leaving=moved?[...last.rings.values()].filter(p=>!m.some(r=>r.level===p.level)):[];
  // The window moved on the same level (1.2.0): the rows that pass through are drawn too, and everything flows.
  const act=m.at(-1),prev=last.open&&!moved?last.rings.get(depth):null;
  const key=`${faceKey}${SEP}${(Array.isArray(v.title)?v.title:[]).join(SEP)}`;
  const same=prev&&act.count>MAX_SLOTS&&prev.count===act.count&&key===flowKey&&flowPos!==null;
  const flowFrom=same&&Math.abs(flowPos-act.offset)>1e-3?flowPos:null;
  if(!same){cancelAnimationFrame(flowFrame);flowPos=act.count>MAX_SLOTS?act.offset:null;}
  flowKey=key;
  if(flowFrom!==null){
    // Every row that passes through the visible slots on the way is drawn too.
    const lo=Math.floor(Math.min(flowFrom,act.offset))+1,hi=Math.min(act.count-1,Math.ceil(Math.max(flowFrom,act.offset))+MAX_SLOTS-1);
    act.transit=[];for(let r=lo;r<=hi;r++)if(!act.shown.includes(r))act.transit.push(r);
  }
  svg.innerHTML=`<g class="qm-rings">${leaving.map(ringSvg).join('')}${m.map(ringSvg).join('')}</g><g class="qm-labels" id="qm-labels">${m.filter(r=>!r.active).map(parentLabel).join('')}${m.filter(r=>r.active).map(labelsSvg).join('')}</g>`;
  const groups=[];
  if(moved){
    const els=[...svg.querySelectorAll('.qm-ring')];
    // Deeper: everything spins clockwise as it settles; back up: anticlockwise.
    const spin=depth>last.depth?SPIN:-SPIN;
    leaving.forEach((p,i)=>groups.push({el:els[i],leaving:true,from:{rot:p.rot,scale:1,opacity:1},to:{rot:p.rot+spin,scale:p.level>depth?1.12:0.85,opacity:0}}));
    m.forEach((r,i)=>{
      const p=last.rings.get(r.level);
      const from=p?{rot:p.rot-spin,scale:p.r1/r.r1,opacity:1}:{rot:r.rot-spin,scale:r.level>last.depth?0.85:1.12,opacity:0};
      groups.push({el:els[leaving.length+i],from,to:{rot:r.rot,scale:1,opacity:1}});
    });
  }
  animate(groups,$('qm-labels'));
  caption(v,m);
  const n=v.bump?.n|0, wasOpen=last.open, bumpBefore=last.bump;
  const hl=v.highlight|0;
  last={open:true,depth,rings:new Map(m.map(r=>[r.level,r])),bump:n,hl,face:faceKey};
  // A turn past an end since the last render (not on opening).
  // The window moved: the ring flows (1.2.1: on its own, so a bump at the end never cuts it short).
  if(flowFrom!==null)flow(act,flowFrom);
  if(wasOpen&&n>bumpBefore&&!moved)bump(v.bump.dir>0?1:-1);
  // A move on the same level: the whole dial twists the way it went (right for the next entry).
  else if(flowFrom===null&&wasOpen&&!moved&&before!==undefined&&hl!==before)twist(hl>before?1:-1);
}

// ---- Dragging (0.35.0). Windows' own move loop takes focus from the page while the window is dragged by its
// handle, and the page closed the menu on that blur: the menu vanished as soon as a drag started. Now a drag
// (from the pointer going down on the handle until 300 ms after the last move or the pointer going up) ignores
// blur and keeps the idle close waiting; a plain click on the handle does nothing. ----
export const drag={active:false,until:0,timer:0,safety:0};
const SETTLE_MS=300, SAFETY_MS=2000;
function endDrag(){
  clearTimeout(drag.timer);clearTimeout(drag.safety);
  if(!drag.active)return;
  drag.active=false;drag.until=performance.now()+SETTLE_MS;
  send('hold');
  // Focus back to the menu, so a click elsewhere closes it again.
  if(T&&!document.hasFocus())T.invoke('plugin:window|set_focus',{label:'quickmenu'}).catch(()=>{});
}
export function startDrag(){
  drag.active=true;
  send('hold');
  clearTimeout(drag.safety);drag.safety=setTimeout(endDrag,SAFETY_MS);
}
/** The window moved (the app's event): a drag is going on; it ends 300 ms after the last move. */
export function moved(){
  if(!drag.active)startDrag();
  clearTimeout(drag.safety);drag.safety=setTimeout(endDrag,SAFETY_MS);
  clearTimeout(drag.timer);drag.timer=setTimeout(endDrag,SETTLE_MS);
  const now=performance.now();
  if(!drag.held||now-drag.held>1000){drag.held=now;send('hold');}
}
/** Blur closes the menu (a click on another window), except during a drag and just after it. */
export function blurred(){
  if(drag.active||performance.now()<drag.until)return false;
  send('close');
  return true;
}

if(T){
  T.invoke('plugin:event|listen',{event:'uc:au05-menu',target:{kind:'Any'},handler:T.transformCallback(e=>render(e.payload))}).catch(()=>{});
  T.invoke('plugin:event|listen',{event:'uc:au05-menu-moved',target:{kind:'Any'},handler:T.transformCallback(()=>moved())}).catch(()=>{});
  send('state');
}
document.addEventListener('keydown',e=>{
  const op={ArrowDown:'next',ArrowRight:'next',ArrowUp:'prev',ArrowLeft:'back',Enter:'choose',' ':'choose',Backspace:'back',Escape:'close',Tab:'nextMenu'}[e.key];
  if(op){e.preventDefault();send(op);}
});
// Clicking a segment of the current ring chooses it; the wheel over the ring moves the highlight.
$('qm-dial').addEventListener('click',e=>{const seg=e.target.closest?.('.qm-ring[data-active="true"] [data-row]');if(seg)send('pick',Number(seg.dataset.row));});
$('qm-dial').addEventListener('wheel',e=>{e.preventDefault();if(e.deltaY)send(e.deltaY>0?'next':'prev');},{passive:false});
$('qm-handle').addEventListener('pointerdown',()=>startDrag());
// The list style (1.1.0): a click chooses a row, the wheel moves, the header drags the window.
$('qm-list').addEventListener('click',e=>{const row=e.target.closest?.('.qm-row[data-row]');if(row)send('pick',Number(row.dataset.row));});
$('qm-list').addEventListener('wheel',e=>{e.preventDefault();if(e.deltaY)send(e.deltaY>0?'next':'prev');},{passive:false});
$('qm-list-title').addEventListener('pointerdown',()=>startDrag());
window.addEventListener('pointerup',()=>{if(drag.active){clearTimeout(drag.timer);drag.timer=setTimeout(endDrag,SETTLE_MS);}});
window.addEventListener('blur',()=>blurred());
