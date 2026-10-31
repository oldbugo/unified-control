// Keep a decoded frame on screen until its replacement is ready. State belongs
// to the current button DOM node, so rebuilding/navigating the editor releases it.
export class KeyPreviewRenderer {
  constructor(){this.frames=new WeakMap();}
  async render(element,png,alt,isCurrent=()=>true){
    if(!element.isConnected || !isCurrent())return false;
    let frame=this.frames.get(element);
    if(!frame){frame={ticket:0,png:null,image:null};this.frames.set(element,frame);}
    const ticket=++frame.ticket;
    const bytes=new Uint8Array(png);
    if(frame.image?.parentNode===element && frame.png?.length===bytes.length && bytes.every((value,index)=>value===frame.png[index])){
      frame.image.alt=alt;return false;
    }
    const image=element.ownerDocument.createElement('img');
    image.className='rendered-key';image.alt=alt;
    const url=URL.createObjectURL(new Blob([bytes],{type:'image/png'}));
    try{
      if(typeof image.decode==='function'){image.src=url;await image.decode();}
      else await new Promise((resolve,reject)=>{image.onload=resolve;image.onerror=reject;image.src=url;});
      if(frame.ticket!==ticket || !element.isConnected || !isCurrent())return false;
      element.replaceChildren(image);
      frame.png=bytes;frame.image=image;
      return true;
    }finally{URL.revokeObjectURL(url);}
  }
}
