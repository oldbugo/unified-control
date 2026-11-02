// Validate installed/staged resources without starting UC or any account source.
const {spawn}=require('node:child_process');
const fs=require('node:fs');
const path=require('node:path');
const os=require('node:os');
const assert=require('node:assert/strict');
async function main(){
  assert.ok(process.env.UC_RESOURCE_DIR,'Set UC_RESOURCE_DIR to the installed or staged resource folder.');
  const root=path.resolve(process.env.UC_RESOURCE_DIR);
  const node=path.join(root,'node','node.exe'),reader=path.join(root,'usage-reader','reader.mjs');
  for(const file of [node,reader,path.join(root,'node','LICENSE'),path.join(root,'monitor','WindowsDisplayControl.ps1'),path.join(root,'monitor','WindowsAudioDefaults.cs')])assert.ok(fs.existsSync(file),`Missing bundled resource: ${file}`);
  const home=fs.mkdtempSync(path.join(os.tmpdir(),'uc-bundled-reader-'));
  const child=spawn(node,[reader],{windowsHide:true,env:{...process.env,UC_USAGE_HOME:home},stdio:['pipe','pipe','ignore']});
  let next=0,buffer='',failure;const pending=new Map();
  const exited=new Promise(resolve=>child.once('exit',code=>resolve(code)));
  child.on('error',error=>{failure=error;for(const {reject}of pending.values())reject(error);pending.clear();});
  child.stdout.setEncoding('utf8');child.stdout.on('data',data=>{
    buffer+=data;let end;
    while((end=buffer.indexOf('\n'))>=0){const line=buffer.slice(0,end);buffer=buffer.slice(end+1);try{const message=JSON.parse(line),request=pending.get(message.id);if(request){pending.delete(message.id);clearTimeout(request.timer);message.error?request.reject(new Error(message.error)):request.resolve(message.result);}}catch{failure=new Error('Invalid bundled reader output');}}
  });
  const request=(method,fields={})=>new Promise((resolve,reject)=>{
    if(failure)return reject(failure);const id=++next;
    const timer=setTimeout(()=>{pending.delete(id);reject(new Error('Bundled reader timed out'));},5000);
    pending.set(id,{resolve,reject,timer});child.stdin.write(JSON.stringify({id,method,...fields})+'\n');
  });
  try{
    assert.equal((await request('views')).length,16);
    assert.equal((await request('health')).alive,true);
    assert.equal((await request('snapshot',{provider:'codex'})).status,'inactive');
    const result=await request('render',{provider:'codex',view:'rings',width:196,height:196});
    assert.equal(result.snapshot.status,'unavailable');
    assert.equal(result.snapshot.provenanceVerified,false);
    const png=Buffer.from(result.png);assert.equal(png.toString('hex',0,8),'89504e470d0a1a0a');
    assert.equal(png.readUInt32BE(16),196);assert.equal(png.readUInt32BE(20),196);
    assert.equal((await request('snapshot',{provider:'codex'})).status,'inactive');
    await request('shutdown');child.stdin.end();
    const code=await Promise.race([exited,new Promise(resolve=>{const timer=setTimeout(()=>resolve(null),3000);timer.unref();})]);
    assert.equal(code,0);
    console.log('PASS: bundled Node, rasterizer, fonts, monitor helpers and notices; inactive private-pipe render; clean shutdown. No app/device/account activated.');
  }finally{
    for(const {timer}of pending.values())clearTimeout(timer);
    if(child.exitCode===null)child.kill();
  }
}
main().catch(error=>{console.error(error.message);process.exitCode=1;});
