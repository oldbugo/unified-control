const { chromium } = require(process.env.UC_PLAYWRIGHT_MODULE || 'playwright');
const http = require('node:http');
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const root = path.resolve(__dirname, '../public');
const output = process.env.UC_PREVIEW_OUTPUT || path.join(require('node:os').tmpdir(), 'uc-interface-preview');
fs.mkdirSync(output, {recursive:true});
const server = http.createServer((req,res) => {
  const file = path.resolve(root, '.' + new URL(req.url,'http://localhost').pathname.replace(/\/$/,'/index.html'));
  if (!file.startsWith(path.resolve(root) + path.sep)) {res.writeHead(403);return res.end();}
  fs.readFile(file,(err,data)=>{res.writeHead(err?404:200,{'Content-Type':file.endsWith('.js')?'text/javascript':file.endsWith('.css')?'text/css':'text/html'});res.end(err?'Not found':data);});
});
(async()=>{
  await new Promise(r=>server.listen(0,'127.0.0.1',r));
  let browser;
  try {
    browser = await chromium.launch({headless:true});
    const page = await browser.newPage({viewport:{width:1440,height:1000}});
    const errors=[];page.on('pageerror',e=>errors.push(e.message));
    const base = `http://127.0.0.1:${server.address().port}`;
    await page.goto(base);
    assert.equal(await page.locator('.nav-item').count(),6);
    await page.locator('[data-page="dock"]').click();
    assert.equal(await page.locator('[data-key]').count(),14);
    assert.equal(await page.locator('.allowance-values').first().textContent(),'——');
    await page.locator('[data-key="6"]').click();
    await page.locator('#face-kind').selectOption('text');
    await page.locator('#face-text').fill('Focus');
    await page.locator('[data-edit-face="b"]').click();
    await page.locator('#face-kind').selectOption('text');
    await page.locator('#face-text').fill('Break');
    await page.locator('#action-kind').selectOption('hotkey');
    await page.locator('#action-value').fill('Ctrl + Shift + M');
    await page.locator('[data-do="preview-flip"]').first().click();
    assert.match(await page.locator('[data-key="6"]').textContent(),/Focus/);
    await page.locator('[data-do="preview-flip"]').first().click();
    assert.match(await page.locator('[data-key="6"]').textContent(),/Break/);
    await page.reload();
    await page.locator('[data-key="6"]').click();
    await page.locator('[data-edit-face="b"]').click();
    assert.equal(await page.locator('#face-text').inputValue(),'Break');
    assert.equal(await page.locator('#action-value').inputValue(),'Ctrl + Shift + M');
    await page.locator('#face-text').fill('<img src=x onerror=alert(1)>');
    assert.equal(await page.locator('[data-key="6"] img').count(),0);
    await page.locator('#face-text').fill('Break');
    await page.locator('[data-do="add-page"]').click();
    assert.equal(await page.locator('[data-dock-page]').count(),2);
    await page.locator('[data-do="add-profile"]').click();
    assert.equal(await page.locator('#profile-picker option').count(),2);
    await page.locator('#profile-picker').selectOption('0');
    await page.locator('[data-dock-page="0"]').click();
    await page.locator('[data-edit-face="a"]').click();
    for (const [w,h] of [[1440,1000],[1100,800],[800,650]]) {
      await page.setViewportSize({width:w,height:h});
      await page.locator('#pane').evaluate(el=>el.scrollTop=0);
      await page.screenshot({path:path.join(output,`uc-dock-${w}.png`),fullPage:true});
      assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>window.innerWidth),false,`horizontal overflow at ${w}`);
    }
    await page.setViewportSize({width:1440,height:1000});
    await page.locator('[data-page="home"]').click();
    await page.screenshot({path:path.join(output,'uc-home.png'),fullPage:true});
    await page.locator('[data-page="cooler"]').click();
    assert.match(await page.locator('#pane').textContent(),/integration is planned/);
    await page.screenshot({path:path.join(output,'uc-cooler.png'),fullPage:true});
    await page.locator('[data-page="dial"]').click();
    await page.locator('[data-page="general"]').click();
    await page.locator('[data-page="about"]').click();
    await page.evaluate(()=>localStorage.setItem('uc.interface-prototype.v1','{broken'));
    await page.reload();
    assert.match(await page.locator('#storage-status').textContent(),/could not be loaded/);
    assert.deepEqual(errors,[]);
    console.log('PASS: six pages; 14 displays; A/B flip; persistence; inert action edits; escaped text; profiles/pages; corrupt draft recovery; 3 responsive sizes; no page errors.');
    console.log(output);
  } finally {if(browser) await browser.close(); await new Promise(r=>server.close(r));}
})().catch(e=>{console.error(e);process.exitCode=1;});
