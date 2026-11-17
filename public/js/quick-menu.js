// Vibe Key quick menu window (0.34.0: a dial). Shows the core's menu view and sends input back (au05_menu).
// The core owns the state: knob turns, the knob press and the menu key arrive there directly; this page adds
// the keyboard (arrows, Enter, Backspace, Esc) and the mouse (click a segment, wheel over the ring).
//
// Each menu level is a thick ring of equal segments; row 0 (Close at the top, Back in a sub-menu) is at
// 12 o'clock and the others follow clockwise. Going into a level turns the chosen segment to the top of its
// (now dimmed) ring and the new level appears as a larger ring around it; at most 3 rings show. The hole in
// the middle is the drag handle.
import {PUSH,ringRadii,dialSize,slotAngles,windowOffset,slotRows,segmentPath,point,labelChars,shorten} from './quick-menu-dial.js';

const T=window.__TAURI_INTERNALS__;
const send=(op,index)=>T?T.invoke('au05_menu',{input:index===undefined?{op}:{op,index}}).then(render).catch(()=>{}):Promise.resolve();
const esc=s=>String(s??'').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
const $=id=>document.getElementById(id);
const SEP='␟', ANIM_MS=200, GAP=1.2;
const GLYPH={close:'✕',back:'‹'};

// Per level (by its title path): the first row shown after row 0 when the list is longer than the ring.
const offsets=new Map();
// The last render: open, depth and each ring's model by level (for the animation).
let last={open:false,depth:0,rings:new Map()};
let frame=0;

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
    return {...ring,level,count,offset,shown,angles,slot,r0:radii[k][0],r1:radii[k][1],rot:ring.active?0:-(angles[slot]?.mid||0)};
  });
}

const kind=r=>r?.type==='close'||r?.type==='back'?'nav':r?.type==='submenu'?'sub':'item';

function ringSvg(m){
  const gap=m.shown.length>1?GAP:0;
  const segs=m.shown.map((row,i)=>{
    const a=m.angles[i],r=m.rows[row]||{};
    const hl=row===m.sel;
    const [dx,dy]=hl&&m.active?point(PUSH,a.mid):[0,0];
    return `<path class="qm-seg" data-kind="${kind(r)}" data-row="${row}"${hl?' data-hl="true"':''}${r.current?' data-current="true"':''} d="${segmentPath(m.r0,m.r1,a.start+gap,a.end-gap)}" transform="translate(${dx.toFixed(2)} ${dy.toFixed(2)})"><title>${esc(r.label)}</title></path>`;
  }).join('');
  return `<g class="qm-ring" data-level="${m.level}" data-active="${m.active}" transform="rotate(${m.rot.toFixed(2)})">${segs}</g>`;
}

function labelsSvg(m){
  const mid=(m.r0+m.r1)/2;
  if(!m.active){
    // A parent ring: only the chosen entry's name, at the top where it was turned to.
    const r=m.rows[m.sel]||{};
    const [x,y]=point(mid,0);
    return `<text class="qm-plabel" x="${x.toFixed(1)}" y="${y.toFixed(1)}">${esc(shorten(r.label,labelChars(m.r0,m.r1,6)))}</text>`;
  }
  const chars=labelChars(m.r0,m.r1,m.shown.length);
  return m.shown.map((row,i)=>{
    const a=m.angles[i],r=m.rows[row]||{};
    const hl=row===m.sel;
    const [x,y]=point(mid+(hl?PUSH:0),a.mid);
    const glyph=GLYPH[r.type];
    const text=glyph?`${glyph} ${shorten(r.label,Math.max(chars-2,2))}`:shorten(r.label,chars);
    const dot=r.working?`<tspan class="qm-wdot">● </tspan>`:'';
    const more=r.type==='submenu'?' ›':'';
    return `<text class="qm-label" data-kind="${kind(r)}"${hl?' data-hl="true"':''} x="${x.toFixed(1)}" y="${y.toFixed(1)}">${dot}${esc(text)}${more}</text>`;
  }).join('');
}

function caption(v,m){
  const a=m.at(-1);
  const r=a.rows[a.sel]||{};
  const pos=a.count>12&&a.sel>0?`<span class="qm-pos">${a.sel} / ${a.count-1}</span>`:'';
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

export function render(v){
  if(!v||v.open===false){last={open:false,depth:0,rings:new Map()};cancelAnimationFrame(frame);return;}
  const m=model(v);
  const {width}=dialSize(m.length);
  const size=width-0, d=2*(m.at(-1).r1+PUSH+12);
  const svg=$('qm-dial');
  svg.setAttribute('viewBox',`${-d/2} ${-d/2} ${d} ${d}`);
  svg.setAttribute('width',d);svg.setAttribute('height',d);
  $('qm').style.setProperty('--dial',`${d}px`);
  $('qm').style.width=`${size}px`;
  const depth=m.at(-1).level;
  // Animate only a change of level while open; reopening where it was left draws the rings in place.
  const moved=last.open&&depth!==last.depth;
  const leaving=moved?[...last.rings.values()].filter(p=>!m.some(r=>r.level===p.level)):[];
  svg.innerHTML=`<g class="qm-rings">${leaving.map(p=>ringSvg({...p,active:p.active})).join('')}${m.map(ringSvg).join('')}</g><g class="qm-labels" id="qm-labels">${m.map(labelsSvg).join('')}</g>`;
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
  last={open:true,depth,rings:new Map(m.map(r=>[r.level,r]))};
}

if(T){
  T.invoke('plugin:event|listen',{event:'uc:au05-menu',target:{kind:'Any'},handler:T.transformCallback(e=>render(e.payload))}).catch(()=>{});
  send('state');
  document.addEventListener('keydown',e=>{
    const op={ArrowDown:'next',ArrowRight:'next',ArrowUp:'prev',ArrowLeft:'back',Enter:'choose',' ':'choose',Backspace:'back',Escape:'close'}[e.key];
    if(op){e.preventDefault();send(op);}
  });
  // Clicking a segment of the current ring chooses it; the wheel over the ring moves the highlight.
  $('qm-dial').addEventListener('click',e=>{const seg=e.target.closest?.('.qm-ring[data-active="true"] [data-row]');if(seg)send('pick',Number(seg.dataset.row));});
  $('qm-dial').addEventListener('wheel',e=>{e.preventDefault();if(e.deltaY)send(e.deltaY>0?'next':'prev');},{passive:false});
  // Clicking another window closes the menu.
  window.addEventListener('blur',()=>send('close'));
}
