// Shared session upkeep for the lobby and the game screen.
//
// The server sends a `gone` SSE event once our session token stops mapping to
// a seat — we left, or the host removed us. Navigate to whatever URL it
// carries (always the home page) so the tab does not sit on a dead room.
(function () {
  function hook(src) {
    if (!src || typeof src.addEventListener !== "function") return;
    src.addEventListener("gone", function (ev) {
      location.href = ev.data || "/";
    });
  }
  document.addEventListener("htmx:sseOpen", function (e) {
    hook(e.detail && e.detail.source);
  });
})();
