// Input tester audio: microphone level, a short recording, play back and test tones. Everything stays in this
// window's memory; nothing is sent anywhere and nothing is saved unless the user presses Save.
// Fake and quiet test instances use FakeAudio: no microphone is opened and no sound is played.
import {blockLevel} from './input-tester-model.js';

const label=(d,i,kind)=>d.label||(d.deviceId==='default'?'Windows default':`${kind} ${i+1}`);

export class WebAudio{
  constructor(){this.ctx=null;this.stream=null;this.analyser=null;this.proc=null;this.recording=null;this.out=null;}
  get simulated(){return false;}
  async list(kind){
    if(!navigator.mediaDevices?.enumerateDevices)return [];
    const all=await navigator.mediaDevices.enumerateDevices();
    const name=kind==='audioinput'?'Microphone':'Speaker';
    return all.filter(d=>d.kind===kind&&d.deviceId!=='communications').map((d,i)=>({id:d.deviceId,label:label(d,i,name)}));
  }
  /** Open a microphone. `processing`: the browser's echo cancelling, noise suppression and gain control. */
  async open(id,{processing=false}={}){
    this.close();
    const audio={echoCancellation:processing,noiseSuppression:processing,autoGainControl:processing,channelCount:{ideal:2}};
    if(id&&id!=='default')audio.deviceId={exact:id};
    this.stream=await navigator.mediaDevices.getUserMedia({audio});
    this.ctx=new AudioContext();
    const src=this.ctx.createMediaStreamSource(this.stream);
    this.analyser=this.ctx.createAnalyser();this.analyser.fftSize=2048;
    src.connect(this.analyser);
    const track=this.stream.getAudioTracks()[0],s=track.getSettings();
    const channels=Math.max(1,Math.min(2,s.channelCount||1));
    // A silent tap for the recorder (the processor must reach the output to run; its output gain is zero).
    this.proc=this.ctx.createScriptProcessor(4096,channels,channels);
    const mute=this.ctx.createGain();mute.gain.value=0;
    src.connect(this.proc);this.proc.connect(mute);mute.connect(this.ctx.destination);
    this.proc.onaudioprocess=ev=>{
      const r=this.recording;if(!r)return;
      for(let c=0;c<channels;c++)r.chunks[c].push(new Float32Array(ev.inputBuffer.getChannelData(c)));
      r.frames+=ev.inputBuffer.length;
      r.onProgress?.(Math.min(1,r.frames/r.target));
      if(r.frames>=r.target){this.recording=null;r.done();}
    };
    this.buf=new Float32Array(this.analyser.fftSize);
    return {label:track.label||'Microphone',deviceId:s.deviceId||id,sampleRate:s.sampleRate||this.ctx.sampleRate,
      channels:s.channelCount||channels,bits:s.sampleSize||null,processing:!!(s.echoCancellation||s.noiseSuppression||s.autoGainControl),
      latency:s.latency??null,contextRate:this.ctx.sampleRate,recordChannels:channels};
  }
  get isOpen(){return !!this.stream;}
  level(){
    if(!this.analyser)return null;
    this.analyser.getFloatTimeDomainData(this.buf);
    return blockLevel(this.buf);
  }
  record(seconds,onProgress){
    if(!this.proc)return Promise.reject(new Error('Start the microphone first.'));
    const channels=this.proc.channelCount,rate=this.ctx.sampleRate;
    return new Promise(resolve=>{
      this.recording={chunks:Array.from({length:channels},()=>[]),frames:0,target:Math.round(seconds*rate),onProgress,
        done:()=>resolve({sampleRate:rate,channels:r.chunks.map(list=>{const out=new Float32Array(r.target);let o=0;for(const c of list){out.set(c.subarray(0,Math.min(c.length,r.target-o)),o);o+=c.length;if(o>=r.target)break;}return out;})})};
      const r=this.recording;
    });
  }
  async output(id){
    if(this.out&&this.outId===id)return this.out;
    try{await this.out?.close();}catch{}
    this.out=new AudioContext();this.outId=id;
    if(id&&id!=='default'&&typeof this.out.setSinkId==='function')await this.out.setSinkId(id);
    return this.out;
  }
  async play(rec,outputId){
    const ctx=await this.output(outputId);
    const b=ctx.createBuffer(rec.channels.length,rec.channels[0].length,rec.sampleRate);
    rec.channels.forEach((c,i)=>b.copyToChannel(c,i));
    const s=ctx.createBufferSource();s.buffer=b;s.connect(ctx.destination);s.start();
    return new Promise(resolve=>{s.onended=()=>resolve();});
  }
  /** A soft 440 Hz tone for one second: side is left, right or both. */
  async tone(side,outputId){
    const ctx=await this.output(outputId),t=ctx.currentTime;
    const osc=ctx.createOscillator(),g=ctx.createGain(),pan=ctx.createStereoPanner();
    osc.frequency.value=440;pan.pan.value=side==='left'?-1:side==='right'?1:0;
    g.gain.setValueAtTime(0,t);g.gain.linearRampToValueAtTime(.15,t+.05);g.gain.setValueAtTime(.15,t+.9);g.gain.linearRampToValueAtTime(0,t+1);
    osc.connect(g);g.connect(pan);pan.connect(ctx.destination);osc.start(t);osc.stop(t+1.02);
    return new Promise(resolve=>{osc.onended=()=>resolve();});
  }
  close(){
    this.recording=null;
    try{this.proc&&(this.proc.onaudioprocess=null);}catch{}
    this.stream?.getTracks().forEach(t=>t.stop());
    try{this.ctx?.close();}catch{}
    this.ctx=this.stream=this.analyser=this.proc=null;
  }
  dispose(){this.close();try{this.out?.close();}catch{}this.out=null;}
}

/** Test stand-in: lists simulated devices, makes a synthetic level and recording, never touches audio. */
export class FakeAudio{
  constructor(){this.opened=null;this.played=[];this.started=0;}
  get simulated(){return true;}
  async list(kind){
    return kind==='audioinput'
      ?[{id:'default',label:'Default - Microphone (AU05) (simulated)'},{id:'fake-au05',label:'Microphone (AU05) (simulated)'},{id:'fake-webcam',label:'Microphone (Webcam) (simulated)'}]
      :[{id:'default',label:'Default - Speakers (simulated)'},{id:'fake-headphones',label:'Headphones (simulated)'}];
  }
  async open(id,{processing=false}={}){
    this.opened={id,processing};this.started=performance.now();
    return {label:id==='fake-webcam'?'Microphone (Webcam) (simulated)':'Microphone (AU05) (simulated)',deviceId:id,sampleRate:48000,channels:1,bits:16,
      processing,latency:.01,contextRate:48000,recordChannels:1};
  }
  get isOpen(){return !!this.opened;}
  level(){
    if(!this.opened)return null;
    const t=(performance.now()-this.started)/1000,a=.05+.25*Math.abs(Math.sin(t*3));
    return {rms:a*.6,peak:a};
  }
  async record(seconds,onProgress){
    if(!this.opened)throw new Error('Start the microphone first.');
    const rate=8000,n=Math.round(seconds*rate),c=new Float32Array(n);
    for(let i=0;i<n;i++)c[i]=.3*Math.sin(2*Math.PI*220*i/rate);
    for(const p of [.25,.5,.75,1]){await new Promise(r=>setTimeout(r,20));onProgress?.(p);}
    return {sampleRate:rate,channels:[c]};
  }
  async play(rec,outputId){this.played.push({what:'recording',outputId,frames:rec.channels[0].length});}
  async tone(side,outputId){this.played.push({what:'tone',side,outputId});}
  close(){this.opened=null;}
  dispose(){this.close();}
}
