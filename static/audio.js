// Procedurally synthesized game audio (Web Audio API). Everything here is
// generated at runtime, so there are no audio files and no licensing.
(function () {
  var STORAGE = "catan_audio_muted";
  var muted = false;
  try { muted = localStorage.getItem(STORAGE) === "1"; } catch (e) {}

  var ctx = null;
  var master = null;
  var musicGain = null;
  var sfxGain = null;
  var noiseBuf = null;
  var started = false;

  function now() { return ctx ? ctx.currentTime : 0; }

  function ensureNoise() {
    if (noiseBuf) return noiseBuf;
    var len = Math.floor(ctx.sampleRate * 0.5);
    var b = ctx.createBuffer(1, len, ctx.sampleRate);
    var d = b.getChannelData(0);
    for (var i = 0; i < len; i++) { d[i] = Math.random() * 2 - 1; }
    noiseBuf = b;
    return b;
  }

  function init() {
    if (ctx) {
      if (ctx.state === "suspended" && ctx.resume) { ctx.resume(); }
      return;
    }
    var AC = window.AudioContext || window.webkitAudioContext;
    if (!AC) { return; }
    ctx = new AC();
    master = ctx.createGain();
    master.gain.value = muted ? 0 : 1;
    master.connect(ctx.destination);
    musicGain = ctx.createGain();
    musicGain.gain.value = 0.14;
    musicGain.connect(master);
    sfxGain = ctx.createGain();
    sfxGain.gain.value = 0.85;
    sfxGain.connect(master);
    startMusic();
  }

  function tone(freq, dur, type, peak, when, slideTo, dest) {
    if (!ctx) { return; }
    when = when || now();
    var o = ctx.createOscillator();
    o.type = type || "sine";
    o.frequency.setValueAtTime(freq, when);
    if (slideTo) { o.frequency.exponentialRampToValueAtTime(slideTo, when + dur); }
    var g = ctx.createGain();
    g.gain.setValueAtTime(0.0001, when);
    g.gain.exponentialRampToValueAtTime(peak, when + 0.012);
    g.gain.exponentialRampToValueAtTime(0.0001, when + dur);
    o.connect(g);
    g.connect(dest || sfxGain);
    o.start(when);
    o.stop(when + dur + 0.03);
  }

  function noise(when, dur, freq, q, peak) {
    if (!ctx) { return; }
    var src = ctx.createBufferSource();
    src.buffer = ensureNoise();
    var bp = ctx.createBiquadFilter();
    bp.type = "bandpass";
    bp.frequency.value = freq;
    bp.Q.value = q || 1;
    var g = ctx.createGain();
    g.gain.setValueAtTime(0.0001, when);
    g.gain.exponentialRampToValueAtTime(peak, when + 0.005);
    g.gain.exponentialRampToValueAtTime(0.0001, when + dur);
    src.connect(bp);
    bp.connect(g);
    g.connect(sfxGain);
    src.start(when);
    src.stop(when + dur + 0.02);
  }

  // ------------------------------------------------------------------
  // Ambient background music: a soft drone plus sparse bell notes.
  // ------------------------------------------------------------------
  var PENT = [261.63, 293.66, 329.63, 392.0, 440.0, 523.25, 587.33];

  function startMusic() {
    if (!ctx || started) { return; }
    started = true;
    var t = now();
    [130.81, 196.0].forEach(function (f, i) {
      var o = ctx.createOscillator();
      o.type = "sine";
      o.frequency.value = f;
      var g = ctx.createGain();
      g.gain.value = i ? 0.03 : 0.045;
      var lfo = ctx.createOscillator();
      lfo.frequency.value = 0.05 + i * 0.03;
      var lg = ctx.createGain();
      lg.gain.value = 0.018;
      lfo.connect(lg);
      lg.connect(g.gain);
      lfo.start(t);
      o.connect(g);
      g.connect(musicGain);
      o.start(t);
    });
    scheduleBell(t + 0.4);
  }

  function scheduleBell(t) {
    if (!ctx) { return; }
    var f = PENT[(Math.random() * PENT.length) | 0];
    var o = ctx.createOscillator();
    o.type = "triangle";
    o.frequency.value = f;
    var lp = ctx.createBiquadFilter();
    lp.type = "lowpass";
    lp.frequency.value = 1700;
    var g = ctx.createGain();
    g.gain.setValueAtTime(0.0001, t);
    g.gain.exponentialRampToValueAtTime(0.07, t + 0.03);
    g.gain.exponentialRampToValueAtTime(0.0001, t + 2.2);
    o.connect(lp);
    lp.connect(g);
    g.connect(musicGain);
    o.start(t);
    o.stop(t + 2.3);
    setTimeout(function () { scheduleBell(now() + 0.1); }, 1500 + Math.random() * 2400);
  }

  // ------------------------------------------------------------------
  // Sound effects
  // ------------------------------------------------------------------
  function sfxDice() {
    if (!ctx) { return; }
    var t = now();
    for (var i = 0; i < 6; i++) {
      noise(t + i * 0.055 + Math.random() * 0.02, 0.05, 1100 + Math.random() * 1600, 6, 0.4);
    }
    noise(t + 0.36, 0.09, 480, 3, 0.5);
    tone(180, 0.12, "triangle", 0.22, t + 0.36, 120);
  }
  function sfxTurn() {
    if (!ctx) { return; }
    var t = now();
    tone(392.0, 0.16, "sine", 0.2, t);
    tone(523.25, 0.22, "sine", 0.18, t + 0.12);
  }
  function sfxYourTurn() {
    if (!ctx) { return; }
    var t = now();
    [523.25, 659.25, 783.99].forEach(function (f, i) {
      tone(f, 0.3, "triangle", 0.25, t + i * 0.1);
    });
  }
  function sfxOffer() {
    if (!ctx) { return; }
    var t = now();
    tone(587.33, 0.16, "sine", 0.24, t);
    tone(880.0, 0.2, "sine", 0.2, t + 0.14);
  }
  function sfxDown() {
    if (!ctx) { return; }
    tone(392.0, 0.2, "sine", 0.2, now(), 261.63);
  }
  function sfxError() {
    if (!ctx) { return; }
    var t = now();
    tone(220.0, 0.18, "square", 0.16, t, 160.0);
    tone(160.0, 0.22, "square", 0.14, t + 0.16, 120.0);
  }
  function sfxWin() {
    if (!ctx) { return; }
    var t = now();
    [523.25, 659.25, 783.99, 1046.5].forEach(function (f, i) {
      tone(f, 0.4, "triangle", 0.28, t + i * 0.13);
    });
  }

  // ------------------------------------------------------------------
  // Event detection: diff the SSE-swapped panels.
  // ------------------------------------------------------------------
  function snap() {
    var cur = null;
    var pc = document.querySelector("#players .pcard.turn .pname");
    if (pc) { cur = pc.textContent.trim(); }

    var dice = null;
    var dt = document.querySelector("#status .dice-wrap .dice-total");
    if (dt) { dice = dt.textContent.replace(/[^0-9]/g, ""); }

    var myTurn = !!document.querySelector("#turn .tg, #turn .tg-bar");

    var trade = null;
    var tc = document.querySelector("#trades .trade-card");
    if (tc) { trade = tc.getAttribute("data-deadline") || "t"; }

    var toastEl = document.getElementById("toasts");
    var toast = toastEl ? toastEl.textContent.trim() : "";

    var win = !!document.querySelector("#status .win, #controls .win");

    return { cur: cur, dice: dice, myTurn: myTurn, trade: trade, toast: toast, win: win };
  }

  var prev = null;
  var pending = false;
  function check() {
    if (pending) { return; }
    pending = true;
    setTimeout(function () {
      pending = false;
      var s = snap();
      if (!prev) { prev = s; return; }
      if (s.win && !prev.win) { init(); sfxWin(); prev = s; return; }
      if (s.toast && s.toast !== prev.toast) { init(); sfxError(); prev = s; return; }
      if (s.myTurn && !prev.myTurn) { init(); sfxYourTurn(); prev = s; return; }
      if (s.dice && s.dice !== prev.dice) {
        init(); sfxDice();
        if (s.dice === "7") { setTimeout(sfxError, 360); }
        prev = s; return;
      }
      if (s.trade && s.trade !== prev.trade) { init(); sfxOffer(); prev = s; return; }
      if (!s.trade && prev.trade) { init(); sfxDown(); prev = s; return; }
      if (s.cur && s.cur !== prev.cur) { init(); sfxTurn(); prev = s; return; }
      prev = s;
    }, 40);
  }

  // ------------------------------------------------------------------
  // Toggle + unlock
  // ------------------------------------------------------------------
  function applyToggle() {
    var b = document.getElementById("audio-toggle");
    if (b) {
      b.textContent = muted ? "🔇" : "🔊";
      b.classList.toggle("off", muted);
      b.setAttribute("aria-pressed", muted ? "false" : "true");
    }
    if (master) { master.gain.value = muted ? 0 : 1; }
  }

  document.addEventListener("click", function (e) {
    if (!e.target || !e.target.closest) { return; }
    var b = e.target.closest("#audio-toggle");
    if (!b) { return; }
    muted = !muted;
    try { localStorage.setItem(STORAGE, muted ? "1" : "0"); } catch (err) {}
    init();
    applyToggle();
  });

  ["pointerdown", "keydown", "touchstart"].forEach(function (ev) {
    window.addEventListener(ev, function once() {
      init();
      if (ctx && ctx.state === "suspended" && ctx.resume) { ctx.resume(); }
      window.removeEventListener(ev, once);
    });
  });
  document.addEventListener("pointerdown", function () {
    if (ctx && ctx.state === "suspended" && ctx.resume) { ctx.resume(); }
  });

  document.addEventListener("htmx:sseMessage", check);
  document.addEventListener("htmx:afterSwap", check);

  function boot() { prev = snap(); applyToggle(); }
  if (document.readyState === "complete" || document.readyState === "interactive") {
    boot();
  } else {
    document.addEventListener("DOMContentLoaded", boot);
  }
})();
