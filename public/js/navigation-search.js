// Search filters navigation only. Activation uses the existing guarded click path.
const ALIASES={home:'overview devices',cooler:'lovingcool sc70 themes library live',dock:'ulanzi d200h stream keys profiles ai usage allowance',dial:'ulanzi dial',general:'settings preferences startup privacy diagnostics mcp agent',about:'version build information'};
export function installNavigationSearch(nav){
  const input=nav?.querySelector('#page-search'),status=nav?.querySelector('.nav-search-status'),toggle=nav?.querySelector('.nav-search-toggle');
  if(!input||!status||!toggle)return;
  const buttons=[...nav.querySelectorAll('.nav-item[data-page]')];
  const heading=nav.querySelector('.nav-heading'),spacer=nav.querySelector('.nav-spacer');
  function apply(){
    const query=input.value.trim().toLowerCase(),terms=query?query.split(/\s+/):[];
    for(const button of buttons){
      const id=button.dataset.page,aliases=Object.hasOwn(ALIASES,id)?ALIASES[id]:'';
      const text=`${button.getAttribute('aria-label')||button.textContent} ${id} ${aliases}`.toLowerCase();
      button.hidden=!terms.every(term=>text.includes(term));
    }
    const visible=buttons.filter(button=>!button.hidden);
    if(heading)heading.hidden=!visible.some(button=>['cooler','dock','dial'].includes(button.dataset.page));
    if(spacer)spacer.hidden=!!query;
    status.hidden=!query;
    status.textContent=query?(visible.length?`${visible.length} matching ${visible.length===1?'page':'pages'}`:'No matching pages'):'';
    return visible;
  }
  input.addEventListener('input',apply);
  input.addEventListener('keydown',event=>{
    if(event.key==='Enter'&&input.value.trim()){
      event.preventDefault();const first=apply()[0];if(first){first.focus();first.click();}
    }
  });
  nav.addEventListener('keydown',event=>{
    if(event.key!=='Escape')return;
    if(input.value){event.preventDefault();input.value='';apply();input.focus();}
    else if(nav.classList.contains('search-open')){event.preventDefault();nav.classList.remove('search-open');toggle.setAttribute('aria-expanded','false');toggle.focus();}
  });
  toggle.addEventListener('click',()=>{
    const open=nav.classList.toggle('search-open');toggle.setAttribute('aria-expanded',String(open));
    if(open)input.focus();else toggle.focus();
  });
  apply();
}
