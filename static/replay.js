// End-of-game replay stepping.
//
// Progressive enhancement only: the Prev/Next links work on their own without
// JS. With htmx, a step swaps only `#replay-board` (the same board fragment the
// live game updates over SSE) plus a few out-of-band chrome pieces, so stepping
// looks like a live move instead of a page reload.
//
// URL state is pushed manually instead of with `hx-push-url`: htmx's own
// history restore replaces a whole-body snapshot on Back/Forward, which
// re-renders the page. Here Back/Forward re-fetch just the board frame too.
(function () {
  if (window.__catanReplayKeys) return;
  window.__catanReplayKeys = true;

  var pendingUrl = null;

  function stepLink(el) {
    return el && el.closest && el.closest('[data-replay-prev],[data-replay-next],[data-replay-step]');
  }

  function syncList() {
    var pos = document.getElementById('replay-pos');
    var list = document.getElementById('replay-list');
    if (!pos || !list) return;
    var n = pos.getAttribute('data-n');
    var items = list.querySelectorAll('[data-move]');
    for (var i = 0; i < items.length; i++) {
      items[i].classList.toggle('on', items[i].getAttribute('data-move') === n);
    }
  }

  function loadFrame(url) {
    if (!window.htmx) return;
    htmx.ajax('GET', url, { target: '#replay-board', swap: 'innerHTML' });
  }

  // Capture the destination before htmx starts the request.
  document.addEventListener('click', function (e) {
    var a = stepLink(e.target);
    if (!a) return;
    var href = a.getAttribute('href');
    pendingUrl = href && href !== location.pathname + location.search ? href : null;
  }, true);

  document.addEventListener('htmx:afterSwap', function (e) {
    syncList();
    var t = e.detail && e.detail.target;
    if (pendingUrl && t && t.id === 'replay-board') {
      history.pushState(null, '', pendingUrl);
      pendingUrl = null;
    }
  });
  document.addEventListener('htmx:oobAfterSwap', syncList);
  document.addEventListener('htmx:afterSettle', syncList);
  document.addEventListener('htmx:responseError', function () { pendingUrl = null; });
  syncList();

  window.addEventListener('popstate', function () {
    pendingUrl = null;
    loadFrame(location.pathname + location.search);
  });

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
