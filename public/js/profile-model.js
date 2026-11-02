// The editor uses indices; the persistent profile store uses stable IDs.
export function toNative(draft) {
  for (const p of draft.profiles) {
    p.id ||= `profile-${crypto.randomUUID()}`;
    for (const pg of p.pages) pg.id ||= `page-${crypto.randomUUID()}`;
  }
  const current = draft.profiles[draft.profile];
  return {
    version: 1,
    currentProfile: current.id,
    currentPage: current.pages[draft.page].id,
    profiles: draft.profiles.map(p => ({id:p.id, name:p.name, pages:p.pages.map(pg => ({
      id:pg.id, name:pg.name, keys:pg.keys.map(k => ({
        a:{...k.a}, b:{...k.b}, action:k.action,
        value:['page','folder'].includes(k.action) ? (p.pages[Number(k.value)]?.id || k.value) : k.value,
      })),
    }))})),
  };
}

export function fromNative(snapshot, current='home') {
  const profiles = snapshot.profiles.map(p => ({id:p.id, name:p.name, pages:p.pages.map(pg => ({
    id:pg.id, name:pg.name, keys:pg.keys.map(k => ({
      a:{...k.a}, b:{...k.b}, action:k.action,
      value:['page','folder'].includes(k.action) ? String(p.pages.findIndex(target=>target.id===k.value)) : k.value,
    })),
  }))}));
  const profile = profiles.findIndex(p=>p.id===snapshot.currentProfile);
  const page = profiles[profile].pages.findIndex(pg=>pg.id===snapshot.currentPage);
  return {version:1,current,profile,page,profiles};
}

// One write in flight. Later edits replace the queued snapshot, never race an
// older disk write. Errors are surfaced and the last edit remains available to retry.
export class ProfileWriter {
  constructor(invoke, onStatus) { this.invoke=invoke; this.onStatus=onStatus; this.pending=null; this.running=false; this.failed=false; this.revision=undefined; this.completion=Promise.resolve(); }
  adoptRevision(revision) {
    if (this.running || this.pending || this.failed) throw new Error('Finish pending layout edits before adopting another revision');
    this.revision=revision;
  }
  enqueue(snapshot) {
    this.pending=JSON.parse(JSON.stringify(snapshot));
    this.failed=false;
    if (!this.running) this.completion=this.drain();
  }
  async flush() {
    if (this.failed && this.pending) this.completion=this.drain();
    await this.completion;
    return !this.running && !this.failed && !this.pending;
  }
  async drain() {
    if (this.running) return;
    this.failed=false;
    this.running=true;
    this.onStatus('saving');
    while (this.pending) {
      const snapshot=this.pending;
      this.pending=null;
      try {
        const result=await this.invoke('profiles_save',{snapshot,expectedRevision:this.revision});
        if(typeof result?.revision==='string')this.revision=result.revision;
      }
      catch (error) {
        // An older edit can be invalid while a newer one is already valid
        // (for example clearing then retyping a profile name).
        if (this.pending && error?.code!=='revision_conflict') continue;
        this.pending ||= snapshot;
        this.running=false; this.failed=true;
        this.onStatus('failed',error);
        return;
      }
    }
    this.running=false;
    this.onStatus('saved');
  }
}
