// Small line icons for the Vibe Key page (24 x 24, stroke = currentColor). Simple glyphs only: no app logos.
const svg=d=>`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${d}</svg>`;
const I={
  mic:svg('<rect x="9" y="3" width="6" height="11" rx="3"/><path d="M5 11a7 7 0 0 0 14 0M12 18v3"/>'),
  micOff:svg('<path d="M9 9v2a3 3 0 0 0 5.1 2.1M15 10V6a3 3 0 0 0-5.7-1.3M5 11a7 7 0 0 0 11.5 5.4M19 11a7 7 0 0 1-.6 2.8M12 18v3M4 4l16 16"/>'),
  send:svg('<path d="M4 12 20 4l-6 16-3-7z"/><path d="m11 13 9-9"/>'),
  plus:svg('<path d="M12 5v14M5 12h14"/>'),
  next:svg('<path d="M5 12h12M13 7l5 5-5 5"/>'),
  prev:svg('<path d="M19 12H7M11 7l-5 5 5 5"/>'),
  chip:svg('<rect x="7" y="7" width="10" height="10" rx="2"/><path d="M10 3v4M14 3v4M10 17v4M14 17v4M3 10h4M3 14h4M17 10h4M17 14h4"/>'),
  shield:svg('<path d="M12 3 5 6v6c0 4 3 7 7 9 4-2 7-5 7-9V6z"/>'),
  chat:svg('<path d="M5 5h14v10H9l-4 4z"/>'),
  terminal:svg('<rect x="3" y="5" width="18" height="14" rx="2"/><path d="m7 10 3 2-3 2M12 15h5"/>'),
  diff:svg('<path d="M8 4v8M4 8h8M14 16h6M6 20h12"/>'),
  keyboard:svg('<rect x="3" y="6" width="18" height="12" rx="2"/><path d="M7 10h1M11 10h1M15 10h2M7 14h10"/>'),
  launch:svg('<path d="M14 4h6v6M20 4l-9 9M18 14v5H5V6h5"/>'),
  stop:svg('<rect x="6" y="6" width="12" height="12" rx="2"/>'),
  sidebar:svg('<rect x="3" y="5" width="18" height="14" rx="2"/><path d="M9 5v14"/>'),
  cursor:svg('<path d="M9 4h6M9 20h6M12 4v16"/>'),
  window:svg('<rect x="3" y="5" width="18" height="14" rx="2"/><path d="M3 9h18"/>'),
  enter:svg('<path d="M20 5v7H6M10 8l-4 4 4 4"/>'),
  play:svg('<path d="M7 5v14l6-7zM15 5v14M19 5v14"/>'),
  nextTrack:svg('<path d="m5 6 8 6-8 6zM17 6v12"/>'),
  prevTrack:svg('<path d="m19 6-8 6 8 6zM7 6v12"/>'),
  volUp:svg('<path d="M4 10v4h4l5 4V6L8 10z"/><path d="M16 9a4 4 0 0 1 0 6M19 6a8 8 0 0 1 0 12"/>'),
  volDown:svg('<path d="M4 10v4h4l5 4V6L8 10z"/><path d="M16 9a4 4 0 0 1 0 6"/>'),
  mute:svg('<path d="M4 10v4h4l5 4V6L8 10z"/><path d="m16 9 5 6M21 9l-5 6"/>'),
  desktop:svg('<rect x="3" y="4" width="18" height="12" rx="2"/><path d="M8 20h8M12 16v4"/>'),
  grid:svg('<rect x="4" y="4" width="7" height="7" rx="1.5"/><rect x="13" y="4" width="7" height="7" rx="1.5"/><rect x="4" y="13" width="7" height="7" rx="1.5"/><rect x="13" y="13" width="7" height="7" rx="1.5"/>'),
  crop:svg('<path d="M6 3v15h15M3 6h15v15"/>'),
  menu:svg('<rect x="4" y="4" width="16" height="16" rx="3"/><path d="M8 9h8M8 12h8M8 15h5"/>'),
  command:svg('<path d="M9 6a3 3 0 1 0-3 3h12a3 3 0 1 0-3-3v12a3 3 0 1 0 3-3H6a3 3 0 1 0 3 3z"/>'),
  none:svg('<circle cx="12" cy="12" r="8"/><path d="m6.5 17.5 11-11"/>'),
  target:svg('<circle cx="12" cy="12" r="8"/><circle cx="12" cy="12" r="3"/>'),
  scroll:svg('<rect x="8" y="3" width="8" height="14" rx="4"/><path d="M12 7v3M9 20l3 2 3-2"/>'),
  profile:svg('<circle cx="12" cy="8" r="4"/><path d="M4 20c1.5-4 4.5-6 8-6s6.5 2 8 6"/>'),
  folder:svg('<path d="M3 7h6l2 2h10v10H3z"/>'),
  sessions:svg('<path d="M5 5h14v10H9l-4 4z"/><path d="M9 9h6M9 12h4"/>'),
  edit:svg('<path d="M4 20h4L19 9l-4-4L4 16z"/>'),
  trash:svg('<path d="M5 7h14M10 7V4h4v3M7 7l1 13h8l1-13"/>'),
  grip:svg('<path d="M9 6h.01M15 6h.01M9 12h.01M15 12h.01M9 18h.01M15 18h.01" stroke-width="3"/>'),
  up:svg('<path d="m6 15 6-6 6 6"/>'),
  down:svg('<path d="m6 9 6 6 6-6"/>'),
  close:svg('<path d="M6 6l12 12M18 6 6 18"/>'),
  read:svg('<path d="M20 12a8 8 0 1 1-2.3-5.7M20 4v5h-5"/>'),
  power:svg('<path d="M12 3v8M7 6a7 7 0 1 0 10 0"/>'),
  light:svg('<path d="M9 18h6M10 21h4M12 3a6 6 0 0 0-4 10.5c.6.6 1 1.5 1 2.5h6c0-1 .4-1.9 1-2.5A6 6 0 0 0 12 3z"/>'),
  sliders:svg('<path d="M4 7h10M18 7h2M4 17h4M12 17h8"/><circle cx="16" cy="7" r="2"/><circle cx="10" cy="17" r="2"/>'),
  dot:svg('<circle cx="12" cy="12" r="3"/>'),
};
export const ICON=I;
// The icon for a catalogue action (or a UC action kind).
const BY_ID={
  claudeDictation:'mic',chatgptDictation:'mic',targetDictation:'mic',claudeSend:'send',chatgptSend:'send',targetSend:'send',
  claudeNewSession:'plus',chatgptNewChat:'plus',targetNew:'plus',claudeNextSession:'next',claudePrevSession:'prev',
  claudeModelMenu:'chip',claudePermissionMenu:'shield',claudeSideChat:'chat',claudeTerminal:'terminal',claudeDiff:'diff',
  claudeShortcuts:'keyboard',chatgptShortcuts:'keyboard',claudeQuickOpen:'launch',claudeStop:'stop',chatgptSidebar:'sidebar',
  chatgptFocusInput:'cursor',chatgptCompanion:'window',enter:'enter',playPause:'play',nextTrack:'nextTrack',previousTrack:'prevTrack',
  volumeUp:'volUp',volumeDown:'volDown',mute:'mute',micMute:'micOff',showDesktop:'desktop',taskView:'grid',screenshot:'crop',
  quickMenu:'menu',hotkey:'command',none:'none',scroll:'scroll',volume:'volUp',
};
export const actionIcon=id=>I[BY_ID[id]]||I.dot;
// Short card names (two or three words); others use the catalogue label.
export const SHORT={
  claudeQuickOpen:'Open anywhere',claudeStop:'Stop',claudePermissionMenu:'Permission mode',claudeShortcuts:'Shortcut list',
  chatgptShortcuts:'Shortcut list',claudeDiff:'Diff pane',claudeTerminal:'Terminal',claudeNextSession:'Next session',
  claudePrevSession:'Previous session',claudeModelMenu:'Model menu',claudeSideChat:'Side chat',chatgptFocusInput:'Focus input',
  chatgptCompanion:'Companion',chatgptSidebar:'Sidebar',enter:'Enter',screenshot:'Screenshot',micMute:'Mute mic',
  showDesktop:'Show desktop',taskView:'Task view',targetNew:'New chat',claudeSend:'Send',chatgptSend:'Send',targetSend:'Send',
  claudeDictation:'Dictation',chatgptDictation:'Dictation',targetDictation:'Dictation',claudeNewSession:'New session',
  chatgptNewChat:'New chat',mute:'Mute sound',
};
// App sections: a simple initial or glyph and a colour token (no logos).
export const SECTIONS=[
  {group:'Claude',title:'Claude',mark:'C',hue:'var(--hue-claude)'},
  {group:'ChatGPT',title:'ChatGPT',mark:'G',hue:'var(--hue-chatgpt)'},
  {group:'Target app',title:'Target app',glyph:'target',hue:'var(--accent)'},
  {group:'Windows / media',title:'Windows & media',glyph:'desktop',hue:'var(--hue-system)'},
];
