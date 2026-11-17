// Geometry of the Vibe Key quick menu dial (0.34.0): a thick ring of equal pie segments per menu level.
// Pure functions, no DOM: checked by scripts/check-quick-menu.cjs. Angles are degrees clockwise from 12 o'clock.

/** At most this many segments per ring; longer lists become a moving window (row 0, Close or Back, stays on top). */
export const MAX_SLOTS=12;
/** How far the highlighted segment is pushed out (px). */
export const PUSH=8;
const PAD=12, MIN_WIDTH=340, CAPTION=96;
/** Inner and outer radius of each ring, oldest (innermost) first, for 1, 2 or 3 rings shown. The active (outer)
 * ring grows about 1.25 × per level (120, 150, 188); the dimmed parent rings fit inside it. */
const RINGS=[[[44,120]],[[44,88],[92,150]],[[44,76],[80,118],[122,188]]];

export function ringRadii(count){return RINGS[Math.min(Math.max(count|0,1),3)-1];}

/** The window's logical size for this many rings (src-tauri/src/quick_menu.rs `dial_size` matches it). */
export function dialSize(rings){
  const outer=ringRadii(rings).at(-1)[1];
  const d=2*(outer+PUSH+PAD);
  return {width:Math.max(d,MIN_WIDTH),height:d+CAPTION};
}

/** Each slot's start, middle and end angle; slot 0 is centred at 12 o'clock, the others follow clockwise. */
export function slotAngles(n){
  const step=360/Math.max(n,1);
  return Array.from({length:Math.max(n,1)},(_,i)=>({start:i*step-step/2,mid:i*step,end:i*step+step/2}));
}

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

/** The highlight after a move (no wrap-around: it stops at Close/Back and at the last row). */
export function step(count,highlight,delta){return Math.min(Math.max(highlight+delta,0),Math.max(count-1,0));}

const rad=a=>(a-90)*Math.PI/180;
export function point(r,a){return [r*Math.cos(rad(a)),r*Math.sin(rad(a))];}

/** An SVG path for a ring segment between radii r0 < r1 and angles a0 < a1 (a full circle when a1 - a0 ≥ 360). */
export function segmentPath(r0,r1,a0,a1){
  const f=n=>n.toFixed(2);
  if(a1-a0>=359.99){
    const ring=r=>`M ${f(0)} ${f(-r)} A ${r} ${r} 0 1 1 ${f(0)} ${f(r)} A ${r} ${r} 0 1 1 ${f(0)} ${f(-r)} Z`;
    return `${ring(r1)} ${ring(r0)}`;
  }
  const large=a1-a0>180?1:0;
  const [x0,y0]=point(r1,a0),[x1,y1]=point(r1,a1),[x2,y2]=point(r0,a1),[x3,y3]=point(r0,a0);
  return `M ${f(x0)} ${f(y0)} A ${r1} ${r1} 0 ${large} 1 ${f(x1)} ${f(y1)} L ${f(x2)} ${f(y2)} A ${r0} ${r0} 0 ${large} 0 ${f(x3)} ${f(y3)} Z`;
}

/** How many characters fit in a segment's label (the rest is cut with "…"). */
export function labelChars(r0,r1,n){
  const arc=2*Math.PI*((r0+r1)/2)/Math.max(n,1);
  return Math.min(Math.max(Math.floor(Math.min(arc,(r1-r0)*1.15)/7),2),12);
}
export function shorten(text,chars){
  const s=String(text??'');
  return s.length<=chars?s:s.slice(0,Math.max(chars-1,1))+'…';
}
