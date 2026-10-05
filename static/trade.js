(function () {
  var RES = ["wood", "brick", "wheat", "ore", "sheep"];
  var EMOJI = { wood: "🌲", brick: "🧱", wheat: "🌾", ore: "🪨", sheep: "🐑" };
  var state = { give: {}, want: {} };
  RES.forEach(function (r) { state.give[r] = 0; state.want[r] = 0; });

  function selected(group) {
    return RES.filter(function (r) { return state[group][r] > 0; });
  }
  function preview(group) {
    return selected(group)
      .map(function (r) { return EMOJI[r] + (state[group][r] > 1 ? state[group][r] : ""); })
      .join(" ");
  }
  function sync() {
    var gTot = 0, wTot = 0;
    ["give", "want"].forEach(function (group) {
      document.querySelectorAll('.pick[data-group="' + group + '"]').forEach(function (btn) {
        var r = btn.getAttribute("data-res");
        var n = state[group][r] || 0;
        var cnt = btn.querySelector(".pick-count");
        if (cnt) { cnt.textContent = n; cnt.setAttribute("data-count", String(n)); }
        btn.classList.toggle("sel", n > 0);
        var hidden = document.querySelector('#trade-form input[name="' + group + "_" + r + '"]');
        if (hidden) hidden.value = n;
        if (group === "give") gTot += n; else wTot += n;
      });
    });
    var pg = document.getElementById("preview-give");
    var pw = document.getElementById("preview-want");
    if (pg) pg.textContent = preview("give") || "—";
    if (pw) pw.textContent = preview("want") || "—";
    var offer = document.getElementById("offer-btn");
    if (offer) offer.disabled = !(gTot > 0 && wTot > 0);
    var bank = document.getElementById("bank-btn");
    if (bank) bank.disabled = !(selected("give").length === 1 && selected("want").length === 1);
  }
  function clearSel() {
    ["give", "want"].forEach(function (g) { RES.forEach(function (r) { state[g][r] = 0; }); });
    sync();
  }
  function refreshGiveCaps() {
    var counts = {};
    document.querySelectorAll("#hand .rcard[data-res]").forEach(function (c) {
      var r = c.getAttribute("data-res");
      var n = c.querySelector(".rcard-count");
      counts[r] = n ? (parseInt(n.textContent, 10) || 0) : 0;
    });
    document.querySelectorAll('.pick[data-group="give"]').forEach(function (btn) {
      var r = btn.getAttribute("data-res");
      var have = counts[r] !== undefined ? counts[r] : 0;
      btn.setAttribute("data-max", String(have));
      btn.classList.toggle("disabled", have <= 0);
      if (state.give[r] > have) state.give[r] = have;
    });
  }
  function openModal() {
    refreshGiveCaps();
    sync();
    var m = document.getElementById("trade-modal");
    if (m) m.hidden = false;
  }
  function closeModal() { var m = document.getElementById("trade-modal"); if (m) m.hidden = true; }

  document.addEventListener("click", function (e) {
    var t = e.target.closest ? e.target.closest("[data-open-trade],[data-close-trade],.pick,#bank-btn,#offer-btn") : null;
    if (!t) return;
    if (t.hasAttribute("data-open-trade")) { openModal(); e.preventDefault(); return; }
    if (t.hasAttribute("data-close-trade")) { closeModal(); e.preventDefault(); return; }
    if (t.classList.contains("pick")) {
      var group = t.getAttribute("data-group");
      if (group !== "give" && group !== "want") { e.preventDefault(); return; }
      var r = t.getAttribute("data-res");
      var n = state[group][r] || 0;
      var capAttr = t.getAttribute("data-max");
      var cap = capAttr === null ? 5 : (parseInt(capAttr, 10) || 0);
      if (cap <= 0) { e.preventDefault(); return; }
      state[group][r] = n >= cap ? 0 : n + 1;
      sync();
      e.preventDefault();
      return;
    }
    var form = document.getElementById("trade-form");
    if (!form) return;
    if (t.id === "bank-btn") {
      form.querySelector('input[name="action"]').value = "bank_trade";
      form.querySelector('input[name="give"]').value = selected("give")[0];
      form.querySelector('input[name="want"]').value = selected("want")[0];
    } else if (t.id === "offer-btn") {
      form.querySelector('input[name="action"]').value = "propose_trade";
    }
  });

  document.addEventListener("htmx:afterRequest", function (e) {
    if (e.target && e.target.id === "trade-form") { clearSel(); closeModal(); }
  });
  document.addEventListener("htmx:afterSwap", function () { refreshGiveCaps(); sync(); });
  document.addEventListener("htmx:sseMessage", function () { refreshGiveCaps(); sync(); });
  sync();
})();
