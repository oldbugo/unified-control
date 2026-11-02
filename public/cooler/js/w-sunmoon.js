// 4.0 W1 "Sun and moon" item: sunrise, sunset, day length and the moon phase. Registered by morewidgets.js.
// Follows docs/v4/ui-style-guide.md section 4: helpers from `api.h`, changes save at once through api.set (no Save button).
export default {
  type: 'sunmoon', key: 'sunmoon', name: 'Sun and moon', icon: '▦', group: 'Info',
  props: () => ({ show: ['sun', 'moon'], format: '24h', color: '#ffd27a' }),
  /** Default box [w, h] (fractions of the 2400x1080 canvas). */
  size: p => [0.34, 0.26],
  title: w => 'Sun and moon',
  /** Append the Properties controls to `host`; return true. `h`: { colour(key, text, dflt, resettable), restructure(fn), clamp, rateField() }. */
  panel(w, host, api, h) {
    const { el } = api;
    const { field, select2, swRow } = api.h;
    const p = w.props;
    const shown = () => (Array.isArray(p.show) && p.show.length ? p.show : ['sun', 'moon']);
    const toggle = (what, on, row) => {
      const next = ['sun', 'moon'].filter(k => (k === what ? on : shown().includes(k)));
      if (!next.length) {  // at least one stays on: put the switch back
        const box = row && row.querySelector('input');
        if (box) box.checked = true;
        return;
      }
      api.set('show', next);
    };
    let sunRow = null, moonRow = null;
    sunRow = swRow('Sun', shown().includes('sun'), v => toggle('sun', v, sunRow), 'Sunrise, sunset and day length.');
    moonRow = swRow('Moon', shown().includes('moon'), v => toggle('moon', v, moonRow), 'Phase picture, name and how much is lit.');
    host.append(sunRow, moonRow);
    host.append(field('Format', select2('pFormat', p.format === '12h' ? '12h' : '24h', [['24h', '24-hour'], ['12h', '12-hour']], v => api.set('format', v)), 'pFormat'));
    host.append(h.colour('color', 'Colour', '#ffd27a'));
    const place = api.S.app && api.S.app.weather && api.S.app.weather.place;
    if (!place) {
      host.append(el('p', { class: 'muted small' }, el('span', { text: 'Sun times need your city. ' }),
        el('button', { type: 'button', class: 'linkbtn', text: 'Set your city', onclick: () => api.openSettings('setWeather') })));
    } else {
      host.append(el('p', { class: 'muted small', text: 'Sun times for ' + place.name + '.' }));
    }
    return true;
  },
};
