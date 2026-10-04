// PWA install affordance.
//
// Registers the service worker (required for Chrome's install prompt) and
// surfaces a one-tap "Install" button as soon as the browser says the app is
// installable. iOS Safari has no `beforeinstallprompt`, so it gets a short
// Add-to-Home-Screen hint instead.

(function () {
  if ("serviceWorker" in navigator) {
    window.addEventListener("load", function () {
      navigator.serviceWorker.register("/sw.js").catch(function () {});
    });
  }

  var deferred = null;
  var bar = null;

  function isStandalone() {
    return (
      (window.matchMedia && window.matchMedia("(display-mode: standalone)").matches) ||
      window.navigator.standalone === true
    );
  }

  function dismissed() {
    try {
      return localStorage.getItem("catan_install_dismissed") === "1";
    } catch (e) {
      return false;
    }
  }

  function buildBar(html) {
    if (bar) return bar;
    bar = document.createElement("div");
    bar.id = "install-bar";
    bar.innerHTML =
      '<span class="ib-text"></span>' +
      '<button type="button" id="install-btn">Install</button>' +
      '<button type="button" id="install-close" aria-label="Dismiss">✕</button>';
    bar.querySelector(".ib-text").innerHTML = html;
    document.body.appendChild(bar);

    bar.querySelector("#install-btn").addEventListener("click", function () {
      if (!deferred) return;
      var choice = deferred;
      deferred = null;
      hide();
      choice.prompt();
      if (choice.userChoice && choice.userChoice.finally) {
        choice.userChoice.finally(function () {});
      }
    });
    bar.querySelector("#install-close").addEventListener("click", function () {
      try {
        localStorage.setItem("catan_install_dismissed", "1");
      } catch (e) {}
      hide();
    });
    return bar;
  }

  function show(html, withButton) {
    if (dismissed() || isStandalone()) return;
    var el = buildBar(html);
    var btn = el.querySelector("#install-btn");
    if (btn) btn.style.display = withButton ? "" : "none";
    el.style.display = "flex";
  }

  function hide() {
    if (bar) bar.style.display = "none";
  }

  window.addEventListener("beforeinstallprompt", function (e) {
    e.preventDefault();
    deferred = e;
    show("Install <b>Play Catanou</b> as an app", true);
  });

  window.addEventListener("appinstalled", function () {
    deferred = null;
    hide();
  });

  // iOS Safari never fires beforeinstallprompt; show the manual hint instead.
  var ua = window.navigator.userAgent || "";
  var isIOS = /iPad|iPhone|iPod/.test(ua) && !window.MSStream;
  if (isIOS && !isStandalone() && !dismissed()) {
    // Wait a moment so the bar does not flash during first paint.
    setTimeout(function () {
      show('Add to Home Screen: tap <b>Share</b> then <b>Add to Home Screen</b>', false);
    }, 2500);
  }
})();
