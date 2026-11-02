# Source and dependency notices

This private helper reuses the user's AI Usage Monitor source from
`oldbugo/ulanzi-toolkit`, checked out at
`bb2825d4a68883656560a8c063706f4709f8f1d5`:

- `source/providers/claude.js`, `source/providers/codex.js`
- `source/history.js`, `source/claude-poll-broker.js`, `source/render.js`
- Provider regression tests, with imports changed for this directory.

Original locations are under
`plugins/ai-usage-monitor/com.ulanzi.aiusage.ulanziPlugin/plugin/` and `tests/`.
The Codex provider adds AbortSignal cleanup so deactivation can stop its own
dedicated account reader, plus read-only metadata checks before/after transactions
and a RAM-only identity side channel. The new engine gates original data before publication
and rendering. No Ulanzi SDK or `ws` code is copied; those are unnecessary here.
No runtime caches, auth files, account IDs, transcripts or personal profiles are
included. The spark/code-bracket artwork is original Toolkit vector art, not
official provider trademark artwork.

`@resvg/resvg-js` 2.6.2 and its platform binary package are pinned in
`package-lock.json`. They use MPL-2.0; the unchanged licence is retained at
`licenses/resvg-MPL-2.0.txt` and in the installed package. Source:
[resvg-js](https://github.com/thx/resvg-js/tree/v2.6.2). The bundled native package
and its notices must accompany the helper when packaging. Node runtime licensing
and packaging are owned by the app's resource build, outside this helper.

This task does not publish either repository or install any plugin.
