(function () {
  var dismissed = false;
  var lastStep = null;
  var rollHoldUntil = 0;

  function guideEl() {
    var turn = document.getElementById("turn");
    return turn ? turn.querySelector("[data-step]") : null;
  }

  function showSub(name) {
    var guide = guideEl();
    if (!guide) return;
    var menu = guide.querySelector(".tg-menu");
    var subs = guide.querySelectorAll(".tg-sub");
    if (menu) menu.hidden = !!name;
    for (var i = 0; i < subs.length; i++) {
      subs[i].hidden = subs[i].getAttribute("data-sub") !== name;
    }
  }

  function render() {
    var guide = guideEl();
    var controls = document.getElementById("controls");
    if (!guide) {
      dismissed = false;
      lastStep = null;
      if (controls) controls.hidden = false;
      return;
    }
    var step = guide.getAttribute("data-step");
    if (step !== lastStep) {
      dismissed = false;
      lastStep = step;
      showSub(false);
    }
    if (dismissed) {
      guide.style.display = "none";
      if (controls) controls.hidden = false;
      return;
    }
    guide.style.display = "";
    if (controls) controls.hidden = true;
  }

  // Reveal a just-rolled result only after a short tumbling animation.
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

  document.addEventListener("click", function (e) {
    if (!e.target.closest) return;
    var rb = e.target.closest("[hx-vals]");
    if (rb && /"roll"/.test(rb.getAttribute("hx-vals") || "")) {
      rollHoldUntil = Date.now() + 2500;
    }
    var t = e.target.closest("[data-tg-menu],[data-tg-back],[data-tg-dismiss]");
    if (!t) return;
    if (t.hasAttribute("data-tg-menu")) {
      showSub(t.getAttribute("data-tg-menu"));
    } else if (t.hasAttribute("data-tg-back")) {
      showSub(false);
    } else if (t.hasAttribute("data-tg-dismiss")) {
      dismissed = true;
      render();
    }
    e.preventDefault();
  });

  // Notifications derived from new log entries (trade outcomes, robber/steal, 7s).
  var logInit = false;
  var lastLog = null;
  var OUTCOME = /complete a trade|declined|withdrawn|expired|moves the robber|steals a card|finds nothing|= 7\.|plays a Knight/i;
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

  document.addEventListener("htmx:afterSwap", function () { render(); paintDice(); checkLog(); });
  document.addEventListener("htmx:sseMessage", function () { render(); paintDice(); checkLog(); });
  render();
})();
