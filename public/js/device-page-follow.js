// Observes the last page delivered to the dock. Following is an editor-only
// selection change: it never saves a profile or sends navigation to hardware.
export class DevicePageFollower {
  constructor(){this.current=null;this.pending=null;}
  observe(status){
    const profile=status?.currentProfile,page=status?.currentPage;
    if(status?.state!=='connected'||status.enabled!==true||typeof profile!=='string'||typeof page!=='string'){
      this.current=null;this.pending=null;return;
    }
    if(this.current?.profile!==profile||this.current?.page!==page){
      this.current={profile,page};this.pending=this.current;
    }
  }
  requestSync(){this.pending=this.current;}
  manualSelection(){this.pending=null;}
  take(draft,{enabled,ready,busy,visible}){
    if(!enabled){this.pending=null;return null;}
    if(!ready||busy||!visible||!this.pending)return null;
    const profile=draft.profiles.findIndex(item=>item.id===this.pending.profile);
    const page=profile<0?-1:draft.profiles[profile].pages.findIndex(item=>item.id===this.pending.page);
    if(profile<0||page<0)return null;
    this.pending=null;
    return draft.profile===profile&&draft.page===page?null:{profile,page};
  }
}
