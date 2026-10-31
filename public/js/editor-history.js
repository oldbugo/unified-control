// Bounded in-memory undo; native profile storage remains authoritative.
export class EditorHistory {
  constructor(){this.items=[];this.bytes=0;this.group='';this.at=0;}
  checkpoint(value,group=''){
    const now=Date.now();
    if(group && group===this.group && now-this.at<800){this.at=now;return;}
    const text=JSON.stringify(value);
    if(this.items.at(-1)===text)return;
    if(text.length>4_000_000)return;
    while(this.items.length>=24 || this.bytes+text.length>8_000_000){this.bytes-=this.items.shift().length;}
    this.items.push(text);this.bytes+=text.length;this.group=group;this.at=now;
  }
  undo(){const text=this.items.pop();if(!text)return null;this.bytes-=text.length;this.group='';return JSON.parse(text);}
  get available(){return this.items.length>0;}
}
