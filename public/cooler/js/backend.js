// The one door between the UI and the Rust core (docs/v2/api-contract.md).
//
//   backend.call(command, args)   -> Promise (rejects with Error{status, code, failedUsb, data})
//   backend.on(event, handler)    -> unsubscribe fn. Events: 'activity' ({rev, deviceBusy, jobs}), 'app' ({})
//   backend.job(jobId, onUpdate)  -> Promise of job.result (rejects with the job's error)
//   backend.mediaUrl(id, file, kind = 'o')   URL on the core's local media server (Range requests, seeking)
//   backend.refresh()             -> re-read the activity snapshot now
//   backend.importFile(file)      -> { id }
//
// Commands are Tauri commands; progress arrives as events (cs:job, cs:library, cs:device, cs:app), never by polling.
const T = window.__UC_COOLER_BRIDGE__;
if (!T) document.body.textContent = 'CoolerScreen must be started from its own window (CoolerScreen.exe).';

const handlers = { activity: new Set(), app: new Set() };
const emit = (ev, payload) => { for (const h of handlers[ev] || []) { try { h(payload); } catch (e) { console.error(e); } } };

function tauriError(x) {
  if (x instanceof Error) return x;
  const o = typeof x === 'string' ? { error: x } : x || {};
  return Object.assign(new Error(o.error || o.message || 'Something went wrong.'), { status: o.status, code: o.code, failedUsb: !!o.failedUsb, data: o });
}

let mediaBase = '';
const jobs = new Map();
const jobWaiters = new Set();
let rev = -1, deviceBusy = false, pushT = null;
// Track requested transient views, including requests still waiting on USB.
// Merely visiting an unavailable screen must not queue redundant cleanup RPCs.
const transientView={twin:false,fit:false,guides:false};

const call = async (command, args = {}) => {
  if(command==='twin_watch')transientView.twin=args.on===true;
  if(command==='fit_cal')transientView.fit=args.action==='start';
  if(command==='calibration_set')transientView.guides=!!args.guides;
  try { return await T.invoke(command, args); } catch (x) { throw tauriError(x); }
};
const listen = (event, fn) =>
  T.invoke('plugin:event|listen', { event, target: { kind: 'Any' }, handler: T.transformCallback(e => fn(e.payload)) });

const CHUNK_BYTES=256*1024,MAX_FILE_BYTES=2*1024*1024*1024;
const pendingImports=new Set();
async function uploadFile(file,kind,originalId){
  if(!Number.isSafeInteger(file?.size)||file.size<0||file.size>MAX_FILE_BYTES||typeof file.name!=='string')throw new Error('Choose a file no larger than 2 GiB.');
  let transferId=null;
  try{
    const started=await call('import_begin',{name:file.name,size:file.size,kind,...(kind==='asset'?{originalId}:{})});
    if(typeof started?.transferId!=='string'||!started.transferId||started.transferId.length>128)throw new Error('The file transfer could not start.');
    transferId=started.transferId;
    for(let offset=0;offset<file.size;offset+=CHUNK_BYTES){
      const end=Math.min(file.size,offset+CHUNK_BYTES),bytes=new Uint8Array(await file.slice(offset,end).arrayBuffer());
      if(bytes.length!==end-offset)throw new Error('The selected file changed during import.');
      const result=await call('import_append',{transferId,offset,bytes:[...bytes]});
      if(result?.received!==end)throw new Error('The file transfer did not receive the expected bytes.');
    }
    const result=await call('import_finish',{transferId});transferId=null;return result;
  }catch(error){
    if(transferId)await call('import_abort',{transferId}).catch(()=>{});
    throw tauriError(error);
  }
}
function trackedUpload(file,kind,originalId){
  const task=uploadFile(file,kind,originalId);pendingImports.add(task);
  task.then(()=>pendingImports.delete(task),()=>pendingImports.delete(task));return task;
}

function snapshot() {
  const now = Date.now();
  for (const [id, j] of jobs) if (j.finishedAt && now - j.finishedAt > 120000) jobs.delete(id);
  return { rev, deviceBusy, jobs: [...jobs.values()] };
}
function push() {
  if (pushT) return;
  pushT = setTimeout(() => { pushT = null; emit('activity', snapshot()); }, 60);
}
async function refresh() {
  try {
    const a = await call('activity');
    jobs.clear();
    for (const j of a.jobs) jobs.set(j.id, j);
    rev = a.rev; deviceBusy = a.deviceBusy;
    emit('activity', snapshot());
  } catch {}
}

export const backend = {
  call,
  endView(){
    const tasks=[];
    if(transientView.twin)tasks.push(call('twin_watch',{on:false}));
    if(transientView.fit)tasks.push(call('fit_cal',{action:'end'}));
    if(transientView.guides)tasks.push(call('calibration_set',{guides:null}));
    return Promise.all(tasks);
  },
  on(event, handler) { handlers[event].add(handler); return () => handlers[event].delete(handler); },
  refresh,
  start() {
    listen('cs:job', j => { jobs.set(j.id, j); for (const w of jobWaiters) w(j); push(); });
    listen('cs:library', p => { rev = p.rev; push(); });
    listen('cs:device', p => { deviceBusy = !!p.busy; push(); });
    listen('cs:app', () => emit('app', {}));
    refresh();
  },
  job(jobId, onUpdate) {
    return new Promise((resolve, reject) => {
      const done = j => {
        onUpdate && onUpdate(j);
        if (j.status === 'done') { jobWaiters.delete(on); resolve(j.result); return true; }
        if (j.status === 'error') { jobWaiters.delete(on); reject(Object.assign(new Error(j.error), { code: j.code, failedUsb: !!j.failedUsb, data: j })); return true; }
        return false;
      };
      const on = j => { if (j.id === jobId) done(j); };
      jobWaiters.add(on);
      call('job_get', { jobId }).then(done, e => { jobWaiters.delete(on); reject(e); }); // it may have finished already
    });
  },
  importFile(file) { return trackedUpload(file,'original'); },
  /** A picture for an image item: copied into the theme's folder. -> { file } (relative to the theme folder) */
  themeAsset(originalId, file) { return trackedUpload(file,'asset',originalId); },
  async flushImports(){await Promise.all([...pendingImports]);},
  /** Called with AppState.mediaBase (the core media server root). */
  setMediaBase(base) { if (base) mediaBase = String(base).replace(/\/+$/, ''); },
  /** Cached media: kind 'o' (original: thumb.jpg, frame.jpg, proxy.mp4, ...) or 'v' (version: preview.mp4, thumb.jpg). */
  mediaUrl(id, file, kind = 'o') { return `${mediaBase}/media/${kind}/${id}/${file}`; },
};
