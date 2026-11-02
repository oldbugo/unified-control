// 4.0 live effects: the "Background" section of the theme settings (docs/v3/theme-format.md, "v4 effects").
// A theme's background is its picture (as before) or a generated effect the engine draws live. Every change is applied and saved
// at once through the editor's normal path (api.edited): there is no Save button.

/** Effects and the controls each one shows. Defaults mirror src-tauri/src/live/effects/mod.rs (the engine fills any missing value). */
export const EFFECTS = {
  aurora: { name: 'Aurora', hint: 'Rays of light drifting sideways under a starry sky.', speed: 1, density: 0.5, scale: 0.5, brightness: 0.85, colors: ['#2cf5a0', '#3a7bff', '#b24bff'], controls: { density: 'Rays', scale: 'Glow height' } },
  plasma: { name: 'Plasma', hint: 'Warm bands of colour sliding past each other.', speed: 1, density: 0.5, scale: 0.5, brightness: 0.85, colors: ['#1b0f4a', '#c13a8f', '#ff9a3c', '#3a1d7a'], controls: { density: 'Bands', scale: 'Size' } },
  drift: { name: 'Drift', hint: 'Soft glowing specks that drift and fade.', speed: 1, density: 0.5, scale: 0.5, brightness: 0.9, colors: ['#7fd4ff', '#ffffff', '#a58bff'], controls: { density: 'Amount', scale: 'Size' } },
  warp: { name: 'Warp', hint: 'Stars streaming sideways past you.', speed: 1, density: 0.5, scale: 0.5, brightness: 0.9, colors: ['#9fd0ff', '#ffffff', '#ffb36b'], controls: { density: 'Stars', scale: 'Streak length' }, names: ['Star', 'Star', 'Star'] },
  waves: { name: 'Waves', hint: 'Layers of sea or hills rolling sideways at sunset.', speed: 1, density: 0.5, scale: 0.5, brightness: 0.95, colors: ['#10163a', '#ff8a5c', '#2b4a8f', '#0b1d3d'], controls: { density: 'Wave height', scale: 'Wavelength' }, names: ['Sky', 'Horizon', 'Far water', 'Near water'] },
  lava: { name: 'Lava', hint: 'Glowing blobs swaying sideways, merging and parting.', speed: 1, density: 0.5, scale: 0.5, brightness: 0.95, colors: ['#1a0626', '#ff4d6d', '#ffb36b', '#fff1c9'], controls: { density: 'Blobs', scale: 'Size' }, names: ['Background', 'Rim', 'Body', 'Core'] },
  retro: { name: 'Retro', hint: 'A neon sun over a grid whose lines slide sideways.', speed: 1, density: 0.5, scale: 0.5, brightness: 0.9, colors: ['#14052e', '#ff2e93', '#ffd23f', '#2de2e6'], controls: { density: 'Grid lines', scale: 'Sun size' }, names: ['Sky top', 'Horizon', 'Sun', 'Grid'] },
  pulse: { name: 'Pulse (sound)', hint: 'Columns of light that glow with the music playing on this PC. Listens only while it is the live background; the stored copy is a demo loop.', speed: 1, density: 0.5, scale: 0.5, brightness: 0.9, colors: ['#ff3d81', '#7a5cff', '#2de2e6'], controls: { density: 'Halo', scale: 'Glow height' }, names: ['Colour', 'Colour', 'Colour'] },
  clouds: { name: 'Clouds', hint: 'Slow clouds or smoke in a four-colour ramp.', speed: 0.5, density: 0.5, scale: 0.5, brightness: 0.9, colors: ['#0b1030', '#3b2f8f', '#d3689b', '#ffd9a8'], controls: { density: 'Cover', scale: 'Size' } },
};

const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));

/** The effect of a theme, or null when its background is the picture. */
export const effectOf = t => (t.background && t.background.kind === 'effect' && EFFECTS[t.background.effect] ? t.background : null);

/** The live speed of an effect background (its own setting or the effect's default). */
export const effectSpeed = b => (Number.isFinite(b.speed) ? b.speed : (EFFECTS[b.effect] || EFFECTS.aurora).speed);

/** Short text for My themes cards and lists. */
export const effectName = t => { const b = effectOf(t); return b ? EFFECTS[b.effect].name : null; };

/**
 * The Background section as an array of elements for the theme settings panel.
 * api: el, h { field, select2, range }, theme (the working copy), edited() (save + preview), repaint() (rebuild the panel).
 */
export function backgroundSection(api) {
  const { el, theme: t } = api;
  const { field, range } = api.h;
  const cur = effectOf(t);
  const kids = [el('h3', { text: 'Background' })];
  const seg = el('div', { class: 'seg', role: 'group', 'aria-label': 'Background' },
    ...[['picture', 'Picture'], ['effect', 'Live effect']].map(([k, label]) => {
      const on = (k === 'effect') === !!cur;
      return el('button', { type: 'button', text: label, 'aria-pressed': String(on), id: 'bg-' + k, onclick: () => {
        if (on) return;
        if (k === 'picture') delete t.background;
        else t.background = { kind: 'effect', effect: (t.lastEffect && EFFECTS[t.lastEffect]) ? t.lastEffect : 'aurora' };
        api.edited(); api.repaint();
      } });
    }));
  kids.push(seg);
  if (!cur) {
    kids.push(el('p', { class: 'muted small', text: 'The theme’s picture. Choose Live effect for a background drawn live that never repeats on screen.' }));
    return kids;
  }
  const def = EFFECTS[cur.effect];
  const val = (k) => (Number.isFinite(cur[k]) ? cur[k] : def[k]);
  const picker = el('select', { id: 'bgEffect', onchange: e => { const next = e.target.value; t.background = { kind: 'effect', effect: next }; api.edited(); api.repaint(); } },
    Object.entries(EFFECTS).map(([k, v]) => el('option', { value: k, text: v.name })));
  picker.value = cur.effect;
  kids.push(field('Effect', picker, 'bgEffect'), el('p', { class: 'muted small', text: def.hint }));
  const set = (k, v) => { cur[k] = v; api.edited(); };
  const pct = v => Math.round(v * 100) + '%';
  kids.push(field('Speed', range('bgSpeed', clamp(val('speed'), 0.1, 3), 0.1, 3, 0.05, v => set('speed', Math.round(v * 100) / 100), v => v.toFixed(2) + '×'), 'bgSpeed'));
  if (def.controls.density) kids.push(field(def.controls.density, range('bgDensity', clamp(val('density'), 0, 1), 0, 1, 0.02, v => set('density', Math.round(v * 100) / 100), pct), 'bgDensity'));
  if (def.controls.scale) kids.push(field(def.controls.scale, range('bgScale', clamp(val('scale'), 0, 1), 0, 1, 0.02, v => set('scale', Math.round(v * 100) / 100), pct), 'bgScale'));
  kids.push(field('Brightness', range('bgBright', clamp(val('brightness'), 0.1, 1.5), 0.1, 1.5, 0.05, v => set('brightness', Math.round(v * 100) / 100), v => Math.round(v * 100) + '%'), 'bgBright'));
  // colours: one swatch per ramp stop
  const colours = (Array.isArray(cur.colors) && cur.colors.length >= 2 ? cur.colors : def.colors).slice(0, 4);
  const sw = el('div', { class: 'inline-ctl', role: 'group', 'aria-label': 'Colours' }, colours.map((c, i) => {
    const nm = (def.names && def.names[i]) || 'Colour ' + (i + 1);
    const inp = el('input', { type: 'color', 'aria-label': nm, title: nm, id: 'bgCol' + i, oninput: e => { const next = colours.slice(); next[i] = e.target.value; colours[i] = e.target.value; cur.colors = next; api.edited(); } });
    inp.value = c;
    return inp;
  }));
  kids.push(field('Colours', sw, 'bgCol0'));
  const secs = 10 / val('speed');
  kids.push(el('p', { class: 'muted small', text: `One loop lasts ${secs < 10 ? secs.toFixed(1) : Math.round(secs)} s. Embedded keeps a recording of one loop, so changing the effect means updating Embedded.` }),
    el('div', { class: 'row' }, el('button', { type: 'button', class: 'btn ghost small-btn', text: 'Reset', title: 'Back to this effect’s own settings', onclick: () => { t.background = { kind: 'effect', effect: cur.effect }; api.edited(); api.repaint(); } })));
  return kids;
}
