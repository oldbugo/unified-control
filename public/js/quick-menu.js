// Vibe Key quick menu window. Shows the core's menu view and sends input back (au05_menu). The core owns
// the state: knob turns, the knob press and the menu key arrive there directly; this page adds the
// keyboard (arrows, Enter, Backspace, Esc) and the mouse.
const T=window.__TAURI_INTERNALS__;
const send=(op,index)=>T.invoke('au05_menu',{input:index===undefined?{op}:{op,index}}).then(render).catch(()=>{});
const esc=s=>String(s??'').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
const $=id=>document.getElementById(id);

export function render(v){
  if(!v||v.open===false)return;
  const title=Array.isArray(v.title)?v.title:['Quick menu'];
  $('qm-title').textContent=title.join(' › ');
  $('qm-context').textContent=`Profile: ${v.profile||'none'} · Target app: ${v.target||'Claude'}`;
  const rows=Array.isArray(v.rows)?v.rows:[];
  $('qm-rows').innerHTML=rows.length?rows.map((r,i)=>{
    const detail=r.type==='submenu'?'›':r.current?'current':r.detail||'';
    return `<li class="qm-row" role="option" data-index="${i}" data-type="${esc(r.type)}" data-current="${r.current?'true':'false'}" aria-selected="${i===v.highlight}">
      <span class="qm-label">${r.type==='back'?'‹ ':''}${esc(r.label)}</span><span class="qm-detail">${esc(detail)}</span></li>`;
  }).join(''):'<li class="qm-empty">This menu is empty. Add entries on the Vibe Key page.</li>';
  $('qm-rows').querySelector('[aria-selected="true"]')?.scrollIntoView({block:'nearest'});
  const foot=$('qm-foot');
  foot.textContent=v.note||`Knob: move · press: choose · Esc: close · ${v.closeAfterS||8} s`;
  foot.classList.toggle('note',!!v.note);
}

if(T){
  T.invoke('plugin:event|listen',{event:'uc:au05-menu',target:{kind:'Any'},handler:T.transformCallback(e=>render(e.payload))}).catch(()=>{});
  send('state');
  document.addEventListener('keydown',e=>{
    const op={ArrowDown:'next',ArrowRight:'next',ArrowUp:'prev',ArrowLeft:'back',Enter:'choose',' ':'choose',Backspace:'back',Escape:'close'}[e.key];
    if(op){e.preventDefault();send(op);}
  });
  $('qm-rows').addEventListener('click',e=>{const row=e.target.closest('[data-index]');if(row)send('pick',Number(row.dataset.index));});
  // Clicking another window closes the menu.
  window.addEventListener('blur',()=>send('close'));
}
