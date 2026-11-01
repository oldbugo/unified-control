// Official MCP client ↔ stdio adapter ↔ request-only fixture. No UC app,
// profiles, Windows settings, hardware, account data or desktop window is used.
const assert=require('node:assert/strict');
const fs=require('node:fs');
const os=require('node:os');
const path=require('node:path');
const {spawn}=require('node:child_process');
const {pathToFileURL}=require('node:url');
const root=path.resolve(__dirname,'..');
const adapter=path.join(root,'companions/agent-mcp/server.mjs');
const sdk=path.join(root,'companions/agent-mcp/node_modules/@modelcontextprotocol/client/dist');
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
async function deadline(promise,ms=4000){let timer;try{return await Promise.race([promise,new Promise((_,reject)=>{timer=setTimeout(()=>reject(Error('Fixture deadline exceeded')),ms);})]);}finally{clearTimeout(timer);}}

(async()=>{
  const {Client}=await import(pathToFileURL(path.join(sdk,'index.mjs')).href);
  const {StdioClientTransport}=await import(pathToFileURL(path.join(sdk,'stdio.mjs')).href);
  const {createProcessRequester,MAX_REQUEST_BYTES}=await import(pathToFileURL(adapter).href);
  const scratch=fs.mkdtempSync(path.join(os.tmpdir(),'uc-agent-mcp-fixture-'));
  const fixture=path.join(scratch,'request-fixture.mjs');
  const entry=path.join(scratch,'entry.mjs');
  const calls=path.join(scratch,'calls.jsonl');
  fs.writeFileSync(fixture,`
    import fs from 'node:fs';
    let input='';for await(const chunk of process.stdin)input+=chunk;
    const request=JSON.parse(input);
    if(process.argv[2]!=='--agent-request')process.exit(4);
    fs.appendFileSync(process.env.UC_MCP_FIXTURE_CALLS,JSON.stringify(request)+'\\n');
    const mode=process.env.UC_MCP_FIXTURE_MODE;
    if(mode==='hang'){setTimeout(()=>{},60000);}
    else if(mode==='oversize')process.stdout.write('x'.repeat(12*1024*1024+1));
    else if(mode==='malformed')process.stdout.write('private invalid result');
    else if(mode==='utf8')process.stdout.write(Buffer.from([0xff]));
    else if(mode==='backend-error')process.stdout.write(JSON.stringify({ok:false,error:{code:'agent_access_disabled',message:'Local agent access is disabled.'}}));
    else{
      if(mode==='stderr')process.stderr.write('PRIVATE-STDERR-PATH-AND-DEVICE-DATA');
      const result=request.method==='get_status'?{appVersion:'fixture',agentAccess:true}:request.method==='get_layout'?{revision:'fixture-r1',profileId:'p',pageId:'pg',keys:[]}:{method:request.method,params:request.params,revision:'fixture-r2'};
      process.stdout.write(JSON.stringify({ok:true,result}));
    }
  `);
  fs.writeFileSync(entry,`import {startStdio,createProcessRequester} from ${JSON.stringify(pathToFileURL(adapter).href)};startStdio({request:createProcessRequester(process.execPath,[${JSON.stringify(fixture)},'--agent-request'],{timeoutMs:500})});`);
  const clients=[];
  async function connect(mode='good'){
    const transport=new StdioClientTransport({command:process.execPath,args:[entry],env:{...process.env,UC_MCP_FIXTURE_MODE:mode,UC_MCP_FIXTURE_CALLS:calls},stderr:'pipe'});
    let stderr='';transport.stderr.on('data',chunk=>{stderr+=chunk;});
    const client=new Client({name:'uc-fixture-client',version:'1.0.0'});
    clients.push(client);await deadline(client.connect(transport));return {client,transport,stderr:()=>stderr};
  }
  try{
    const {client,stderr}=await connect();
    const tools=(await client.listTools()).tools;
    assert.equal(tools.length,7);
    assert.deepEqual(tools.map(tool=>tool.name).sort(),['uc_get_status','uc_get_layout','uc_list_monitors','uc_preview_changes','uc_apply_changes','uc_undo_changes','uc_get_job'].sort());
    for(const tool of tools){assert.equal(tool.annotations.openWorldHint,false);assert.equal(tool.inputSchema.additionalProperties,false);}
    assert.equal(tools.find(tool=>tool.name==='uc_apply_changes').annotations.readOnlyHint,false);
    assert.equal(tools.find(tool=>tool.name==='uc_get_layout').annotations.readOnlyHint,true);
    let response=await client.callTool({name:'uc_get_status',arguments:{}});
    assert.equal(response.structuredContent.result.appVersion,'fixture');
    assert.equal(response.isError,false);assert.deepEqual(JSON.parse(response.content[0].text),response.structuredContent);
    response=await client.callTool({name:'uc_get_layout',arguments:{profileId:'p',pageId:'pg'}});
    assert.equal(response.structuredContent.result.revision,'fixture-r1');
    for(const tool of ['uc_preview_changes','uc_apply_changes']){
      response=await client.callTool({name:tool,arguments:{expectedRevision:'fixture-r1',ops:[{op:'set_face',profileId:'p',pageId:'pg',key:14,face:'a',value:{kind:'text',text:'Wide'}}]}});
      assert.equal(response.structuredContent.result.params.ops[0].key,14);
      assert.equal(response.structuredContent.result.method,tool.replace('uc_',''));
    }
    response=await client.callTool({name:'uc_list_monitors',arguments:{}});assert.equal(response.structuredContent.result.method,'list_monitors');
    response=await client.callTool({name:'uc_undo_changes',arguments:{expectedRevision:'fixture-r2',undoToken:'undo-fixture'}});assert.equal(response.structuredContent.result.params.undoToken,'undo-fixture');
    response=await client.callTool({name:'uc_get_job',arguments:{jobId:'fixture-job'}});assert.equal(response.structuredContent.result.params.jobId,'fixture-job');
    const count=()=>fs.readFileSync(calls,'utf8').trim().split('\n').length;
    const before=count();
    for(const args of [
      {expectedRevision:'fixture-r1',ops:[{op:'set_face',profileId:'p',pageId:'pg',key:0,face:'a',value:{kind:'text',text:'No'}}]},
      {expectedRevision:'fixture-r1',ops:[{op:'shell',command:'never-run'}]},
      {expectedRevision:'fixture-r1',ops:[{op:'create_page',profileId:'p',name:'Page',path:'never-read'}]},
      {expectedRevision:'fixture-r1',ops:Array.from({length:65},()=>({op:'create_page',profileId:'p',name:'Page'}))},
    ]){
      const result=await client.callTool({name:'uc_apply_changes',arguments:args});assert.equal(result.isError,true);
    }
    assert.equal(count(),before,'invalid tool arguments must never reach the native request proxy');
    assert.equal(stderr(),'');
    for(const [mode,code]of [['backend-error','agent_access_disabled'],['malformed','invalid_response'],['utf8','invalid_response'],['oversize','invalid_response'],['hang','timeout']]){
      const other=await connect(mode);const result=await deadline(other.client.callTool({name:'uc_get_status',arguments:{}}));
      assert.equal(result.isError,true);assert.equal(result.structuredContent.error.code,code);assert.equal(JSON.stringify(result).includes('PRIVATE'),false);
      await deadline(other.client.close());
    }
    const noisy=await connect('stderr');response=await noisy.client.callTool({name:'uc_get_status',arguments:{}});assert.equal(response.isError,false);assert.equal(noisy.stderr(),'');await deadline(noisy.client.close());
    const missing=createProcessRequester(path.join(scratch,'not-installed.exe'));
    response=await missing('get_status',{});assert.equal(response.error.code,'app_unavailable');assert.equal(JSON.stringify(response).includes(scratch),false);
    const oversized=await missing('get_status',{padding:'x'.repeat(MAX_REQUEST_BYTES)});assert.equal(oversized.error.code,'request_too_large');
    // An older MCP opening still discovers and calls the same tools. Raw wire
    // here intentionally verifies compatibility independently of SDK Client.
    const raw=spawn(process.execPath,[entry],{windowsHide:true,env:{...process.env,UC_MCP_FIXTURE_CALLS:calls},stdio:['pipe','pipe','pipe']});
    let buffer='';const messages=[];raw.stdout.on('data',chunk=>{buffer+=chunk;let at;while((at=buffer.indexOf('\n'))>=0){messages.push(JSON.parse(buffer.slice(0,at)));buffer=buffer.slice(at+1);}});raw.stderr.resume();
    const send=value=>raw.stdin.write(JSON.stringify(value)+'\n');
    const reply=async id=>{for(let n=0;n<100;n++){const message=messages.find(value=>value.id===id);if(message)return message;await delay(20);}throw Error('Legacy fixture response missing');};
    send({jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-03-26',capabilities:{},clientInfo:{name:'legacy-fixture',version:'1'}}});
    assert.equal((await reply(1)).result.protocolVersion,'2025-03-26');send({jsonrpc:'2.0',method:'notifications/initialized'});
    send({jsonrpc:'2.0',id:2,method:'tools/list',params:{}});assert.equal((await reply(2)).result.tools.length,7);
    send({jsonrpc:'2.0',id:3,method:'tools/call',params:{name:'uc_get_status',arguments:{}}});assert.equal((await reply(3)).result.structuredContent.result.appVersion,'fixture');
    const rawExit=new Promise(resolve=>raw.once('exit',resolve));raw.stdin.end();await deadline(rawExit);
    const huge=spawn(process.execPath,[entry],{windowsHide:true,env:{...process.env,UC_MCP_FIXTURE_CALLS:calls},stdio:['pipe','pipe','pipe']});
    huge.stdin.on('error',()=>{});huge.stdout.resume();huge.stderr.resume();
    const hugeExit=new Promise(resolve=>huge.once('exit',resolve));
    huge.stdin.end('x'.repeat(MAX_REQUEST_BYTES+1));await deadline(hugeExit);
    const override=spawn(process.execPath,[adapter,'--app-exe','never-run.exe'],{windowsHide:true,stdio:['ignore','pipe','pipe']});
    let overrideOut='';override.stdout.on('data',chunk=>{overrideOut+=chunk;});override.stderr.resume();
    assert.equal(await deadline(new Promise(resolve=>override.once('exit',resolve))),2);
    assert.equal(overrideOut,'','invalid production flags must not corrupt stdout or launch a proxy');
    await deadline(client.close());
    console.log('PASS: official SDK stdio discovery/calls; legacy MCP initialize; seven allowlisted tools; typed validation; bounded native request/response; disabled/missing/error/timeout states; private stderr suppression; clean client exit. Fixtures only, no real UC or device access.');
  }finally{for(const client of clients){try{await client.close();}catch{}}}
})().catch(error=>{console.error(error);process.exitCode=1;});
