// Two-step dice roll.
//
// The dock dice area opens a board overlay for the active player who has not
// rolled. The roll POST only exists inside that overlay, so the tap that
// starts a roll can never land on the End Turn button on the other side of
// the dock. The overlay is moved to the stable #roll-root while open so an
// SSE turn swap cannot close it mid-roll; once the dice arrive it shows the
// result and dismisses itself.
(function () {
  var pending = false;
  var rollSentAt = 0;
  var settleUntil = 0;
  var lastFocus = null;
  var timeout = 0;

  function overlay() {
    return document.querySelector("#roll-root [data-roll-overlay]");
  }
  function turnOverlay() {
    return document.querySelector("#turn [data-roll-overlay]");
  }
  function liveOverlay() {
    var el = overlay();
    return el && !el.hidden ? el : null;
  }

  function lockScroll(on) {
    document.documentElement.classList.toggle("roll-open", on);
    document.body.classList.toggle("roll-open", on);
  }

  function focusables(el) {
    var all = el.querySelectorAll(
      'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])'
    );
    var out = [];
    for (var i = 0; i < all.length; i++) {
      if (all[i].offsetParent !== null) out.push(all[i]);
    }
    return out;
  }

  function open() {
    var host = document.getElementById("roll-root");
    var el = turnOverlay() || overlay();
    if (!host || !el) return;
    var stale = overlay();
    if (stale && stale !== el) stale.remove();
    lastFocus = document.activeElement;
    host.appendChild(el);
    el.hidden = false;
    lockScroll(true);
    var first = el.querySelector("[data-roll-action]") || el.querySelector("button");
    if (first) first.focus();
  }

  function close() {
    var el = overlay();
    if (!el) return;
    el.hidden = true;
    lockScroll(false);
    var turn = document.getElementById("turn");
    var duplicate = turnOverlay();
    if (turn && document.querySelector("#turn [data-roll-open]")) {
      if (duplicate && duplicate !== el) duplicate.remove();
      turn.appendChild(el);
    } else if (el.parentNode) {
      el.parentNode.removeChild(el);
    }
    if (lastFocus && document.contains(lastFocus)) lastFocus.focus();
    lastFocus = null;
  }

  function startRoll(e) {
    var el = overlay();
    if (!el) return;
    var btn = e.target.closest("[data-roll-action]");
    if (!btn) return;
    if (pending || btn.disabled) {
      e.preventDefault();
      e.stopPropagation();
      return;
    }
    pending = true;
    rollSentAt = Date.now();
    btn.disabled = true;
    btn.classList.add("is-rolling");
    var wrap = el.querySelector("[data-roll-dicewrap]");
    if (wrap) wrap.classList.add("rolling");
    var status = el.querySelector("[data-roll-status]");
    if (status) status.textContent = "Rolling…";
    timeout = window.setTimeout(function () {
      if (!pending) return;
      pending = false;
      var box = liveOverlay();
      if (!box) return;
      var b = box.querySelector("[data-roll-action]");
      var s = box.querySelector("[data-roll-status]");
      if (b) {
        b.disabled = false;
        b.classList.remove("is-rolling");
      }
      if (wrap) wrap.classList.remove("rolling");
      if (s) s.textContent = "The dice did not arrive — tap Roll dice to try again.";
    }, 10000);
  }

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

  function onUpdate() {
    guardEndTurn();
    if (!pending) return;
    var real = document.querySelector("#turn .dock-dice[data-my-dice] .dice-real");
    if (!real) return;
    pending = false;
    rollSentAt = 0;
    window.clearTimeout(timeout);
    // The result just landed: give End Turn a short settle window so an
    // already-in-flight tap near the dock cannot skip the turn.
    settleUntil = Date.now() + 850;
    guardEndTurn();
    var el = liveOverlay();
    if (!el) return;
    var stage = el.querySelector(".roll-stage");
    var btn = el.querySelector("[data-roll-action]");
    var status = el.querySelector("[data-roll-status]");
    var title = el.querySelector("h2");
    if (stage) stage.innerHTML = real.innerHTML;
    if (btn) btn.hidden = true;
    if (title) title.textContent = "You rolled";
    if (status) status.textContent = "Good luck!";
    window.setTimeout(function () {
      if (overlay()) close();
    }, 850);
  }

  document.addEventListener(
    "click",
    function (e) {
      if (!e.target.closest) return;
      if (e.target.closest("[data-roll-open]")) {
        e.preventDefault();
        open();
        return;
      }
      if (e.target.closest("[data-roll-close]")) {
        e.preventDefault();
        close();
        return;
      }
      if (e.target.closest("[data-roll-action]")) startRoll(e);
    },
    true
  );

  document.addEventListener("keydown", function (e) {
    var el = liveOverlay();
    if (!el) return;
    if (e.key === "Escape") {
      e.preventDefault();
      close();
      return;
    }
    if (e.key !== "Tab") return;
    var items = focusables(el);
    if (!items.length) return;
    var first = items[0];
    var last = items[items.length - 1];
    if (e.shiftKey && document.activeElement === first) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && document.activeElement === last) {
      e.preventDefault();
      first.focus();
    }
  });

  document.addEventListener("htmx:afterSwap", onUpdate);
  document.addEventListener("htmx:sseMessage", onUpdate);
})();
