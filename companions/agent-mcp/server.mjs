// Local stdio MCP adapter. The running UC host owns all app data and devices.
import {McpServer} from '@modelcontextprotocol/server';
import {StdioServerTransport, serveStdio} from '@modelcontextprotocol/server/stdio';
import * as z from 'zod/v4';
import {spawn} from 'node:child_process';
import {fileURLToPath, pathToFileURL} from 'node:url';
import path from 'node:path';

export const MAX_REQUEST_BYTES=1024*1024;
export const MAX_RESPONSE_BYTES=12*1024*1024;
const REQUEST_TIMEOUT_MS=15000;
const id=z.string().min(1).max(64).regex(/^[A-Za-z0-9_-]+$/);
const revision=z.string().min(1).max(160);
const face=z.strictObject({
  kind:z.enum(['text','icon','picture','clock','date','claude','codex','reset','trend','cpu','gpu','ram']),
  text:z.string().max(40),asset:id.optional(),icon:z.string().max(64).optional(),usageView:z.string().max(64).optional(),
});
const target={profileId:id,pageId:id,key:z.number().int().min(1).max(14)};
const operation=z.discriminatedUnion('op',[
  z.strictObject({op:z.literal('set_face'),...target,face:z.enum(['a','b']),value:face}),
  z.strictObject({op:z.literal('set_action'),...target,action:z.enum(['none','hotkey','media','launch','url','text','page','monitor','folder','back']),value:z.string().max(4096)}),
  z.strictObject({op:z.literal('rename_page'),profileId:id,pageId:id,name:z.string().min(1).max(80)}),
  z.strictObject({op:z.literal('create_page'),profileId:id,name:z.string().min(1).max(80)}),
]);
const change=z.strictObject({expectedRevision:revision,ops:z.array(operation).min(1).max(64)});

function failure(code,message){return {ok:false,error:{code,message}};}
function validEnvelope(value){
  if(!value||typeof value!=='object'||Array.isArray(value)||typeof value.ok!=='boolean')return false;
  if(value.ok)return Object.hasOwn(value,'result');
  return !!value.error&&typeof value.error==='object'&&!Array.isArray(value.error)
    &&typeof value.error.code==='string'&&/^[A-Za-z0-9_-]{1,64}$/.test(value.error.code)
    &&typeof value.error.message==='string'&&value.error.message.length<=512
    &&!/[\x00-\x1f\x7f]/.test(value.error.message);
}

// Exported for isolated native tests. The production entry has no executable,
// command, environment or path override: it uses its installed sibling only.
export function createProcessRequester(command,args=['--agent-request'],{timeoutMs=REQUEST_TIMEOUT_MS}={}){
  return (method,params={},signal)=>new Promise(resolve=>{
    let encoded;
    try{encoded=Buffer.from(JSON.stringify({method,params}));}catch{return resolve(failure('invalid_request','The UC request could not be encoded.'));}
    if(encoded.length>MAX_REQUEST_BYTES)return resolve(failure('request_too_large','The UC request exceeds its size limit.'));
    if(signal?.aborted)return resolve(failure('cancelled','The UC request was cancelled.'));
    let child,complete=false,total=0,chunks=[];
    const finish=(result,stopChild=false)=>{
      if(complete)return;complete=true;clearTimeout(timer);signal?.removeEventListener('abort',cancel);
      // This child is a request-only proxy, never the running UC host. Ending it
      // cannot cancel a transaction already admitted by the host. Read back the
      // current revision before retrying any timed-out or cancelled write.
      if(stopChild&&child&&!child.killed)child.kill();
      resolve(result);
    };
    const cancel=()=>finish(failure('cancelled','The UC request was cancelled; read the layout before retrying a change.'),true);
    const timer=setTimeout(()=>finish(failure('timeout','UC did not answer in time; read the layout before retrying a change.'),true),timeoutMs);
    try{child=spawn(command,args,{windowsHide:true,stdio:['pipe','pipe','pipe']});}
    catch{return finish(failure('app_unavailable','The Unified Control request helper is unavailable.'));}
    signal?.addEventListener('abort',cancel,{once:true});
    child.on('error',()=>finish(failure('app_unavailable','The Unified Control request helper is unavailable.')));
    child.stdin.on('error',()=>{});
    // Drain helper stderr without exposing or retaining OS paths or payloads.
    child.stderr.on('data',()=>{});
    child.stdout.on('data',chunk=>{
      total+=chunk.length;
      if(total>MAX_RESPONSE_BYTES)return finish(failure('invalid_response','The UC response exceeds its size limit.'),true);
      chunks.push(chunk);
    });
    child.on('close',()=>{
      if(complete)return;
      try{
        const value=JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(Buffer.concat(chunks)));
        finish(validEnvelope(value)?value:failure('invalid_response','UC returned an invalid response.'));
      }catch{finish(failure('invalid_response','UC returned an invalid response.'));}
    });
    child.stdin.end(encoded);
  });
}

const definitions=[
  ['uc_get_status','get_status','Read UC version, local agent access, device health and available capabilities. This also works when agent access is off, with restricted information.',z.strictObject({}),true],
  ['uc_get_layout','get_layout','Read the selected dock page, stable profile/page IDs and the current revision. Supply IDs to read another page. Keys are numbered 1–14; 14 is the wide display.',z.strictObject({profileId:id.optional(),pageId:id.optional()}),true],
  ['uc_list_monitors','list_monitors','Discover monitor choices without changing display topology or audio. Use the returned keys when configuring a Monitor Toggle action.',z.strictObject({}),true],
  ['uc_preview_changes','preview_changes','Validate and preview a bounded layout change without saving or executing key actions. Read the current revision first.',change,true],
  ['uc_apply_changes','apply_changes','Apply one atomic revision-checked layout change. UC may refuse busy editor work or stale revisions. Changes are inert configuration; no key action is executed. Read back after timeout before retrying.',change,false],
  ['uc_undo_changes','undo_changes','Undo an agent layout transaction using its undo token and the current revision. Later edits can make the token stale; UC refuses rather than overwrite them.',z.strictObject({expectedRevision:revision,undoToken:z.string().min(1).max(160)}),false],
  ['uc_get_job','get_job','Read a UC job result. A saved layout and a physically delivered layout are different states; inspect the returned status.',z.strictObject({jobId:z.string().min(1).max(160)}),true],
];

export function createServer({request}){
  const server=new McpServer({name:'unified-control',version:'0.1.0'}, {instructions:
    'Control the running local Unified Control app through its validated core. UC must already be running, and Local agent access must be enabled in General for layout access. Never start another device engine or edit app-data files. Read the layout and revision, preview a change, then apply with that revision. A busy or conflict result requires a fresh read; preserve the user\'s edits. Keys are 1-based, 1–14, with exactly faces a and b. Physical key actions are not exposed by this server. No firmware, USB reset, shell, credentials or arbitrary-command tools are available.'});
  for(const [name,method,description,inputSchema,readOnly]of definitions){
    server.registerTool(name,{description,inputSchema,annotations:{readOnlyHint:readOnly,destructiveHint:!readOnly,idempotentHint:readOnly,openWorldHint:false}},async(params,context)=>{
      let result;
      try{result=await request(method,params,context?.signal);}catch{result=failure('request_failed','The UC request failed.');}
      if(!validEnvelope(result))result=failure('invalid_response','UC returned an invalid response.');
      return {content:[{type:'text',text:JSON.stringify(result)}],structuredContent:result,isError:!result.ok};
    });
  }
  return server;
}

export function startStdio({request}){
  // The SDK's shared factory serves modern discovery and legacy initialize
  // clients using the same tool implementations and bounded stdio framing.
  return serveStdio(()=>createServer({request}),{legacy:'serve',transport:new StdioServerTransport(process.stdin,process.stdout,{maxBufferSize:MAX_REQUEST_BYTES}),onerror:()=>{}});
}

if(process.argv[1]&&import.meta.url===pathToFileURL(path.resolve(process.argv[1])).href){
  if(process.argv.length!==2){process.stderr.write('Unified Control MCP accepts no command-line overrides.\n');process.exitCode=2;}
  else{
    const executable=fileURLToPath(new URL('../UnifiedControl.exe',import.meta.url));
    const request=createProcessRequester(executable);
    const connection=startStdio({request});
    const close=()=>{void connection.close();};
    process.once('SIGINT',close);process.once('SIGTERM',close);
  }
}
