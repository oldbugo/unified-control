// Vibe Key quick menu window (0.34.0: a dial; 0.35.0: bigger, curved labels that fit, an end stop, a bump at
// the ends, dragging that no longer closes it). Shows the core's menu view and sends input back (au05_menu).
// The core owns the state: knob turns, the knob press and the menu key arrive there directly; this page adds
// the keyboard (arrows, Enter, Backspace, Esc) and the mouse (click a segment, wheel over the ring).
//
// Each menu level is a thick ring of segments; row 0 (Close at the top, Back in a sub-menu) is at 12 o'clock and
// the others follow clockwise, with a gap and a stop on row 0's anticlockwise side where the list ends. Going
// into a level turns the chosen segment to the top of its (now dimmed) ring and the new level appears as a larger
// ring around it; at most 3 rings show. The filled hole in the middle is the drag handle.
import {PUSH,STEP,STOP_ANGLE,ringRadii,dialDiameter,fitScale,slotAngles,windowOffset,slotRows,edges,segmentPath,
  point,tangent,textArc,fitLabel} from './quick-menu-dial.js';

const T=window.__TAURI_INTERNALS__;
const send=(op,index)=>T?T.invoke('au05_menu',{input:index===undefined?{op}:{op,index}}).then(render).catch(()=>{}):Promise.resolve();
const esc=s=>String(s??'').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
const $=id=>document.getElementById(id);
const SEP='␟', ANIM_MS=200, BUMP_MS=120, GAP=1.2;
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
    return {...ring,level,count,offset,shown,angles,slot,edges:edges(count,offset),r0:radii[k][0],r1:radii[k][1],rot:ring.active?0:-(angles[slot]?.mid||0)};
  });
}

const kind=r=>r?.type==='close'||r?.type==='back'?'nav':r?.type==='submenu'?'sub':'item';

/** A label along the segment's arc (lines, size, radii from `fitLabel`), clipped to the segment. */
function labelSvg(fit,a,gap,cls,attrs,clip){
  const id=`qm-a${++ids}`;
  const defs=fit.lines.map((_,j)=>`<path id="${id}-${j}" d="${textArc(fit.radii[j],a.start+gap,a.end-gap)}"/>`).join('');
  const text=fit.lines.map((line,j)=>`<text class="${cls}" style="font-size:${fit.size}px"${attrs}><textPath href="#${id}-${j}" startOffset="50%">${line}</textPath></text>`).join('');
  return `<defs>${defs}</defs><g clip-path="url(#${clip})">${text}</g>`;
}
/** A label's lines with the working dot and the sub-menu mark as marked-up spans. */
function markup(fit,r){
  return {...fit,lines:fit.lines.map((l,j)=>{
    let s=esc(l);
    if(j===0&&r.working)s=s.replace(/^●/,'<tspan class="qm-wdot">●</tspan>');
    return s;
  })};
}

function ringSvg(m){
  const gap=GAP;
  const siblings=m.rows.filter(r=>r.type!=='close'&&r.type!=='back').map(r=>r.label);
  const parts=m.shown.map((row,i)=>{
    const a=m.angles[i],r=m.rows[row]||{};
    const hl=row===m.sel;
    const [dx,dy]=hl&&m.active?point(PUSH,a.mid):[0,0];
    const clip=`qm-c${++ids}`;
    const d=segmentPath(m.r0,m.r1,a.start+gap,a.end-gap);
    // The "more" fade on an edge segment whose neighbours in the list are not shown.
    const fade=!hl&&((i===m.shown.length-1&&m.edges.moreAfter)||(i===1&&m.edges.moreBefore));
    let fill='';
    if(fade){
      const toEnd=i===m.shown.length-1;
      const rm=(m.r0+m.r1)/2,[x0,y0]=point(rm,a.mid),[x1,y1]=point(rm,toEnd?a.end:a.start);
      fill=`<linearGradient id="${clip}-g" gradientUnits="userSpaceOnUse" x1="${x0.toFixed(1)}" y1="${y0.toFixed(1)}" x2="${x1.toFixed(1)}" y2="${y1.toFixed(1)}"><stop offset="0" class="qm-fade0"/><stop offset="1" class="qm-fade1"/></linearGradient>`;
    }
    let label='';
    if(m.active){
      const glyph=GLYPH[r.type];
      const box={r0:m.r0,r1:m.r1,deg:STEP-2*gap,mid:a.mid};
      const fit=fitLabel(r.label,{siblings:r.type==='close'||r.type==='back'?[]:siblings,box,measure,
        prefix:glyph||(r.working?'●':''),suffix:r.type==='submenu'?'›':''});
      label=labelSvg(markup(fit,r),a,gap,'qm-label',` data-kind="${kind(r)}"${hl?' data-hl="true"':''}${fit.short?' data-short="true"':''}`,clip);
    }
    return `<g class="qm-slot" data-row="${row}"${hl?' data-hl="true"':''} transform="translate(${dx.toFixed(2)} ${dy.toFixed(2)})"><clipPath id="${clip}"><path d="${d}"/></clipPath>${fill}<path class="qm-seg${fade?' qm-more':''}" data-kind="${kind(r)}" data-row="${row}"${hl?' data-hl="true"':''}${r.current?' data-current="true"':''}${fade?` fill="url(#${clip}-g)"`:''} d="${d}"><title>${esc(r.label)}</title></path>${label}</g>`;
  }).join('');
  // The end stop: a wall across the ring on row 0's anticlockwise side, at the list's true end only.
  let stop='';
  if(m.edges.stop&&m.active){
    const [x0,y0]=point(m.r0-4,STOP_ANGLE),[x1,y1]=point(m.r1+4,STOP_ANGLE);
    stop=`<line class="qm-stop" x1="${x0.toFixed(1)}" y1="${y0.toFixed(1)}" x2="${x1.toFixed(1)}" y2="${y1.toFixed(1)}"/>`;
  }
  return `<g class="qm-ring" data-level="${m.level}" data-active="${m.active}" transform="rotate(${m.rot.toFixed(2)})">${parts}${stop}</g>`;
}

/** A parent ring's chosen entry, named at the top where it was turned to (outside the turning group). */
function parentLabel(m){
  const r=m.rows[m.sel]||{};
  const a={start:-STEP/2,mid:0,end:STEP/2};
  const fit=fitLabel(r.label,{siblings:m.rows.map(x=>x.label),box:{r0:m.r0,r1:m.r1,deg:STEP-2*GAP,mid:0},measure,max:13});
  const clip=`qm-c${++ids}`;
  return `<clipPath id="${clip}"><path d="${segmentPath(m.r0,m.r1,a.start,a.end)}"/></clipPath>${labelSvg(markup(fit,{}),a,GAP,'qm-plabel','',clip)}`;
}

function caption(v,m){
  const a=m.at(-1);
  const r=a.rows[a.sel]||{};
  const pos=a.count>8&&a.sel>0?`<span class="qm-pos">${a.sel} / ${a.count-1}</span>`:'';
  const dot=r.working?'<span class="qm-dot" aria-label="working"></span>':'';
  const detail=r.type==='submenu'&&!r.detail?'':r.detail||(r.current?'current':'');
  $('qm-caption').innerHTML=a.count?`${dot}<span class="qm-name">${esc(r.label)}</span>${detail?`<span class="qm-detail">${esc(detail)}</span>`:''}${pos}`
    :'<span class="qm-name">This menu is empty. Add entries on the Vibe Key page.</span>';
  const foot=$('qm-foot');
  foot.textContent=v.note||`${v.hints||'Turn: move'} · ${v.closeAfterS||8} s`;
  foot.classList.toggle('note',!!v.note);
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

/** The bump (120 ms): the highlighted segment nudges against the end it was turned into, then settles. */
function bump(dir){
  const g=$('qm-dial').querySelector('.qm-ring[data-active="true"] .qm-slot[data-hl="true"]');
  if(!g)return;
  const row=Number(g.dataset.row);
  const m=last.rings.get(last.depth);
  const a=m?.angles[m.shown.indexOf(row)];
  if(!a)return;
  const [px,py]=point(PUSH,a.mid),[tx,ty]=tangent(a.mid);
  const t0=performance.now();
  $('qm-dial').dataset.bumps=String(Number($('qm-dial').dataset.bumps||0)+1);
  const tick=now=>{
    const t=Math.min((now-t0)/BUMP_MS,1),k=Math.sin(t*Math.PI)*6*dir;
    g.setAttribute('transform',`translate(${(px+tx*k).toFixed(2)} ${(py+ty*k).toFixed(2)})`);
    if(t<1)requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick);
}

export function render(v){
  if(!v||v.open===false){last={open:false,depth:0,rings:new Map(),bump:last.bump};cancelAnimationFrame(frame);return;}
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
  svg.innerHTML=`<g class="qm-rings">${leaving.map(ringSvg).join('')}${m.map(ringSvg).join('')}</g><g class="qm-labels" id="qm-labels">${m.filter(r=>!r.active).map(parentLabel).join('')}</g>`;
  const groups=[];
  if(moved){
    const els=[...svg.querySelectorAll('.qm-ring')];
    leaving.forEach((p,i)=>groups.push({el:els[i],leaving:true,from:{rot:p.rot,scale:1,opacity:1},to:{rot:p.rot,scale:p.level>depth?1.12:0.85,opacity:0}}));
    m.forEach((r,i)=>{
      const p=last.rings.get(r.level);
      const from=p?{rot:p.rot,scale:p.r1/r.r1,opacity:1}:{rot:r.rot,scale:r.level>last.depth?0.85:1.12,opacity:0};
      groups.push({el:els[leaving.length+i],from,to:{rot:r.rot,scale:1,opacity:1}});
    });
  }
  animate(groups,$('qm-labels'));
  caption(v,m);
  const n=v.bump?.n|0, wasOpen=last.open, before=last.bump;
  last={open:true,depth,rings:new Map(m.map(r=>[r.level,r])),bump:n};
  // A turn past an end since the last render (not on opening).
  if(wasOpen&&n>before&&!moved)bump(v.bump.dir>0?1:-1);
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
  const op={ArrowDown:'next',ArrowRight:'next',ArrowUp:'prev',ArrowLeft:'back',Enter:'choose',' ':'choose',Backspace:'back',Escape:'close'}[e.key];
  if(op){e.preventDefault();send(op);}
});
// Clicking a segment of the current ring chooses it; the wheel over the ring moves the highlight.
$('qm-dial').addEventListener('click',e=>{const seg=e.target.closest?.('.qm-ring[data-active="true"] [data-row]');if(seg)send('pick',Number(seg.dataset.row));});
$('qm-dial').addEventListener('wheel',e=>{e.preventDefault();if(e.deltaY)send(e.deltaY>0?'next':'prev');},{passive:false});
$('qm-handle').addEventListener('pointerdown',()=>startDrag());
window.addEventListener('pointerup',()=>{if(drag.active){clearTimeout(drag.timer);drag.timer=setTimeout(endDrag,SETTLE_MS);}});
window.addEventListener('blur',()=>blurred());
