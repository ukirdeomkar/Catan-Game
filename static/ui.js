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
      reconcileAutoRoll();
      guardEndTurn();
      checkLog();
    });
  }

  document.addEventListener("htmx:afterSwap", onBump);

  // ---- Error toast clearing ------------------------------------------------
  // A successful action returns `HX-Trigger-After-Swap: hexports:clear-toasts`.
  // htmx dispatches that event on <body> when the SSE swap detached the
  // element that started the request, so the clear still lands even though an
  // out-of-band `#toasts` swap would have had no target.
  document.addEventListener("hexports:clear-toasts", function () {
    var box = document.getElementById("toasts");
    if (box) box.innerHTML = "";
  });

  // ---- Dice roll reveal ----------------------------------------------------
  // Every viewer sees the roll in the bottom-left dock. While a turn has not
  // produced a result yet the dock tumbles in place (server-rendered
  // `.rolling`), then when the pair arrives it holds the tumble for a moment
  // before revealing the result (CSS hides `.dice-real` while `.rolling`).
  // Keying on the pair keeps consecutive rolls with the same total animating.
  // There is one path: the roller, a bot and an observer all get this.
  var DICE_REVEAL_MS = 900;
  var EXIT_SETTLE_MS = 850;
  var diceSeen = null;
  var dicePrimed = false;
  var diceRevealUntil = 0;
  var diceTimer = 0;
  var settleUntil = 0;

  function dockDiceWrap() {
    return document.querySelector("#turn .dock-dice .dice-wrap");
  }
  function dockDiceResult() {
    return document.querySelector("#turn .dock-dice .dice-real");
  }
  function holdDice(wrap) {
    window.clearTimeout(diceTimer);
    // Reduced motion: skip the tumble hold and show the landed pair at once.
    if (window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
      diceRevealUntil = 0;
      wrap.classList.remove("rolling");
      return;
    }
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
      // `—` (no roll yet) or the in-flight "Rolling…" placeholder.
      diceSeen = pair;
      return;
    }
    if (pair === diceSeen) {
      // A re-render inside the reveal window can drop the class; put it back.
      if (Date.now() < diceRevealUntil) wrap.classList.add("rolling");
      return;
    }
    diceSeen = pair;
    diceRevealUntil = 0;
    // The result landed: stop the auto-roll timeout and give End Turn a short
    // settle window so an already-in-flight tap near the dock cannot skip it.
    clearAutoRollTimer();
    settleUntil = Date.now() + EXIT_SETTLE_MS;
    guardEndTurn();
    holdDice(wrap);
  }

  // ---- Automatic roll ------------------------------------------------------
  // The dock renders the tumbling dice plus the one-shot turn marker. Send the
  // existing roll POST once per turn with no tap. The turn key is the guard: an
  // SSE bump, an htmx swap or a reconnect that re-renders the same turn can
  // never send a second roll. On failure or a short timeout, reveal the
  // non-modal retry button; the turn is never skipped.
  var ROLL_TIMEOUT_MS = 4000;
  var autoRollActiveKey = null;
  var autoRollTimer = 0;
  // The turn key is also remembered in sessionStorage so a full page reload in
  // the same tab cannot re-send the roll for a turn this tab already fired.
  // (SSE reconnects never reload the script, so the in-memory value is enough
  // there; this covers an explicit refresh during the in-flight window.)
  function autoRollStoreKey() {
    return "hexports-auto-roll:" + window.location.pathname;
  }
  function readAutoRollTurn() {
    try {
      return window.sessionStorage.getItem(autoRollStoreKey());
    } catch (e) {
      return null;
    }
  }
  function rememberAutoRollTurn(key) {
    try {
      window.sessionStorage.setItem(autoRollStoreKey(), key);
    } catch (e) {}
  }
  var autoRollTurn = readAutoRollTurn();

  function autoRollMarker() {
    return document.querySelector("#turn .dock-dice [data-auto-roll]");
  }
  function retryButton() {
    return document.querySelector("#turn [data-roll-retry]");
  }
  function clearAutoRollTimer() {
    window.clearTimeout(autoRollTimer);
    autoRollTimer = 0;
    autoRollActiveKey = null;
  }
  function armAutoRollTimeout(key) {
    clearAutoRollTimer();
    autoRollActiveKey = key;
    autoRollTimer = window.setTimeout(function () {
      autoRollTimer = 0;
      if (autoRollActiveKey !== key) return;
      autoRollActiveKey = null;
      if (dockDiceResult()) return;
      showRetry();
    }, ROLL_TIMEOUT_MS);
  }
  // Non-modal fallback: stop the tumble and offer the dice button again. Called
  // on a network error, an htmx error response, or when no dice arrive in time.
  function showRetry() {
    var wrap = autoRollMarker();
    if (wrap && !dockDiceResult()) {
      wrap.classList.remove("rolling");
      wrap.hidden = true;
    }
    var btn = retryButton();
    if (btn) {
      btn.hidden = false;
      btn.disabled = false;
    }
  }
  function reconcileAutoRoll() {
    var marker = autoRollMarker();
    if (!marker) {
      // The result landed (or the turn moved on): nothing left to send.
      if (dockDiceResult()) clearAutoRollTimer();
      return;
    }
    var key = marker.getAttribute("data-roll-turn");
    if (key === null) return;
    if (key === autoRollTurn) {
      // Already sent for this turn. Keep the retry affordance if the request
      // is no longer in flight and no result has arrived.
      if (!autoRollActiveKey) showRetry();
      return;
    }
    var btn = retryButton();
    if (!btn || btn.disabled) return;
    var url = btn.getAttribute("hx-post");
    if (!url) return;
    autoRollTurn = key;
    rememberAutoRollTurn(key);
    // Send the same roll POST the retry button carries. Going through
    // `htmx.ajax` avoids relying on a synthetic click on a hidden button,
    // which can be dropped while the swap that revealed it is still settling.
    armAutoRollTimeout(key);
    try {
      var ajax = htmx.ajax("POST", url, { values: { action: "roll" }, swap: "none", source: btn });
      // The htmx events below own the failure UI; swallow the promise
      // rejection so a network failure does not also log an unhandled error.
      if (ajax && ajax.catch) ajax.catch(function () {});
    } catch (e) {
      clearAutoRollTimer();
      showRetry();
    }
  }
  document.addEventListener("htmx:beforeRequest", function (e) {
    var el = e.target || (e.detail && e.detail.elt);
    if (!el || !el.hasAttribute || !el.hasAttribute("data-roll-retry")) return;
    var marker = autoRollMarker();
    armAutoRollTimeout(marker ? marker.getAttribute("data-roll-turn") : autoRollTurn);
  });
  function rollRequestFailed(e) {
    var el = e.target || (e.detail && e.detail.elt);
    if (!el || !el.hasAttribute || !el.hasAttribute("data-roll-retry")) return;
    clearAutoRollTimer();
    showRetry();
  }
  document.addEventListener("htmx:afterRequest", function (e) {
    if (e.detail && e.detail.successful === false) rollRequestFailed(e);
  });
  document.addEventListener("htmx:sendError", rollRequestFailed);
  document.addEventListener("htmx:responseError", rollRequestFailed);

  // End Turn must not become tappable under a finger that was reaching for the
  // dock while the result settles.
  function guardEndTurn() {
    var btn = document.querySelector("#turn [data-end-turn]");
    if (!btn || Date.now() >= settleUntil) return;
    btn.disabled = true;
    btn.setAttribute("aria-disabled", "true");
    var until = settleUntil;
    window.setTimeout(function () {
      if (Date.now() < until) return;
      var b = document.querySelector("#turn [data-end-turn]");
      if (!b) return;
      b.disabled = false;
      b.removeAttribute("aria-disabled");
    }, Math.max(0, until - Date.now()) + 30);
  }
  syncDice();
  reconcileAutoRoll();

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

// Confirmations for destructive forms.
//
// These used to be inline `onsubmit="return confirm(...)"` attributes, which the
// site's own CSP (`script-src 'self'`, no `unsafe-hashes`) refuses to run — so
// Remove / Close room / Leave submitted with no prompt at all. Ask here instead,
// in the capture phase so htmx never sees a cancelled submit.
(function () {
  document.addEventListener("submit", function (e) {
    var form = e.target;
    if (!form || !form.getAttribute) return;
    var message = form.getAttribute("data-confirm");
    if (!message) return;
    if (!window.confirm(message)) {
      e.preventDefault();
      e.stopImmediatePropagation();
    }
  }, true);
})();
