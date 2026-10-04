(function () {
  // When the host starts, the server emits a `started` SSE event carrying the
  // room URL. Navigate all lobby viewers into the game automatically.
  document.addEventListener("htmx:sseOpen", function (e) {
    var src = e.detail && e.detail.source;
    if (!src || typeof src.addEventListener !== "function") return;
    src.addEventListener("started", function (ev) {
      location.href = ev.data || location.pathname;
    });
  });
})();
