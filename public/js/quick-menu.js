// Vibe Key quick menu window. Shows the core's menu view and sends input back (au05_menu). The core owns the
// state: knob turns, the knob press and the keys arrive there directly; this page adds the keyboard (arrows,
// Enter, Backspace, Esc) and the mouse (click a segment or row, the wheel).
//
// 1.0 (user requests 2026-11-18): several menus, K2 shows the next one. Two styles:
// - "dial": one helix of segments (quick-menu-dial.js, `helix*`). The highlighted row is at 12 o'clock; the rows
//   after it go anticlockwise and deeper (smaller), the ones before it are larger and fade out. A turn moves the
//   whole spiral: it turns and grows one step, so the next row comes up from the depth. Going into a sub-menu the
//   spiral spins and sinks into the middle while the new level spins in from outside ("drilling down"); Back does
//   the reverse. No end marker: a long list goes deeper.
// - "list": a plain drop list, the highlighted row kept in view.
// A thin bar under the caption runs down to the idle close; the hint text is gone.
import {helixDiameter,helixSegmentPath,helixBox,helixPlace,centroid,fitLabel,HELIX_STEP} from './quick-menu-dial.js';

const T=window.__TAURI_INTERNALS__;
const send=(op,index)=>T?T.invoke('au05_menu',{input:index===undefined?{op}:{op,index}}).then(render).catch(()=>{}):Promise.resolve();
const esc=s=>String(s??'').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
const $=id=>document.getElementById(id);
const MOVE_MS=200, LEVEL_MS=460, TURN=360/HELIX_STEP;
const GLYPH={close:'✕',back:'‹'};

const ctx=document.createElement('canvas').getContext('2d');
const FONT=getComputedStyle(document.documentElement).getPropertyValue('--font').trim()||'"Segoe UI", system-ui, sans-serif';
export function measure(text,size){ctx.font=`600 ${size}px ${FONT}`;return ctx.measureText(text).width;}

const kind=r=>r?.type==='close'||r?.type==='back'?'nav':r?.type==='submenu'?'sub':'item';
const SEG=helixSegmentPath(), BOX=helixBox(), [CX,CY]=centroid(BOX);

// The last render: style, level key, the highlight's position (a fraction while it moves) and the live layers.
let last={open:false,style:'',key:'',pos:0,face:-1};
let frame=0;

/** A row's label fitted for where its segment will rest (`rot`, degrees): upright text in a segment turned to
 * the side needs other line breaks than at 12 o'clock. Drawn at the canonical centre (the label is turned back
 * around it), so it lands upright at the segment's middle. */
function labelFor(r,rot,siblings){
  const glyph=GLYPH[r.type];
  const box={...BOX,a0:BOX.a0+rot,a1:BOX.a1+rot};
  const fit=fitLabel(r.label,{siblings:glyph?[]:siblings,box,measure,prefix:glyph||(r.working?'●':''),suffix:r.type==='submenu'?'›':''});
  const top=CY-(fit.lines.length*fit.lh)/2+fit.lh/2;
  const spans=fit.lines.map((l,j)=>{let s=esc(l);if(j===0&&r.working)s=s.replace(/^●/,'<tspan class="qm-wdot">●</tspan>');return `<tspan x="${CX.toFixed(1)}" y="${(top+j*fit.lh).toFixed(1)}">${s}</tspan>`;}).join('');
  return {spans,size:fit.size,short:fit.short};
}
const siblingsOf=rows=>rows.filter(r=>r.type!=='close'&&r.type!=='back').map(r=>r.label);
/** Fit the labels that show for this highlight (re-done after each move). */
function fitLabels(level,rows,sel){
  const sib=siblingsOf(rows);
  for(const t of level.querySelectorAll('.qm-label')){
    const i=Number(t.dataset.row),p=helixPlace(i-sel);
    if(!p||!p.label)continue;
    const rot=((p.rot%360)+360)%360, f=labelFor(rows[i]||{},rot>180?rot-360:rot,sib);
    t.innerHTML=f.spans;t.style.fontSize=`${f.size}px`;
    if(f.short)t.dataset.short='true';else delete t.dataset.short;
    t.dataset.rot=String(p.rot);
  }
}

/** One level of the helix as SVG: every row once, placed later by `place`. */
function levelSvg(rows,sel,cls){
  const parts=rows.map((r,i)=>{
    const hl=i===sel?' data-hl="true"':'';
    return `<g class="qm-slot" data-row="${i}"${hl}><path class="qm-seg" data-kind="${kind(r)}" data-row="${i}"${hl}${r.current?' data-current="true"':''} d="${SEG}"><title>${esc(r.label)}</title></path>`+
      `<text class="qm-label" data-kind="${kind(r)}" data-row="${i}"${hl}></text></g>`;
  }).join('');
  return `<g class="qm-level ${cls}">${parts}</g>`;
}

/** Put every row of a level where the helix has it for this position (`pos`: the highlight, a fraction while
 * moving); `fade` multiplies the opacity (a level spinning in or out). */
function place(level,pos,fade=1){
  for(const g of level.querySelectorAll('.qm-slot')){
    const p=helixPlace(Number(g.dataset.row)-pos);
    if(!p||fade<=0.001){g.setAttribute('display','none');continue;}
    g.removeAttribute('display');
    g.setAttribute('transform',`rotate(${p.rot.toFixed(2)}) scale(${p.scale.toFixed(4)})`);
    g.setAttribute('opacity',(p.opacity*fade).toFixed(3));
    const t=g.querySelector('text');
    // Labels stay upright: turned back around their own centre.
    t.setAttribute('transform',`rotate(${(-p.rot).toFixed(2)} ${CX.toFixed(1)} ${CY.toFixed(1)})`);
    t.setAttribute('display',p.label?'inline':'none');
  }
}

const ease=t=>1-Math.pow(1-t,3);
function run(ms,step){
  cancelAnimationFrame(frame);
  const t0=performance.now();
  const tick=now=>{const t=Math.min((now-t0)/ms,1);step(ease(t),t>=1);if(t<1)frame=requestAnimationFrame(tick);};
  frame=requestAnimationFrame(tick);
}

function renderDial(v,rows,sel,key){
  const d=helixDiameter();
  const k=Math.min(1,(innerWidth||d)/d,Math.max((innerHeight||d+90)-90,1)/d);
  const svg=$('qm-dial');
  svg.setAttribute('viewBox',`${-d/2} ${-d/2} ${d} ${d}`);
  svg.setAttribute('width',(d*k).toFixed(0));svg.setAttribute('height',(d*k).toFixed(0));
  $('qm').style.setProperty('--dial',`${(d*k).toFixed(0)}px`);
  $('qm').style.setProperty('--k',k.toFixed(3));
  const same=last.open&&last.style==='dial'&&last.key===key;
  const deeper=last.open&&last.style==='dial'&&!same&&key.startsWith(last.key+'␟');
  const up=last.open&&last.style==='dial'&&!same&&last.key.startsWith(key+'␟');
  const old=svg.querySelector('.qm-level.qm-now');
  if(same){
    // A move: the whole spiral turns from where it was to the new highlight.
    const from=last.pos;
    for(const g of svg.querySelectorAll('.qm-now [data-hl]'))g.removeAttribute('data-hl');
    for(const g of svg.querySelectorAll(`.qm-now [data-row="${sel}"]`))g.setAttribute('data-hl','true');
    const level=svg.querySelector('.qm-now');
    svg.querySelectorAll('.qm-out').forEach(e=>e.remove());
    fitLabels(level,rows,sel);
    run(MOVE_MS,(e)=>{last.pos=from+(sel-from)*e;place(level,last.pos);});
    last.pos=from;
    return;
  }
  // A new level (or the first draw): build it; when it is a level change, drill.
  if(old){old.classList.remove('qm-now');old.classList.add('qm-out');}
  svg.querySelectorAll('.qm-out').forEach(e=>{if(e!==old)e.remove();});
  svg.insertAdjacentHTML('beforeend',levelSvg(rows,sel,'qm-now'));
  const level=svg.querySelector('.qm-level.qm-now');
  fitLabels(level,rows,sel);
  if(deeper||up){
    // Into a sub-menu: the old level spins a turn deeper and fades; the new one spins in from outside. Back: the
    // reverse (the old level rises out, the parent comes up from the depth).
    const dir=deeper?1:-1, oldPos=last.pos;
    $('qm-dial').dataset.drills=String(Number($('qm-dial').dataset.drills||0)+1);
    run(LEVEL_MS,(e,done)=>{
      if(old)place(old,oldPos-dir*TURN*e,1-e);
      place(level,sel+dir*TURN*(1-e),e);
      if(done)old?.remove();
    });
  }else{
    old?.remove();
    place(level,sel);
  }
  last.pos=sel;
}

function renderList(v,rows,sel){
  const list=$('qm-list');
  list.innerHTML=rows.map((r,i)=>{
    const glyph=GLYPH[r.type];
    const hl=i===sel?' data-hl="true" aria-selected="true"':'';
    const dot=r.working?'<span class="qm-dot" aria-label="working"></span>':'';
    return `<li class="qm-row" role="option" data-row="${i}" data-kind="${kind(r)}"${hl}>${glyph?`<span class="qm-glyph">${glyph}</span>`:dot}<span class="qm-row-name">${esc(r.label)}</span>${r.detail?`<span class="qm-row-detail">${esc(r.detail)}</span>`:''}${r.type==='submenu'?'<span class="qm-glyph">›</span>':''}</li>`;
  }).join('');
  list.querySelector('[data-hl]')?.scrollIntoView({block:'nearest'});
}

function caption(v,rows,sel){
  const r=rows[sel]||{};
  const faces=Array.isArray(v.faces)?v.faces:[];
  const face=faces.length>1?`<span class="qm-face">${(v.faceIndex|0)+1}/${faces.length} ${esc(v.face||'')}</span>`:'';
  const dot=r.working?'<span class="qm-dot" aria-label="working"></span>':'';
  const detail=r.type==='submenu'&&!r.detail?'':r.detail||(r.current?'current':'');
  const pos=rows.length>8&&sel>0?`<span class="qm-pos">${sel} / ${rows.length-1}</span>`:'';
  const body=v.note?`<span class="qm-name qm-note">${esc(v.note)}</span>`:rows.length>1||rows[0]?.type!=='close'
    ?`${dot}<span class="qm-name">${esc(r.label)}</span>${detail?`<span class="qm-detail">${esc(detail)}</span>`:''}${pos}`
    :'<span class="qm-name">This menu is empty. Add entries on the Vibe Key page.</span>';
  $('qm-caption').innerHTML=face+body;
  const title=Array.isArray(v.title)?v.title:[];
  const crumb=title.length===1&&title[0]===v.face&&face?'':esc(title.join(' › '));
  $('qm-list-title').innerHTML=`${face}${crumb?`<span class="qm-crumb">${crumb}</span>`:''}`;
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
  if(!v||v.open===false){last={open:false,style:'',key:'',pos:0,face:-1};cancelAnimationFrame(frame);return;}
  const rows=Array.isArray(v.rows)?v.rows:[];
  const sel=Math.max(0,Math.min(v.highlight|0,rows.length-1));
  const style=v.style==='list'?'list':'dial';
  const title=Array.isArray(v.title)?v.title:['Quick menu'];
  const key=`${v.faceIndex|0}␟${title.join('␟')}`;
  $('qm').dataset.style=style;
  if(last.style!==style||last.face!==(v.faceIndex|0)){
    // Another menu (K2): drawn fresh.
    $('qm-dial').innerHTML='';last.open=false;
  }
  if(style==='dial')renderDial(v,rows,sel,key);else renderList(v,rows,sel);
  caption(v,rows,sel);
  timer(v);
  last={...last,open:true,style,key,face:v.faceIndex|0};
}

// ---- Dragging (0.35.0). Windows' own move loop takes focus from the page while the window is dragged by its
// handle, and the page closed the menu on that blur. A drag (from the pointer going down on the handle until
// 300 ms after the last move or the pointer going up) ignores blur and keeps the idle close waiting. ----
export const drag={active:false,until:0,timer:0,safety:0};
const SETTLE_MS=300, SAFETY_MS=2000;
function endDrag(){
  clearTimeout(drag.timer);clearTimeout(drag.safety);
  if(!drag.active)return;
  drag.active=false;drag.until=performance.now()+SETTLE_MS;
  send('hold');
  if(T&&!document.hasFocus())T.invoke('plugin:window|set_focus',{label:'quickmenu'}).catch(()=>{});
}
export function startDrag(){
  drag.active=true;
  send('hold');
  clearTimeout(drag.safety);drag.safety=setTimeout(endDrag,SAFETY_MS);
}
export function moved(){
  if(!drag.active)startDrag();
  clearTimeout(drag.safety);drag.safety=setTimeout(endDrag,SAFETY_MS);
  clearTimeout(drag.timer);drag.timer=setTimeout(endDrag,SETTLE_MS);
  const now=performance.now();
  if(!drag.held||now-drag.held>1000){drag.held=now;send('hold');}
}
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
const pick=e=>{const el=e.target.closest?.('.qm-now [data-row], .qm-row[data-row]');if(el)send('pick',Number(el.dataset.row));};
$('qm-dial').addEventListener('click',pick);
$('qm-list').addEventListener('click',pick);
for(const el of [$('qm-dial'),$('qm-list')])el.addEventListener('wheel',e=>{e.preventDefault();if(e.deltaY)send(e.deltaY>0?'next':'prev');},{passive:false});
for(const el of [$('qm-handle'),$('qm-list-title')])el.addEventListener('pointerdown',()=>startDrag());
window.addEventListener('pointerup',()=>{if(drag.active){clearTimeout(drag.timer);drag.timer=setTimeout(endDrag,SETTLE_MS);}});
window.addEventListener('blur',()=>blurred());
