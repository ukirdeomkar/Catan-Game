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

  // htmx already fires `htmx:afterSwap` for every SSE fragment swap, so the
  // separate `htmx:sseMessage` listeners this file used to carry ran the same
  // work a second time for every fragment. One bump can swap several
  // fragments, so coalesce the whole batch into a single pass on the next
  // frame instead of scanning the document once per fragment.
  var bumpQueued = false;
  function onBump() {
    if (bumpQueued) return;
    bumpQueued = true;
    requestAnimationFrame(function () {
      bumpQueued = false;
      apply();
      syncDice();
      checkLog();
    });
  }

  document.addEventListener("htmx:afterSwap", onBump);

  // ---- Error toast clearing ------------------------------------------------
  // A successful action returns `HX-Trigger-After-Swap: catan:clear-toasts`.
  // htmx dispatches that event on <body> when the SSE swap detached the
  // element that started the request, so the clear still lands even though an
  // out-of-band `#toasts` swap would have had no target.
  document.addEventListener("catan:clear-toasts", function () {
    var box = document.getElementById("toasts");
    if (box) box.innerHTML = "";
  });

  // ---- Dice roll reveal ----------------------------------------------------
  // Every viewer sees the roll in the bottom-left dock. When an SSE bump brings
  // in a new dice pair, hold the tumble face for a moment before revealing the
  // result (CSS hides `.dice-real` while `.rolling`). Keying on the pair keeps
  // consecutive rolls with the same total animating. Bot rolls already tumble
  // in place and the roller is watching the dialog, so both are left alone.
  var DICE_REVEAL_MS = 900;
  var diceSeen = null;
  var dicePrimed = false;
  var diceRevealUntil = 0;
  var diceTimer = 0;

  function dockDiceWrap() {
    return document.querySelector("#turn .dock-dice .dice-wrap");
  }
  function rollDialogOpen() {
    var el = document.querySelector("#roll-root [data-roll-overlay], #turn [data-roll-overlay]");
    return !!(el && !el.hidden);
  }
  function holdDice(wrap) {
    window.clearTimeout(diceTimer);
    diceRevealUntil = Date.now() + DICE_REVEAL_MS;
    wrap.classList.add("rolling");
    diceTimer = window.setTimeout(function () {
      diceRevealUntil = 0;
      var w = dockDiceWrap();
      if (w) w.classList.remove("rolling");
    }, DICE_REVEAL_MS + 40);
  }
  function syncDice() {
    var wrap = dockDiceWrap();
    var pair = wrap ? wrap.getAttribute("title") || null : null;
    if (!dicePrimed) {
      // Seed from the loaded page so a refresh mid-game does not tumble.
      dicePrimed = true;
      diceSeen = pair;
      return;
    }
    if (!wrap) {
      diceSeen = null;
      return;
    }
    if (!pair || pair.indexOf("+") < 0) {
      // `—` (no roll yet) or the bot's "Rolling…" placeholder.
      diceSeen = pair;
      return;
    }
    if (pair === diceSeen) {
      // A re-render inside the reveal window can drop the class; put it back.
      if (Date.now() < diceRevealUntil) wrap.classList.add("rolling");
      return;
    }
    var prev = diceSeen;
    diceSeen = pair;
    diceRevealUntil = 0;
    if (prev === "Rolling…" || rollDialogOpen()) return;
    holdDice(wrap);
  }
  syncDice();

  // ---- Log-derived notifications ------------------------------------------
  var logInit = false;
  var lastLog = null;
  var OUTCOME =
    /complete a trade|declined|withdrawn|expired|moves the robber|steals a card|finds nothing|= 7\.|plays a Knight|gets \d+ free road|takes Longest Road|takes Largest Army|loses Longest Road|loses Largest Army/i;

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

  apply();
})();
