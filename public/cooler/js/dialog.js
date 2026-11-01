// One accessible modal dialog used for every confirmation (no native confirm()/prompt()).
// openDialog({ title, body: Node|string, actions: [{ label, value, kind: 'primary'|'danger'|'secondary' }], dismissValue })
// resolves with the chosen action's value (dismissValue on Esc / backdrop). Focus is trapped and returned afterwards.
let counter = 0;

export function openDialog({ title, body, actions = [{ label: 'OK', value: true, kind: 'primary' }], dismissValue = null, wide = false, onOpen }) {
  return new Promise(resolve => {
    const prev = document.activeElement;
    const id = 'dlg' + ++counter;
    const back = document.createElement('div');
    back.className = 'backdrop';
    const box = document.createElement('div');
    box.className = 'dialog' + (wide ? ' wide' : '');
    box.setAttribute('role', 'dialog');
    box.setAttribute('aria-modal', 'true');
    box.setAttribute('aria-labelledby', id + 't');
    const h = document.createElement('h2');
    h.id = id + 't';
    h.textContent = title;
    const content = document.createElement('div');
    content.className = 'dialog-body';
    if (typeof body === 'string') { const p = document.createElement('p'); p.textContent = body; content.append(p); }
    else if (body) content.append(body);
    const row = document.createElement('div');
    row.className = 'dialog-actions';
    let done = false;
    const close = v => {
      if (done) return;
      done = true;
      document.removeEventListener('keydown', onKey, true);
      back.remove();
      if (prev && prev.focus && document.contains(prev)) prev.focus();
      resolve(v);
    };
    const buttons = actions.map(a => {
      const b = document.createElement('button');
      b.type = 'button';
      b.className = 'btn ' + (a.kind || 'secondary');
      b.textContent = a.label;
      b.onclick = () => close(a.value);
      row.append(b);
      return b;
    });
    box.append(h, content, row);
    back.append(box);
    back.addEventListener('mousedown', e => { if (e.target === back) close(dismissValue); });
    function onKey(e) {
      if (!document.contains(box)) return;
      if (e.key === 'Escape') { e.stopPropagation(); e.preventDefault(); if (!box.dataset.locked) close(dismissValue); return; }
      if (e.key === 'Tab') {
        const f = [...box.querySelectorAll('button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])')].filter(x => !x.disabled && x.offsetParent !== null);
        if (!f.length) return;
        const first = f[0], last = f[f.length - 1];
        if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last.focus(); }
        else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first.focus(); }
      }
    }
    document.addEventListener('keydown', onKey, true);
    document.body.append(back);
    const primary = buttons.find((b, i) => actions[i].kind === 'primary' || actions[i].kind === 'danger') || buttons[buttons.length - 1];
    // Destructive dialogs focus the safe choice first.
    const safe = actions.some(a => a.kind === 'danger') ? buttons.find((b, i) => actions[i].kind === 'secondary') : null;
    (content.querySelector('[autofocus]') || safe || primary || box).focus();
    if (onOpen) onOpen({ box, close, buttons, setLocked: v => { if (v) box.dataset.locked = '1'; else delete box.dataset.locked; buttons.forEach(b => (b.disabled = !!v)); } });
  });
}
