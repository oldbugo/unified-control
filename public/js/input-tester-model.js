// Input tester: pure logic (no DOM, no audio). Keyboard drawing data, what is held and was pressed, the last
// combination, log formatting, browser-event fallback and WAV encoding. Used by input-tester.js and its checks.

// A key: [code, label, width in key units, height in rows]. A gap: [null, '', width].
const k=(code,label,w=1,h=1)=>[code,label,w,h];
const gap=w=>[null,'',w,1];
const fn=(a,b)=>Array.from({length:b-a+1},(_,i)=>k(`F${a+i}`,`F${a+i}`));

// The main block is 15 units wide. ANSI and ISO differ in rows 2 to 4 only.
function mainRows(iso){
  return [
    [k('Escape','Esc'),gap(1),...fn(1,4),gap(.5),...fn(5,8),gap(.5),...fn(9,12)],
    [k('Backquote','`'),...'1234567890'.split('').map(d=>k(`Digit${d}`,d)),k('Minus','-'),k('Equal','='),k('Backspace','Backspace',2)],
    iso
      ?[k('Tab','Tab',1.5),...'QWERTYUIOP'.split('').map(c=>k(`Key${c}`,c)),k('BracketLeft','['),k('BracketRight',']'),k('Enter','Enter',1.5)]
      :[k('Tab','Tab',1.5),...'QWERTYUIOP'.split('').map(c=>k(`Key${c}`,c)),k('BracketLeft','['),k('BracketRight',']'),k('Backslash','\\',1.5)],
    iso
      ?[k('CapsLock','Caps',1.75),...'ASDFGHJKL'.split('').map(c=>k(`Key${c}`,c)),k('Semicolon',';'),k('Quote',"'"),k('Backslash','#'),k('Enter','',1.25)]
      :[k('CapsLock','Caps',1.75),...'ASDFGHJKL'.split('').map(c=>k(`Key${c}`,c)),k('Semicolon',';'),k('Quote',"'"),k('Enter','Enter',2.25)],
    iso
      ?[k('ShiftLeft','Shift',1.25),k('IntlBackslash','\\'),...'ZXCVBNM'.split('').map(c=>k(`Key${c}`,c)),k('Comma',','),k('Period','.'),k('Slash','/'),k('ShiftRight','Shift',2.75)]
      :[k('ShiftLeft','Shift',2.25),...'ZXCVBNM'.split('').map(c=>k(`Key${c}`,c)),k('Comma',','),k('Period','.'),k('Slash','/'),k('ShiftRight','Shift',2.75)],
    [k('ControlLeft','Ctrl',1.25),k('MetaLeft','Win',1.25),k('AltLeft','Alt',1.25),k('Space','',6.25),k('AltRight','Alt',1.25),k('MetaRight','Win',1.25),k('ContextMenu','Menu',1.25),k('ControlRight','Ctrl',1.25)],
  ];
}
const NAV_ROWS=[
  [k('PrintScreen','PrtSc'),k('ScrollLock','ScrLk'),k('Pause','Pause')],
  [k('Insert','Ins'),k('Home','Home'),k('PageUp','PgUp')],
  [k('Delete','Del'),k('End','End'),k('PageDown','PgDn')],
  [],
  [gap(1),k('ArrowUp','↑')],
  [k('ArrowLeft','←'),k('ArrowDown','↓'),k('ArrowRight','→')],
];
const PAD_ROWS=[
  [],
  [k('NumLock','Num'),k('NumpadDivide','/'),k('NumpadMultiply','*'),k('NumpadSubtract','-')],
  [k('Numpad7','7'),k('Numpad8','8'),k('Numpad9','9'),k('NumpadAdd','+',1,2)],
  [k('Numpad4','4'),k('Numpad5','5'),k('Numpad6','6')],
  [k('Numpad1','1'),k('Numpad2','2'),k('Numpad3','3'),k('NumpadEnter','Ent',1,2)],
  [k('Numpad0','0',2),k('NumpadDecimal','.')],
];
export const EXTRA_ROW=[fn(13,24)];
export const MOUSE_KEYS=[k('MouseLeft','Left'),k('MouseMiddle','Middle'),k('MouseRight','Right'),k('MouseBack','Back'),k('MouseForward','Forward'),k('WheelUp','Wheel ↑'),k('WheelDown','Wheel ↓'),k('WheelLeft','Wheel ←'),k('WheelRight','Wheel →')];
export const MEDIA_KEYS=[k('AudioVolumeMute','Mute'),k('AudioVolumeDown','Vol −'),k('AudioVolumeUp','Vol +'),k('MediaTrackPrevious','Prev'),k('MediaPlayPause','Play'),k('MediaTrackNext','Next'),k('MediaStop','Stop')];
// HID consumer usages map onto the same media keys, so a knob or media key lights them whichever way it reports.
export const HID_CODES={'Hid0C:00E2':'AudioVolumeMute','Hid0C:00EA':'AudioVolumeDown','Hid0C:00E9':'AudioVolumeUp','Hid0C:00B6':'MediaTrackPrevious','Hid0C:00CD':'MediaPlayPause','Hid0C:00B5':'MediaTrackNext','Hid0C:00B7':'MediaStop'};
// The Vibe Key's controls and the codes they type when set to Unified Control actions (core/au05.rs, own_vk).
export const VIBE_KEY=[
  {code:'F20',vk:0x83,label:'K1',hint:'voice'},
  {code:'F21',vk:0x84,label:'K2',hint:'confirm'},
  {code:'F22',vk:0x85,label:'K3',hint:'cancel'},
  {code:'F19',vk:0x82,label:'Knob press',hint:''},
  {code:'F24',vk:0x87,label:'Knob left',hint:''},
  {code:'F23',vk:0x86,label:'Knob right',hint:''},
];

/** Place rows of keys on a grid of quarter units. Returns {keys:[{code,label,x,y,w,h}], cols, rows}. */
export function place(rows){
  const keys=[];let cols=0;const taken=new Set();
  rows.forEach((row,y)=>{
    let x=0;
    for(const [code,label,w,h] of row){
      while(taken.has(`${x},${y}`))x+=1;
      if(code)keys.push({code,label,x:x*4,y,w:w*4,h});
      if(code&&h>1)for(let r=1;r<h;r++)for(let c=0;c<w;c++)taken.add(`${x+c},${y+r}`);
      x+=w;
    }
    cols=Math.max(cols,x*4);
  });
  return {keys,cols:Math.round(cols),rows:rows.length};
}
export function layout({iso=false,numpad=true}={}){
  const blocks=[{id:'main',...place(mainRows(iso))},{id:'nav',...place(NAV_ROWS)}];
  if(numpad)blocks.push({id:'pad',...place(PAD_ROWS)});
  return blocks;
}
/** Every code the drawing (all blocks) shows. */
export function drawnCodes(){
  const s=new Set();
  for(const iso of [false,true])for(const b of layout({iso,numpad:true}))for(const key of b.keys)s.add(key.code);
  for(const row of EXTRA_ROW)for(const [c] of row)s.add(c);
  for(const [c] of [...MOUSE_KEYS,...MEDIA_KEYS])s.add(c);
  return s;
}

const MODIFIER={ControlLeft:'Ctrl',ControlRight:'Ctrl',ShiftLeft:'Shift',ShiftRight:'Shift',AltLeft:'Alt',AltRight:'Alt',MetaLeft:'Win',MetaRight:'Win'};
const MOD_ORDER=['Ctrl','Shift','Alt','Win'];
export const isModifier=code=>Object.hasOwn(MODIFIER,code);
/** The code that lights in the drawing for an event (HID media usages light the media keys). */
export const drawCode=e=>HID_CODES[e.code]||e.code;

/**
 * What the page knows from the events it has seen: keys held now, keys pressed since Clear, the last
 * combination, and per-device counts when the host does not count (browser preview).
 */
export class InputModel{
  constructor(){this.clear();}
  clear(){
    this.held=new Map();     // `${device}|${code}` -> code
    this.pressed=new Set();  // codes pressed since Clear
    this.combo=[];
    this.counts={};
    this.total=0;
  }
  heldCodes(){return new Set(this.held.values());}
  /** Apply one event. Returns true when the drawing or combination changed. */
  apply(e){
    this.total++;
    const c=this.counts[e.device]||(this.counts[e.device]={down:0,up:0,repeat:0,buttons:0,wheel:0,moves:0,hid:0,last:null});
    c.last=e.t;
    if(e.kind==='key')c[e.action==='repeat'?'repeat':e.action==='up'?'up':'down']++;
    else if(e.kind==='button'){if(e.action==='down')c.buttons++;}
    else if(e.kind==='wheel')c.wheel++;
    else if(e.kind==='move'){c.moves++;return false;}
    else if(e.kind==='hid')c.hid++;
    else return false;
    const code=drawCode(e),id=`${e.device}|${code}`;
    if(e.action==='up'){this.held.delete(id);return true;}
    if(e.action==='report')return false;
    if(e.kind==='wheel'){this.pressed.add(code);this.combo=[...(e.mods||[]),e.name];return true;}
    if(e.action==='repeat')return false;
    this.held.set(id,code);this.pressed.add(code);
    if(isModifier(code)){
      const mods=new Set([...this.held.values()].filter(isModifier).map(m=>MODIFIER[m]));
      this.combo=MOD_ORDER.filter(m=>mods.has(m));
    }else this.combo=[...(e.mods||[]),comboName(e)];
    return true;
  }
}
/** A short name for a key in a combination chip ("Left Ctrl" is just "Ctrl" there). */
export function comboName(e){
  if(isModifier(e.code))return MODIFIER[e.code];
  if(e.code==='Space')return 'Space';
  return e.name||e.code;
}

const pad=(n,w)=>String(n).padStart(w,'0');
export const hex=(n,w=2)=>n==null?'':`0x${pad(Number(n).toString(16).toUpperCase(),w)}`;
const ACTIONS={down:'Key down',up:'Key up',repeat:'Repeat',turn:'Turn',move:'Move',report:'Report',added:'Connected',removed:'Disconnected'};
/** The event's type in words, as in the log. */
export function eventType(e){
  if(e.kind==='button')return e.action==='down'?'Button down':'Button up';
  if(e.kind==='wheel')return `Wheel ${e.delta>0?'+':''}${e.delta??''}`.trim();
  if(e.kind==='move')return `Move ${e.dx},${e.dy}`;
  if(e.kind==='hid')return e.action==='report'?'HID report':e.action==='down'?'HID press':'HID release';
  return ACTIONS[e.action]||e.action;
}
/** One log row as plain columns: time, +gap, device, type, key, VK, scan, ext, modifiers, note. */
export function logColumns(e,prev,deviceName){
  const gap=prev?Math.max(0,e.t-prev.t):null;
  return [
    e.t.toFixed(1),
    gap==null?'':`+${gap.toFixed(1)}`,
    deviceName(e.device),
    eventType(e),
    e.kind==='hid'&&e.action==='report'?(e.bytes||''):(e.name||e.code),
    hex(e.vk),
    e.scan==null?'':hex(e.scan),
    e.ext==null?'':e.ext?'E0':'',
    (e.mods||[]).join('+'),
    e.claimed?'Claimed by Unified Control':e.kind==='hid'&&e.bytes&&e.action!=='report'?e.bytes:'',
  ];
}
export const LOG_HEADER=['Time (ms)','Gap (ms)','Device','Type','Key','VK','Scan','Ext','Modifiers','Note'];
/** Tab-separated text for the clipboard, oldest first. */
export function toTsv(events,deviceName){
  const rows=[LOG_HEADER];
  events.forEach((e,i)=>rows.push(logColumns(e,events[i-1],deviceName)));
  return rows.map(r=>r.map(v=>String(v).replace(/[\t\n]/g,' ')).join('\t')).join('\n');
}

const MOUSE_BUTTONS=['MouseLeft','MouseMiddle','MouseRight','MouseBack','MouseForward'];
const MOUSE_NAMES=['Left button','Middle button','Right button','Back button (X1)','Forward button (X2)'];
/**
 * Browser preview only (no app): turn a DOM event into the host's event shape. Only this window's keys,
 * with no device names. Returns null for events that are not tested.
 */
export function domEvent(ev,t,held){
  const mods=[];
  if(ev.type.startsWith('key')){
    if(ev.ctrlKey&&!/^Control/.test(ev.code))mods.push('Ctrl');
    if(ev.shiftKey&&!/^Shift/.test(ev.code))mods.push('Shift');
    if(ev.altKey&&!/^Alt/.test(ev.code))mods.push('Alt');
    if(ev.metaKey&&!/^Meta/.test(ev.code))mods.push('Win');
    const up=ev.type==='keyup',code=ev.code||`Key${ev.key}`;
    const action=up?'up':ev.repeat||held?.has(code)?'repeat':'down';
    const name=ev.key===' '?'Space':ev.key.length===1?ev.key.toUpperCase():ev.key;
    return {t,device:'browser',kind:'key',action,code,name,vk:ev.keyCode||null,scan:null,ext:null,mods};
  }
  if(ev.type==='mousedown'||ev.type==='mouseup'){
    const i=ev.button===0?0:ev.button===1?1:ev.button===2?2:ev.button===3?3:ev.button===4?4:-1;
    if(i<0)return null;
    return {t,device:'browser',kind:'button',action:ev.type==='mousedown'?'down':'up',code:MOUSE_BUTTONS[i],name:MOUSE_NAMES[i],mods};
  }
  if(ev.type==='wheel'){
    const v=ev.deltaY!==0,delta=Math.round(-(v?ev.deltaY:ev.deltaX));
    if(!delta)return null;
    const code=v?(delta>0?'WheelUp':'WheelDown'):(delta>0?'WheelRight':'WheelLeft');
    return {t,device:'browser',kind:'wheel',action:'turn',code,name:code.replace('Wheel','Wheel '),delta,mods};
  }
  return null;
}

/** Linear sample peak or RMS to dBFS (floored at -90). */
export const dbfs=v=>v>0?Math.max(-90,20*Math.log10(v)):-90;
/** Level of a block of samples: {rms, peak} (linear, 0..1). */
export function blockLevel(samples){
  let sum=0,peak=0;
  for(const s of samples){const a=Math.abs(s);sum+=s*s;if(a>peak)peak=a;}
  return {rms:samples.length?Math.sqrt(sum/samples.length):0,peak};
}

/** 16-bit PCM WAV from channels of Float32 samples. */
export function encodeWav(channels,sampleRate){
  const ch=Math.max(1,channels.length),n=channels[0]?.length||0,bytes=n*ch*2;
  const buf=new ArrayBuffer(44+bytes),v=new DataView(buf);
  const str=(o,s)=>{for(let i=0;i<s.length;i++)v.setUint8(o+i,s.charCodeAt(i));};
  str(0,'RIFF');v.setUint32(4,36+bytes,true);str(8,'WAVE');str(12,'fmt ');
  v.setUint32(16,16,true);v.setUint16(20,1,true);v.setUint16(22,ch,true);v.setUint32(24,sampleRate,true);
  v.setUint32(28,sampleRate*ch*2,true);v.setUint16(32,ch*2,true);v.setUint16(34,16,true);
  str(36,'data');v.setUint32(40,bytes,true);
  let o=44;
  for(let i=0;i<n;i++)for(let c=0;c<ch;c++){
    const s=Math.max(-1,Math.min(1,channels[c]?.[i]||0));
    v.setInt16(o,s<0?s*0x8000:s*0x7FFF,true);o+=2;
  }
  return new Uint8Array(buf);
}
export function toBase64(bytes){
  let s='';
  for(let i=0;i<bytes.length;i+=0x8000)s+=String.fromCharCode.apply(null,bytes.subarray(i,i+0x8000));
  return btoa(s);
}
