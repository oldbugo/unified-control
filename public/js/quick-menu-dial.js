// Geometry and label fitting of the Vibe Key quick menu dial (0.34.0; 0.35.0: bigger, 8 slots, an end stop,
// curved labels that fit). Pure functions, no DOM (text widths come from a `measure` function): checked by
// scripts/check-quick-menu.cjs. Angles are degrees clockwise from 12 o'clock.

/** At most this many segments per ring, Close/Back included; longer lists become a moving window. */
export const MAX_SLOTS=8;
/** The gap kept anticlockwise of Close/Back, where the list's end stop is drawn (degrees). */
export const STOP=16;
/** Each segment's angle: the full ring less the stop gap, in 8 (so fewer items leave a larger gap). */
export const STEP=(360-STOP)/MAX_SLOTS;
/** How far the highlighted segment is pushed out (px). */
export const PUSH=10;
/** The label font: from the normal size down to a readable minimum (px). */
export const FONT_MAX=15, FONT_MIN=11;
/** A single word too long for its segment may go this small before it is split with a hyphen. */
export const WORD_MIN=8;
const PAD=14, MIN_WIDTH=380, CAPTION=96;
/** Inner and outer radius of each ring, oldest (innermost) first, for 1, 2 or 3 rings shown. The active (outer)
 * ring grows per level (170, 210, 260); the dimmed parent rings fit inside it. */
const RINGS=[[[60,170]],[[60,118],[124,210]],[[60,104],[110,160],[166,260]]];

export function ringRadii(count){return RINGS[Math.min(Math.max(count|0,1),3)-1];}

/** The window's logical size for this many rings (src-tauri/src/quick_menu.rs `dial_size` matches it). */
export function dialSize(rings){
  const d=dialDiameter(rings);
  return {width:Math.max(d,MIN_WIDTH),height:d+CAPTION};
}
export function dialDiameter(rings){return 2*(ringRadii(rings).at(-1)[1]+PUSH+PAD);}
/** The dial's drawn size when the window is smaller than wanted (a small work area): scaled down to fit. */
export function fitScale(rings,width,height){
  const d=dialDiameter(rings);
  return Math.min(1,width/d,Math.max(height-CAPTION,1)/d);
}

/** Each slot's start, middle and end angle: slot 0 centred at 12 o'clock, the others clockwise. */
export function slotAngles(n){
  return Array.from({length:Math.max(n,1)},(_,i)=>({start:i*STEP-STEP/2,mid:i*STEP,end:i*STEP+STEP/2}));
}
/** Where the end stop is drawn: in the middle of the reserved gap, on row 0's anticlockwise side. */
export const STOP_ANGLE=-STEP/2-STOP/2;

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
/** What the ring shows at its ends: the stop only at the list's true end; a "more" fade where rows are hidden. */
export function edges(count,offset){
  const shown=slotRows(count,offset);
  return {stop:shown.at(-1)===count-1,moreBefore:count>MAX_SLOTS&&offset>0,moreAfter:shown.at(-1)<count-1};
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
/** True when text along an arc at this angle would be upside down: it then runs anticlockwise. */
export function flipped(mid){const m=((mid%360)+360)%360;return m>90&&m<270;}
/** An arc for text at radius r from a0 to a1, read left to right (anticlockwise in the bottom half). */
export function textArc(r,a0,a1){
  const [x0,y0]=point(r,a0),[x1,y1]=point(r,a1);
  return flipped((a0+a1)/2)?`M ${f(x1)} ${f(y1)} A ${r} ${r} 0 0 0 ${f(x0)} ${f(y0)}`:`M ${f(x0)} ${f(y0)} A ${r} ${r} 0 0 1 ${f(x1)} ${f(y1)}`;
}

// ---- Labels: along the arc, up to 2 lines, whole words, auto-fit size, a deliberate short form. ----

const FILLER=new Set(['a','an','the','of','and','or','to','for','with','in','on','at','by','from','into','onto','about','via','my','this','that','it','is','are','be',
  'after','before','during','when','while','then','session','sessions','add','adds','fix','fixes','update','updates','new','open']);
const norm=w=>w.toLowerCase().replace(/[^\p{L}\p{N}]/gu,'');

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

/** The usable length of an arc of radius r over `deg` degrees, less a margin at each end. */
function room(r,deg,margin){return Math.max(r*deg*Math.PI/180-2*margin,0);}

/**
 * Fit a label in a segment. `box`: {r0, r1, deg (the segment's angle less its gap), mid (its middle angle)}.
 * `measure(text, size)` gives a text width in px. `prefix`/`suffix` (a working dot, "›") are kept on the first
 * and last line. Returns {lines, size, radii, short}: whole words, one or two lines, every line inside the
 * segment, never an ellipsis.
 */
export function fitLabel(text,{siblings=[],box,measure,prefix='',suffix='',max=FONT_MAX,min=FONT_MIN}){
  const {r0,r1,deg,mid}=box;
  const thick=r1-r0,rm=(r0+r1)/2,margin=7;
  const top=!flipped(mid);
  const tryWords=(words,floor=min)=>{
    for(let size=max;size>=floor;size--){
      const lh=size*1.15;
      if(thick>=lh+8){
        const line=[prefix,words.join(' '),suffix].filter(Boolean).join(' ');
        if(measure(line,size)<=room(rm-size*0.3,deg,margin))return {lines:[line],size,radii:[rm]};
      }
      if(words.length>1&&thick>=2*lh+6){
        // The first line reads on top: the outer arc in the top half, the inner one in the bottom half.
        const ra=top?rm+lh/2:rm-lh/2,rb=top?rm-lh/2:rm+lh/2;
        let best=null;
        for(let k=1;k<words.length;k++){
          const a=[prefix,words.slice(0,k).join(' ')].filter(Boolean).join(' ');
          const b=[words.slice(k).join(' '),suffix].filter(Boolean).join(' ');
          const wa=measure(a,size)/room(ra-size*0.3,deg,margin),wb=measure(b,size)/room(rb-size*0.3,deg,margin);
          if(wa<=1&&wb<=1&&(!best||Math.max(wa,wb)<best.worst))best={lines:[a,b],worst:Math.max(wa,wb)};
        }
        if(best)return {lines:best.lines,size,radii:[ra,rb]};
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
  // One word longer than the segment (0.37.0: never cut): smaller, down to 8 px; else split with a hyphen
  // over two lines, as near the middle as fits.
  const w=sf.words[sf.order[0]]||words[0]||'';
  const small=tryWords([w],WORD_MIN);
  if(small)return {...small,short:true};
  const half=Math.ceil(w.length/2);
  for(let d=0;d<half;d++)for(const k of [half-d,half+d]){
    if(k<2||k>w.length-2)continue;
    const r=tryWords([`${w.slice(0,k)}-`,w.slice(k)],WORD_MIN);
    if(r&&r.lines.length===2)return {...r,short:true};
  }
  return {lines:[`${w.slice(0,half)}-`,w.slice(half)],size:WORD_MIN,radii:[rm+WORD_MIN*0.6,rm-WORD_MIN*0.6],short:true};
}
