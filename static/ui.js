// UI controller for the colonist-style game screen.
//
//   * bottom sheets (build / dev / steal / discard / menu) that survive the
//     per-viewer SSE fragment swaps,
//   * the dice tumble animation,
//   * toast-style notifications derived from new log lines.
//
// The trade modal is managed by trade.js; this file leaves it alone.
(function () {
  var openSheet = null;

  function sheets() {
    return document.querySelectorAll(".sheet[data-sheet]");
  }
  function byName(name) {
    return document.querySelector('.sheet[data-sheet="' + name + '"]');
  }
  function show(name) {
    if (!byName(name)) return;
    openSheet = name;
    var list = sheets();
    for (var i = 0; i < list.length; i++) {
      list[i].hidden = list[i].getAttribute("data-sheet") !== name;
    }
  }
  function closeAll() {
    openSheet = null;
    var list = sheets();
    for (var i = 0; i < list.length; i++) list[i].hidden = true;
  }

  // Force-open a mandatory sheet (discard / steal) if present, otherwise keep
  // whatever the user had open, otherwise keep everything closed.
  function apply() {
    var auto = document.querySelector(".sheet[data-auto]");
    if (auto) {
      show(auto.getAttribute("data-sheet"));
      return;
    }
    if (openSheet && byName(openSheet)) {
      show(openSheet);
    } else {
      closeAll();
    }
  }

  document.addEventListener("click", function (e) {
    if (!e.target.closest) return;
    var opener = e.target.closest("[data-sheet-open]");
    if (opener) {
      show(opener.getAttribute("data-sheet-open"));
      e.preventDefault();
      return;
    }
    if (e.target.closest("[data-sheet-close]")) {
      closeAll();
      e.preventDefault();
    }
  });

  // Any action fired from inside a user sheet (choosing a build, playing a
  // card) closes it so the board / result is visible. Mandatory sheets stay.
  document.addEventListener("htmx:beforeRequest", function (e) {
    var el = e.target || (e.detail && e.detail.elt);
    if (el && el.closest && el.closest(".sheet") && !el.closest(".sheet[data-auto]")) {
      closeAll();
    }
  });

  document.addEventListener("htmx:afterSwap", apply);
  document.addEventListener("htmx:sseMessage", apply);

  // ---- Dice roll animation -------------------------------------------------
  var rollHoldUntil = 0;
  document.addEventListener("click", function (e) {
    if (e.target.closest && e.target.closest("[x-roll]")) {
      rollHoldUntil = Date.now() + 2200;
    }
  });
  function paintDice() {
    if (Date.now() >= rollHoldUntil) return;
    var remain = rollHoldUntil - Date.now();
    var wraps = document.querySelectorAll(".dice-wrap");
    for (var i = 0; i < wraps.length; i++) {
      (function (w) {
        w.classList.add("rolling");
        setTimeout(function () { w.classList.remove("rolling"); }, remain + 60);
      })(wraps[i]);
    }
  }

  // ---- Log-derived notifications ------------------------------------------
  var logInit = false;
  var lastLog = null;
  var OUTCOME =
    /complete a trade|declined|withdrawn|expired|moves the robber|steals a card|finds nothing|= 7\.|plays a Knight/i;

  function notify(text) {
    var box = document.getElementById("notif");
    if (!box) return;
    var d = document.createElement("div");
    d.className = "note";
    d.textContent = text;
    box.appendChild(d);
    setTimeout(function () {
      d.style.transition = "opacity .4s";
      d.style.opacity = "0";
      setTimeout(function () { if (d.parentNode) d.parentNode.removeChild(d); }, 400);
    }, 4000);
  }
  function checkLog() {
    var log = document.getElementById("log");
    if (!log) return;
    var first = log.firstElementChild;
    var text = first ? first.textContent.trim() : "";
    if (!logInit) { lastLog = text; logInit = true; return; }
    if (text && text !== lastLog) {
      lastLog = text;
      if (OUTCOME.test(text)) notify(text);
    }
  }

  document.addEventListener("htmx:afterSwap", function () { paintDice(); checkLog(); });
  document.addEventListener("htmx:sseMessage", function () { paintDice(); checkLog(); });

  apply();
})();
