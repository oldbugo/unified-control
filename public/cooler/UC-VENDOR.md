# Embedded CoolerScreen interface

Source: private `oldbugo/coolerscreen`, commit `b68d2e8b98f544ddd48a0a535109f3ee6b591ec7`.
This is the existing `public/` interface, embedded without UI redesign.

UC changes: load `bridge.js` before modules; omit the separate window titlebar
import; hide the separate app restart setting; transfer files in bounded 256 KiB
chunks (up to 2 GiB) with abort cleanup; await imports, flush saved edits and end
preview watches before leaving. Section navigation saves real edits/imports but
dispatches transient preview/watch cleanup without waiting for hardware replies;
disable, reset and Quit retain awaited cleanup. A still-loading bridge cannot
hold navigation before embedded controls can submit edits. View removal stops
the embedded page's polling and animation loops; the worker remains independent.
The parent allowlists commands/events in `../js/cooler-panel.js`; this frame's
bridge never forwards arbitrary native commands. Engine ownership stays in the
separate CoolerScreen repository.
