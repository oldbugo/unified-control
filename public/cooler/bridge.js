// UC embeds the reviewed CoolerScreen UI. Controls use this dedicated bridge.
// Requests go only to its same-origin UC parent, which validates every command.
(() => {
  const origin=location.origin, pending=new Map(), callbacks=new Map(), listeners=new Map();
  let next=0;
  const send=message=>parent.postMessage({channel:'uc-cooler-v1',...message},origin);
  const request=(type,body={})=>new Promise((resolve,reject)=>{
    const id=++next;
    const timer=setTimeout(()=>{pending.delete(id);reject(new Error('CoolerScreen did not respond.'));},120000);
    pending.set(id,{resolve,reject,timer});send({type,id,...body});
  });
  const bridge={
    transformCallback(fn){const id=++next;callbacks.set(id,fn);return id;},
    unregisterCallback(id){callbacks.delete(id);},
    invoke(command,args={}){
      if(command==='plugin:event|listen'){
        if(!['cs:job','cs:library','cs:device','cs:app'].includes(args.event)||!callbacks.has(args.handler))return Promise.reject(new Error('Unsupported cooler event.'));
        const id=++next;listeners.set(id,{event:args.event,handler:args.handler});return Promise.resolve(id);
      }
      if(command==='plugin:event|unlisten'){listeners.delete(args.eventId);return Promise.resolve();}
      return request('call',{command,args});
    }
  };
  Object.defineProperty(window,'__UC_COOLER_BRIDGE__',{value:bridge,configurable:false,writable:false});
  window.addEventListener('message',async e=>{
    if(e.source!==parent||e.origin!==origin||e.data?.channel!=='uc-cooler-v1')return;
    const m=e.data;
    if(m.type==='result'){
      const p=pending.get(m.id);if(!p)return;pending.delete(m.id);clearTimeout(p.timer);
      if(m.error)p.reject(m.error);else p.resolve(m.result);
    }else if(m.type==='event'&&['cs:job','cs:library','cs:device','cs:app'].includes(m.event)){
      for(const item of listeners.values())if(item.event===m.event)callbacks.get(item.handler)?.({payload:m.payload});
    }else if(m.type==='prepare-leave'&&Number.isSafeInteger(m.id)){
      try{await window.__UC_COOLER_FLUSH__?.();send({type:'leave-ready',id:m.id});}
      catch{send({type:'leave-ready',id:m.id,error:'Cooler edits could not be saved. Keep this page open and retry.'});}
    }
  });
})();
