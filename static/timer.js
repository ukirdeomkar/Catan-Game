(function () {
  function tick() {
    var now = Date.now();
    var nodes = document.querySelectorAll("[data-deadline]");
    for (var i = 0; i < nodes.length; i++) {
      var el = nodes[i];
      var out = el.querySelector("[data-secs]");
      if (!out) continue;
      var d = parseInt(el.getAttribute("data-deadline"), 10);
      if (!d) { out.textContent = ""; continue; }
      var left = Math.max(0, Math.ceil((d - now) / 1000));
      out.textContent = left + "s";
      var timer = el.querySelector(".trade-timer");
      if (timer) timer.classList.toggle("expiring", left <= 10);
    }
  }
  setInterval(tick, 500);
  document.addEventListener("htmx:afterSwap", tick);
  document.addEventListener("htmx:sseMessage", tick);
  tick();
})();
