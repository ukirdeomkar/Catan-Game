// End-of-game replay keyboard stepping.
//
// Progressive enhancement only: the Prev/Next links work on their own without
// JS. Here we click the same link so htmx swaps just the replay stage instead
// of reloading the whole document. Loaded as an external file so the app's
// `script-src 'self'` CSP does not block it.
(function () {
  if (window.__catanReplayKeys) return;
  window.__catanReplayKeys = true;
  document.addEventListener('keydown', function (e) {
    if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight') return;
    if (e.altKey || e.ctrlKey || e.metaKey) return;
    var sel = e.key === 'ArrowLeft' ? '[data-replay-prev]' : '[data-replay-next]';
    var a = document.querySelector(sel);
    if (!a) return;
    e.preventDefault();
    a.click();
  });
})();
