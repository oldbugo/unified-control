// An external layout write borrows the editor only after its own edits finish.
// The native host owns the transaction; the editor never predicts its revision.
export class AgentEditorLease {
  constructor({invoke,busy,flush,lock,adopt,reload,onError}) {
    Object.assign(this,{invoke,busy,flush,lock,adopt,reload,onError});
    this.current=null;this.preserveDraft=false;this.queue=Promise.resolve();
  }
  event(message) {
    this.queue=this.queue.then(()=>this.receive(message)).catch(error=>this.onError(error));
    return this.queue;
  }
  async receive({event,payload={}}) {
    const leaseId=payload.leaseId;
    if(typeof leaseId!=='string')return;
    if(event==='prepare') {
      if(this.current || this.busy()) {
        await this.invoke('agent_editor_ready',{leaseId,saved:false});return;
      }
      this.current=leaseId;this.preserveDraft=false;this.lock(true);
      let saved=false;
      try { saved=await this.flush(); } catch { /* Keep the failed draft intact. */ }
      this.preserveDraft=!saved;
      try { await this.invoke('agent_editor_ready',{leaseId,saved}); }
      catch(error) { this.onError(error); } // Release event still owns unlocking.
      return;
    }
    if(leaseId!==this.current)return;
    if(event==='committed') {
      await this.adopt(payload);
      return;
    }
    if(event==='released') {
      try {
        // A timeout/rejected CAS can follow a flush. Always recover the host's
        // accepted state before allowing new edits, except for a refused save.
        if(!this.preserveDraft)await this.reload();
      } finally {
        this.current=null;this.preserveDraft=false;this.lock(false);
      }
    }
  }
}
