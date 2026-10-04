(function () {
  var RES = ["wood", "brick", "wheat", "ore", "sheep"];
  var state = {};
  RES.forEach(function (r) { state[r] = 0; });

  function form() { return document.getElementById("discard-form"); }
  function need() {
    var f = form();
    return f ? parseInt(f.getAttribute("data-need"), 10) || 0 : 0;
  }
  function total() {
    return RES.reduce(function (s, r) { return s + (state[r] || 0); }, 0);
  }

  function sync() {
    var f = form();
    if (!f) return;
    RES.forEach(function (r) {
      var n = state[r] || 0;
      var btn = f.querySelector('.pick[data-res="' + r + '"]');
      if (btn) {
        var cnt = btn.querySelector(".pick-count");
        if (cnt) { cnt.textContent = n; cnt.setAttribute("data-count", String(n)); }
        btn.classList.toggle("sel", n > 0);
      }
      var hidden = f.querySelector('input[name="' + r + '"]');
      if (hidden) hidden.value = n;
    });
    var t = total();
    var status = document.getElementById("discard-status");
    if (status) status.textContent = t + " / " + need() + " selected";
    var submit = document.getElementById("discard-btn");
    if (submit) submit.disabled = t !== need();
  }

  function refreshCaps() {
    var f = form();
    if (!f) { RES.forEach(function (r) { state[r] = 0; }); return; }
    RES.forEach(function (r) {
      var btn = f.querySelector('.pick[data-res="' + r + '"]');
      if (!btn) return;
      var cap = parseInt(btn.getAttribute("data-max"), 10) || 0;
      btn.classList.toggle("disabled", cap <= 0);
      if (state[r] > cap) state[r] = cap;
    });
  }

  document.addEventListener("click", function (e) {
    if (!e.target.closest) return;
    var btn = e.target.closest('#discard-form .pick[data-res]');
    if (!btn) return;
    e.preventDefault();
    var f = form();
    if (!f) return;
    var r = btn.getAttribute("data-res");
    var cap = parseInt(btn.getAttribute("data-max"), 10) || 0;
    var n = state[r] || 0;
    var others = total() - n;
    var maxHere = Math.min(cap, Math.max(0, need() - others));
    state[r] = n >= maxHere ? 0 : n + 1;
    sync();
  });

  document.addEventListener("htmx:afterSwap", function () { refreshCaps(); sync(); });
  document.addEventListener("htmx:sseMessage", function () { refreshCaps(); sync(); });
  sync();
})();
