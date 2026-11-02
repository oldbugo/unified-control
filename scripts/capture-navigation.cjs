// Bounded read-only production MCP capture. Never presses keys or changes layouts.
const fs=require('node:fs'),path=require('node:path'),os=require('node:os');
const {pathToFileURL}=require('node:url');
const assert=require('node:assert/strict');
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
async function main(){
  const node=process.env.UC_MCP_NODE,server=process.env.UC_MCP_SERVER;
  for(const file of [node,server])assert.ok(file&&path.isAbsolute(file)&&fs.existsSync(file),'Set UC_MCP_NODE and UC_MCP_SERVER to explicit installed MCP paths.');
  const seconds=Number(process.argv[2]||300);
  assert.ok(Number.isInteger(seconds)&&seconds>=10&&seconds<=600,'Capture duration must be 10–600 seconds.');
  const output=process.env.UC_NAV_CAPTURE_OUTPUT||path.join(os.tmpdir(),`uc-navigation-${Date.now()}.jsonl`);
  assert.ok(path.isAbsolute(output)&&!output.startsWith('\\\\'),'Capture output must be an absolute local filesystem path.');
  const sdk=path.resolve(__dirname,'../companions/agent-mcp/node_modules/@modelcontextprotocol/client/dist');
  const {Client}=await import(pathToFileURL(path.join(sdk,'index.mjs')).href);
  const {StdioClientTransport}=await import(pathToFileURL(path.join(sdk,'stdio.mjs')).href);
  const transport=new StdioClientTransport({command:node,args:[server],stderr:'pipe'});
  transport.stderr?.resume();
  const client=new Client({name:'uc-navigation-read-only',version:'1'});
  const numeric=value=>value===null||typeof value==='number'||(value&&typeof value==='object'&&!Array.isArray(value)&&Object.values(value).every(numeric));
  let stop=false,lastCompleted=-1;
  const halt=()=>{stop=true;};process.on('SIGINT',halt);process.on('SIGTERM',halt);
  try{
    await client.connect(transport);
    const started=Date.now();
    console.log(`Read-only navigation capture: ${output}; ${seconds}s. No key actions are executed.`);
    while(!stop&&Date.now()-started<seconds*1000){
      const reply=(await client.callTool({name:'uc_get_status',arguments:{}})).structuredContent;
      assert.ok(reply?.ok,'UC status is unavailable.');
      const timing=reply.result.dock?.timing;
      assert.ok(timing&&numeric(timing),'This running UC build does not provide numerical navigation timings.');
      const sample={elapsedMs:Date.now()-started,timing};
      fs.appendFileSync(output,JSON.stringify(sample)+'\n',{encoding:'utf8'});
      if(timing.navigationsCompleted!==lastCompleted){
        console.log(JSON.stringify({elapsedMs:sample.elapsedMs,completed:timing.navigationsCompleted,aborted:timing.navigationsAborted,lastNavigation:timing.lastNavigation}));
        lastCompleted=timing.navigationsCompleted;
      }
      await delay(2000);
    }
    console.log('Capture complete. UC remains running.');
  }finally{
    process.removeListener('SIGINT',halt);process.removeListener('SIGTERM',halt);
    await client.close();
  }
}
main().catch(error=>{console.error(error.message);process.exitCode=1;});
