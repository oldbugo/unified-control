// Settings drawer: a row of section links under the title (docs/v4/ui-audit.md H6). Same sections, same order;
// a link scrolls its section to the top of the drawer and the link of the section in view is marked.
export function initSettingsNav() {
  const body = document.querySelector('#drawer .drawer-body');
  const head = document.querySelector('#drawer .drawer-head');
  if (!body || !head) return;
  const nav = document.createElement('nav');
  nav.className = 'drawer-index';
  nav.setAttribute('aria-label', 'Settings sections');
  const links = [];
  for (const sec of body.querySelectorAll(':scope > section')) {
    if(sec.hidden)continue;
    const h = sec.querySelector('h3');
    if (!h) continue;
    const b = document.createElement('button');
    b.type = 'button';
    b.className = 'chip-link';
    b.textContent = h.textContent.trim();
    b.onclick = () => { sec.scrollIntoView({ block: 'start', behavior: 'smooth' }); };
    nav.append(b);
    links.push([sec, b]);
  }
  head.after(nav);
  const mark = () => {
    const top = body.getBoundingClientRect().top + 8;
    let cur = links[0];
    for (const l of links) if (l[0].getBoundingClientRect().top <= top) cur = l;
    for (const l of links) l[1].classList.toggle('on', l === cur);
  };
  body.addEventListener('scroll', mark, { passive: true });
  new MutationObserver(mark).observe(document.getElementById('drawerBack'), { attributes: true, attributeFilter: ['hidden'] });
  mark();
}
