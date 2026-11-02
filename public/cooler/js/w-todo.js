// 4.0 W1 "To-do" item. Registered by morewidgets.js. A card with an optional title and 1-8 rows (tick box + text). The user types
// the lines and ticks them here in Properties (the screen has no touch); the engine only draws. Follows docs/v4/ui-style-guide.md
// section 4: changes save at once through api.set / api.edited (no Save button); labels short, sentence case.
const MAX_ITEMS = 8;

export default {
  type: 'todo', key: 'todo', name: 'To-do', icon: '▦', group: 'Decoration',
  props: () => ({
    title: null,
    items: [{ text: 'First thing', done: false }, { text: 'Second thing', done: false }, { text: 'Third thing', done: false }],
    hideDone: false, color: '#ffffff', accent: '#5ac8ff',
  }),
  /** Default box [w, h] (fractions of the 2400x1080 canvas). */
  size: p => {
    const items = Array.isArray(p.items) ? p.items : [];
    const rows = Math.max(2, (p.hideDone ? items.filter(i => !i.done) : items).length) + (p.title ? 1 : 0);
    return [0.26, Math.min(1, 0.06 * rows + 0.06)];
  },
  title: w => (w.props && typeof w.props.title === 'string' && w.props.title.trim()) ? w.props.title.trim() : 'To-do',
  /** Append the Properties controls to `host`; return true. `h`: { colour(key, text, dflt, resettable), restructure(fn), clamp, rateField() }. */
  panel(w, host, api, h) {
    const { el } = api;
    const p = w.props;
    const items = Array.isArray(p.items) ? p.items : (p.items = []);
    const title = el('input', { type: 'text', id: 'pTodoTitle', maxLength: 30, spellcheck: false, placeholder: 'No title', oninput: e => {
      p.title = e.target.value.trim() ? e.target.value : null; api.edited(); api.relabel();
    } });
    title.value = typeof p.title === 'string' ? p.title : '';
    const list = el('div', { class: 'meter-rows' });
    const draw = () => {
      list.textContent = '';
      items.forEach((it, i) => {
        const tick = el('input', { type: 'checkbox', 'aria-label': 'Done', title: 'Done', onchange: e => { it.done = e.target.checked; api.edited(); } });
        tick.checked = !!it.done;
        const txt = el('input', { type: 'text', maxLength: 60, spellcheck: false, placeholder: 'What to do', 'aria-label': 'Item text', oninput: e => { it.text = e.target.value; api.edited(); } });
        txt.value = typeof it.text === 'string' ? it.text : '';
        const move = d => { const j = i + d; if (j < 0 || j >= items.length) return; h.restructure(() => { [items[i], items[j]] = [items[j], items[i]]; }); };
        list.append(el('div', { class: 'meter-row' }, tick, txt,
          el('div', { class: 'inline-ctl' },
            el('button', { type: 'button', class: 'iconbtn', title: 'Move up', 'aria-label': 'Move up', text: '↑', disabled: i === 0, onclick: () => move(-1) }),
            el('button', { type: 'button', class: 'iconbtn', title: 'Move down', 'aria-label': 'Move down', text: '↓', disabled: i === items.length - 1, onclick: () => move(1) }),
            el('button', { type: 'button', class: 'iconbtn', title: 'Remove', 'aria-label': 'Remove this item', text: '✕', onclick: () => h.restructure(() => { items.splice(i, 1); }) }))));
      });
      if (!items.length) list.append(el('p', { class: 'muted small', text: 'No items yet.' }));
    };
    draw();
    const add = el('button', { type: 'button', class: 'btn secondary', text: 'Add an item', disabled: items.length >= MAX_ITEMS,
      onclick: () => h.restructure(() => { if (items.length < MAX_ITEMS) items.push({ text: '', done: false }); }) });
    host.append(
      api.h.field('Title', title, 'pTodoTitle'),
      el('p', { class: 'small muted', text: 'Items' }), list,
      el('div', { class: 'row' }, add),
      api.h.swRow('Hide done items', !!p.hideDone, v => h.restructure(() => { p.hideDone = v; })),
      h.colour('color', 'Colour', '#ffffff'),
      h.colour('accent', 'Tick colour', '#5ac8ff'));
    return true;
  },
};
