// Geometry and label fitting of the Vibe Key quick menu dial (0.34.0; 0.35.0: bigger, 8 slots; 0.37.0: the
// rows share the whole ring, a thicker band, a taper as the end cue, upright labels). Pure functions, no DOM
// (text widths come from a `measure` function): checked by scripts/check-quick-menu.cjs. Angles are degrees
// clockwise from 12 o'clock.

/** At most this many segments per ring, Close/Back included; longer lists become a moving window. */
export const MAX_SLOTS=8;
/** How far the highlighted segment is pushed out (px). */
export const PUSH=10;
/** The taper (the end cue, user feedback 2026-11-17): Close/Back reaches this much further out than the band,
 * the next segments step down to the band, and the last one sits this much inside it ("just a notch"). */
export const TAPER_FIRST=12, TAPER_NEXT=6, TAPER_LAST=-9;
/** The label font: from the normal size down to a readable minimum (px). */
export const FONT_MAX=15, FONT_MIN=11;
/** A single word too long for its segment may go this small before it is split with a hyphen. */
export const WORD_MIN=8;
/** At most this many lines per label. */
export const MAX_LINES=3;
const PAD=14, MIN_WIDTH=380, CAPTION=96;
/** Inner and outer radius of each ring, oldest (innermost) first, for 1, 2 or 3 rings shown. The active (outer)
 * ring is about 1.5 times the 0.35.0 band (110 → 165, 86 → 128, 94 → 141) and grows per level (225, 254,
 * 305); the dimmed parent rings fit inside it. */
const RINGS=[[[60,225]],[[60,118],[126,254]],[[60,100],[106,156],[164,305]]];

export function ringRadii(count){return RINGS[Math.min(Math.max(count|0,1),3)-1];}

/** The window's logical size for this many rings (src-tauri/src/quick_menu.rs `dial_size` matches it). */
export function dialSize(rings){
  const d=dialDiameter(rings);
  return {width:Math.max(d,MIN_WIDTH),height:d+CAPTION};
}
export function dialDiameter(rings){return 2*(ringRadii(rings).at(-1)[1]+TAPER_FIRST+PUSH+PAD);}
/** The dial's drawn size when the window is smaller than wanted (a small work area): scaled down to fit. */
export function fitScale(rings,width,height){
  const d=dialDiameter(rings);
  return Math.min(1,width/d,Math.max(height-CAPTION,1)/d);
}

/** Each slot's start, middle and end angle: the n rows share the whole ring (360 / n each), slot 0 centred
 * at 12 o'clock, the others clockwise. */
export function slotAngles(n){
  const step=360/Math.max(n,1);
  return Array.from({length:Math.max(n,1)},(_,i)=>({start:i*step-step/2,mid:i*step,end:i*step+step/2}));
}

/** The taper: how far each visible slot's outer edge sits from the band's outer radius. Slot 0 is the
 * largest, the next ones step down evenly to the band, the last one is a notch inside it. */
export function taper(n){
  if(n<=1)return [TAPER_FIRST];
  return Array.from({length:n},(_,i)=>i===0?TAPER_FIRST:i===n-1?TAPER_LAST:n<=3?0:TAPER_NEXT*(1-(i-1)/(n-3)));
}
/** The subtle shading toward the end of the list: 0 at the start, 1 at the last slot. */
export function shade(n,i){return n<=1?0:i/(n-1);}

/** The first list row shown after row 0 is `offset + 1`. The window moves only when the highlight reaches an
 * edge; it never wraps. */
export function windowOffset(count,highlight,previous=0){
  if(count<=MAX_SLOTS)return 0;
  const max=count-MAX_SLOTS;
  let o=Math.min(Math.max(previous|0,0),max);
  if(highlight>0){
    if(highlight<o+1)o=highlight-1;
    else if(highlight>o+MAX_SLOTS-1)o=highlight-(MAX_SLOTS-1);
  }
  return Math.min(Math.max(o,0),max);
}

/** The row shown in each slot. */
export function slotRows(count,offset){
  if(count<=MAX_SLOTS)return Array.from({length:count},(_,i)=>i);
  return [0,...Array.from({length:MAX_SLOTS-1},(_,i)=>offset+1+i)];
}
/** Whether rows are hidden before or after the visible window ("more" fades at the edges). */
export function edges(count,offset){
  const shown=slotRows(count,offset);
  return {moreBefore:count>MAX_SLOTS&&offset>0,moreAfter:shown.at(-1)<count-1};
}

/** The highlight after a move (no wrap-around: it stops at Close/Back and at the last row). `bump`: it pushed
 * against an end. */
export function step(count,highlight,delta){
  const to=Math.min(Math.max(highlight+delta,0),Math.max(count-1,0));
  return {to,bump:to===highlight&&delta!==0};
}

const rad=a=>(a-90)*Math.PI/180;
export function point(r,a){return [r*Math.cos(rad(a)),r*Math.sin(rad(a))];}
/** The clockwise tangent at angle `a` (a unit vector). */
export function tangent(a){return [-Math.sin(rad(a)),Math.cos(rad(a))];}

const f=n=>n.toFixed(2);
/** An SVG path for a ring segment between radii r0 < r1 and angles a0 < a1 (a full circle when a1 - a0 ≥ 360). */
export function segmentPath(r0,r1,a0,a1){
  if(a1-a0>=359.99){
    const ring=r=>`M ${f(0)} ${f(-r)} A ${r} ${r} 0 1 1 ${f(0)} ${f(r)} A ${r} ${r} 0 1 1 ${f(0)} ${f(-r)} Z`;
    return `${ring(r1)} ${ring(r0)}`;
  }
  const large=a1-a0>180?1:0;
  const [x0,y0]=point(r1,a0),[x1,y1]=point(r1,a1),[x2,y2]=point(r0,a1),[x3,y3]=point(r0,a0);
  return `M ${f(x0)} ${f(y0)} A ${r1} ${r1} 0 ${large} 1 ${f(x1)} ${f(y1)} L ${f(x2)} ${f(y2)} A ${r0} ${r0} 0 ${large} 0 ${f(x3)} ${f(y3)} Z`;
}

/** Whether an upright box (centre cx, cy; w x h) lies inside the segment {r0, r1, a0, a1}, `margin` px from
 * every edge. The farthest point of the box is a corner; the nearest point to the centre is found exactly; the
 * wedge (at most 180°) holds the box when it holds its corners. */
export function boxFits(cx,cy,w,h,{r0,r1,a0,a1},margin=6){
  const xs=[cx-w/2,cx+w/2],ys=[cy-h/2,cy+h/2];
  const corners=xs.flatMap(x=>ys.map(y=>[x,y]));
  if(corners.some(([x,y])=>Math.hypot(x,y)>r1-margin))return false;
  const nx=Math.min(Math.max(0,xs[0]),xs[1]),ny=Math.min(Math.max(0,ys[0]),ys[1]);
  if(Math.hypot(nx,ny)<r0+margin)return false;
  if(a1-a0>=359.99)return true;
  for(const [x,y] of corners){
    // The distance to each edge ray, on its inner side (positive inside the wedge).
    const side=a=>{const [ux,uy]=point(1,a);return ux*y-uy*x;};
    if(side(a0)<margin||-side(a1)<margin)return false;
  }
  return true;
}
/** The point a segment's label is centred on: the middle angle, the middle of the band. */
export function centroid({r0,r1,a0,a1}){
  if(a1-a0>=359.99)return [0,-(r0+r1)/2];
  return point((r0+r1)/2,(a0+a1)/2);
}

// ---- Labels: upright, centred in the segment, up to 3 lines of whole words, auto-fit size, a deliberate short form. ----

const FILLER=new Set(['a','an','the','of','and','or','to','for','with','in','on','at','by','from','into','onto','about','via','my','this','that','it','is','are','be',
  'after','before','during','when','while','then','session','sessions','add','adds','fix','fixes','update','updates','new','open']);
const norm=w=>w.toLowerCase().replace(/[^\p{L}\p{N}]/gu,'');

/** The other siblings' words (normalised); comparisons need at least two of them. */
function others(text,siblings){
  return siblings.filter(s=>s!==text).map(s=>String(s??'').split(/\s+/).filter(Boolean).map(norm)).filter(s=>s.length);
}
/** How many leading words every other sibling shares with this name (the group's project, "Session"...): at
 * least two siblings, and at least one word is always left. */
export function sharedLead(words,sibs){
  let lead=0;
  if(sibs.length>=2)while(lead<words.length-1&&sibs.every(s=>s[lead]===norm(words[lead])))lead++;
  return lead;
}

/** A short form for a long name: its words without filler, without leading words that every sibling shares
 * (the project, "Session"...), without words every sibling has. `order`: the kept words' indices, the most
 * distinctive first (in fewer siblings, then longer). */
export function shortForm(text,siblings=[]){
  const words=String(text??'').split(/\s+/).filter(Boolean);
  const sibs=others(text,siblings);
  const lead=sharedLead(words,sibs);
  const inAll=w=>sibs.length>=2&&sibs.every(s=>s.includes(w));
  let kept=words.map((w,i)=>i).filter(i=>i>=lead&&norm(words[i])&&!FILLER.has(norm(words[i]))&&!inAll(norm(words[i])));
  if(!kept.length){
    const longest=words.map((w,i)=>i).sort((a,b)=>norm(words[b]).length-norm(words[a]).length)[0];
    kept=longest===undefined?[]:[longest];
  }
  const freq=i=>sibs.filter(s=>s.includes(norm(words[i]))).length;
  const order=[...kept].sort((a,b)=>freq(a)-freq(b)||norm(words[b]).length-norm(words[a]).length||a-b);
  // A trailing colon or dash left by a dropped word reads oddly: drop it.
  return {words:words.map(w=>w.replace(/[:\-–]$/,'')),kept,order};
}

/** Every way to put `words` on `k` lines (in order), as lists of line strings. */
function wraps(words,k){
  if(k===1)return [[words.join(' ')]];
  const out=[];
  for(let i=1;i<=words.length-k+1;i++)for(const rest of wraps(words.slice(i),k-1))out.push([words.slice(0,i).join(' '),...rest]);
  return out;
}

/**
 * Fit a label in a segment. `box`: {r0, r1, a0, a1} (the segment, its gap taken off). `measure(text, size)`
 * gives a text width in px. `prefix`/`suffix` (a working dot, "›", a glyph) are kept on the first and last line.
 * Returns {lines, size, x, y, lh, short}: upright lines of whole words (1 to 3), centred on the segment's
 * centroid, all inside the segment; never an ellipsis.
 */
export function fitLabel(text,{siblings=[],box,measure,prefix='',suffix='',max=FONT_MAX,min=FONT_MIN}){
  const [x,y]=centroid(box);
  const tryWords=(words,floor=min,lineMax=MAX_LINES)=>{
    if(!words.length)return null;
    for(let size=max;size>=floor;size--){
      const lh=Math.round(size*1.2);
      for(let k=1;k<=Math.min(lineMax,words.length);k++){
        let best=null;
        for(const lines0 of k===words.length&&k>1?[words]:wraps(words,k)){
          const lines=lines0.map((l,j)=>[j===0?prefix:'',l,j===lines0.length-1?suffix:''].filter(Boolean).join(' '));
          const w=Math.max(...lines.map(l=>measure(l,size)))+2;
          if(!best||w<best.w)best={lines,w};
        }
        if(best&&boxFits(x,y,best.w,k*lh,box))return {lines:best.lines,size,x,y,lh};
      }
    }
    return null;
  };
  // A prefix every sibling shares says nothing here (the caption keeps the full name): it is left out.
  const all=String(text??'').split(/\s+/).filter(Boolean);
  const lead=sharedLead(all,others(text,siblings));
  const words=all.slice(lead);
  const full=tryWords(words);
  if(full)return {...full,short:lead>0};
  // A deliberate short form: the distinctive words, dropping the least distinctive until it fits.
  const sf=shortForm(text,siblings);
  for(let n=sf.order.length;n>=1;n--){
    const keep=new Set(sf.order.slice(0,n));
    const r=tryWords(sf.kept.filter(i=>keep.has(i)).map(i=>sf.words[i]));
    if(r)return {...r,short:true};
  }
  // One word longer than the segment (never cut): smaller, down to 8 px; else split with a hyphen over two
  // lines, as near the middle as fits.
  // Readable first: split with a hyphen at the normal sizes, then smaller (one line, then split).
  const w=sf.words[sf.order[0]]||words[0]||'';
  const half=Math.ceil(w.length/2);
  const split=floor=>{
    for(let d=0;d<half;d++)for(const k of [half-d,half+d]){
      if(k<2||k>w.length-2)continue;
      const r=tryWords([`${w.slice(0,k)}-`,w.slice(k)],floor,2);
      if(r&&r.lines.length===2)return r;
    }
    return null;
  };
  const readable=split(min);
  if(readable)return {...readable,short:true};
  const small=tryWords([w],WORD_MIN)||split(WORD_MIN);
  if(small)return {...small,short:true};
  return {lines:[`${w.slice(0,half)}-`,w.slice(half)],size:WORD_MIN,x,y,lh:Math.round(WORD_MIN*1.2),short:true};
}
