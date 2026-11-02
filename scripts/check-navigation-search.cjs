// Headless browser drafts only; no native app, account, device or OS action.
const fs=require('node:fs'),path=require('node:path'),http=require('node:http'),os=require('node:os'),assert=require('node:assert/strict');
const {chromium}=require(process.env.UC_PLAYWRIGHT_MODULE||'playwright');
const root=path.resolve(__dirname,'../public');
const server=http.createServer((req,res)=>{
  const file=path.resolve(root,'.'+new URL(req.url,'http://localhost').pathname.replace(/\/$/,'/index.html'));
  if(!file.startsWith(root+path.sep)){res.writeHead(403);return res.end();}
  fs.readFile(file,(error,data)=>{res.writeHead(error?404:200,{'Content-Type':file.endsWith('.js')?'text/javascript':file.endsWith('.css')?'text/css':'text/html'});res.end(error?'Not found':data);});
});
async function main(){
  let browser;
  try{
    await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
    browser=await chromium.launch({headless:true});
    const output=path.join(os.tmpdir(),'uc-navigation-search-preview');fs.mkdirSync(output,{recursive:true});
    for(const [width,height] of [[1440,1000],[1100,800],[800,650]]){
      const page=await browser.newPage({viewport:{width,height}}),errors=[];
      page.on('pageerror',error=>errors.push(error.message));
      await page.goto(`http://127.0.0.1:${server.address().port}/`);
      const input=page.getByRole('searchbox',{name:'Find a page'}),visible=()=>page.locator('.nav-item:not([hidden])');
      assert.equal(await visible().count(),6);
      const draft=await page.evaluate(()=>localStorage.getItem('uc.interface-prototype.v1'));
      const originalHeading=await page.locator('main h1').textContent();
      await input.fill('  AI  USAGE ');
      assert.equal(await visible().count(),1);assert.equal(await visible().getAttribute('data-page'),'dock');
      assert.equal(await page.locator('main h1').textContent(),originalHeading,'filtering must not navigate');
      assert.equal(await page.evaluate(()=>localStorage.getItem('uc.interface-prototype.v1')),draft,'filtering must not save layouts or preferences');
      await input.fill('mCp');await page.keyboard.press('Tab');
      assert.equal(await visible().evaluate(el=>el===document.activeElement),true,'Tab skips hidden results');
      await page.keyboard.press('Escape');
      assert.equal(await input.inputValue(),'');assert.equal(await visible().count(),6);
      assert.equal(await input.evaluate(el=>el===document.activeElement),true);
      await input.fill('MCP');await page.keyboard.press('Enter');
      assert.equal(await page.locator('main h1').textContent(),'General');
      assert.equal(await visible().getAttribute('aria-current'),'page');
      assert.equal(await visible().evaluate(el=>el===document.activeElement),true,'activation keeps focus on the result');
      await page.keyboard.press('Escape');await input.fill('<img id=unsafe src=x>');await page.keyboard.press('Enter');
      assert.equal(await visible().count(),0);assert.equal(await page.locator('.nav-search-status').textContent(),'No matching pages');
      assert.equal(await page.locator('#unsafe').count(),0);assert.equal(await page.locator('main h1').textContent(),'General');
      await page.keyboard.press('Escape');await input.fill('usage');await page.keyboard.press('Enter');
      assert.equal(await page.locator('main h1').textContent(),'Stream dock');
      await page.keyboard.press('Escape');await page.locator('[data-key="6"]').click();
      await page.locator('#face-text').fill('Editing is separate');
      assert.equal(await visible().count(),6,'typing in the editor does not filter navigation');
      await input.fill('screen');
      assert.equal(await visible().getAttribute('data-page'),'cooler');
      assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));
      await page.screenshot({path:path.join(output,`uc-page-search-${width}.png`),fullPage:true});
      assert.deepEqual(errors,[]);await page.close();
    }
    const compact=await browser.newPage({viewport:{width:600,height:700}}),compactErrors=[];
    compact.on('pageerror',error=>compactErrors.push(error.message));
    await compact.goto(`http://127.0.0.1:${server.address().port}/`);
    const search=compact.getByRole('searchbox',{name:'Find a page'}),toggle=compact.getByRole('button',{name:'Find a page',exact:true});
    assert.equal(await search.isVisible(),false);
    await toggle.click();assert.equal(await search.isVisible(),true);
    assert.equal(await search.evaluate(el=>el===document.activeElement),true);
    await search.fill('about');await compact.keyboard.press('Enter');
    assert.equal(await compact.locator('.nav-item[data-page="about"]').getAttribute('aria-current'),'page');
    assert.equal(await compact.locator('main h1').textContent(),'Unified Control');
    await compact.keyboard.press('Escape');await compact.keyboard.press('Escape');
    assert.equal(await search.isVisible(),false);assert.equal(await toggle.getAttribute('aria-expanded'),'false');
    assert.equal(await toggle.evaluate(el=>el===document.activeElement),true);
    assert.ok(await compact.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));
    assert.deepEqual(compactErrors,[]);
    console.log('PASS: page/alias/token search, no-save/no-navigation filtering, keyboard activation/tab/Escape, empty and escaped queries, editor isolation, three widths and compact search access. Browser fixtures only.');
    console.log(output);
  }finally{if(browser)await browser.close();await new Promise(resolve=>server.close(resolve));}
}
main().catch(error=>{console.error(error);process.exitCode=1;});
