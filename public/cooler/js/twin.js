// The editor's digital twin: the On screen preview shows the live picture as it is on the screen (clock ticking, music progress,
// sensors, effects moving), about 15 frames per second while the editor is on screen and looking, and nothing at all otherwise.
// The engine hands over the JPEG it sends (Live on) or renders it for the editor only (Live off, nothing sent to the screen); the
// page fetches it from the local media server, turns the portrait frame back to landscape on a canvas and shows it under the
// editable boxes. While no fresh frame arrives (live layer off, screen busy, preview not ready) the old still preview stays visible.
//
// createTwin(stage, { call, mediaBase }) puts a <canvas class="layer twin"> into `stage` and runs itself; it stops when the stage
// leaves the page. CSS: `.stage.twin-on > img.layer { visibility: hidden }` hides the still images while frames are fresh.
const FRESH_MS = 1500;   // a frame younger than this counts as "the twin is live"
const POLL_MS = 45;      // between requests (the engine publishes about 15 per second)
const W = 1200, H = 540; // canvas size (landscape), like the still preview

export function createTwin(stage, { call, mediaBase }) {
  const cv = document.createElement('canvas');
  cv.className = 'layer twin';
  cv.width = W; cv.height = H;
  cv.setAttribute('aria-hidden', 'true');
  cv.hidden = true;
  // under the corner overlay, guides and boxes (the first non-image child), over the still images
  const before = [...stage.children].find(c => c.tagName !== 'IMG') || null;
  stage.insertBefore(cv, before);
  const ctx = cv.getContext('2d', { alpha: false });
  const born = performance.now();
  let everConnected = false;
  let seq = 0, lastFrame = 0, watching = false, lastWatch = 0, stopped = false, busy = false;

  const visible = () => stage.isConnected && stage.offsetParent !== null && !document.hidden;
  const watch = on => { watching = on; lastWatch = performance.now(); return call('twin_watch', { on }).catch(() => {}); };

  async function pull() {
    const base = mediaBase();
    if (!base) return;
    const r = await fetch(`${base}/twin/frame.jpg?since=${seq}`, { cache: 'no-store' });
    if (r.status !== 200) return;
    const s = Number(r.headers.get('X-Twin-Seq')) || seq + 1;
    const bmp = await createImageBitmap(await r.blob());
    seq = s;
    // the frame is the landscape picture turned 90 degrees clockwise (what the screen takes): turn it back while drawing
    ctx.setTransform(0, -H / bmp.width, W / bmp.height, 0, 0, H);
    ctx.drawImage(bmp, 0, 0);
    bmp.close && bmp.close();
    lastFrame = performance.now();
  }

  async function tick() {
    if (stopped) return;
    if (stage.isConnected) everConnected = true;
    // the stage is built before it is put on the page: only a stage that left (or never arrived) ends the twin
    else if (everConnected || performance.now() - born > 3000) { stopped = true; if (watching) call('twin_watch', { on: false }).catch(() => {}); return; }
    const now = performance.now();
    try {
      if (visible()) {
        // (re)start the engine side now and then: it also restarts after Live was switched on or off underneath
        if (!watching || now - lastWatch > 5000 || (now - lastFrame > 2500 && now - lastWatch > 1500)) await watch(true);
        if (!busy) { busy = true; try { await pull(); } finally { busy = false; } }
      } else if (watching) {
        await watch(false);
      }
    } catch (e) { /* the next tick tries again */ }
    const fresh = performance.now() - lastFrame < FRESH_MS;
    cv.hidden = !fresh;
    stage.classList.toggle('twin-on', fresh);
    setTimeout(tick, document.hidden ? 500 : POLL_MS);
  }
  tick();

  return { stop() { stopped = true; stage.classList.remove('twin-on'); cv.remove(); if (watching) call('twin_watch', { on: false }).catch(() => {}); } };
}
