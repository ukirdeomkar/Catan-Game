(function () {
  // When the host starts, the server emits a `started` SSE event carrying the
  // room URL. Navigate all lobby viewers into the game automatically.
  document.addEventListener("htmx:sseOpen", function (e) {
    var src = e.detail && e.detail.source;
    if (!src || typeof src.addEventListener !== "function") return;
    src.addEventListener("started", function (ev) {
      location.href = ev.data || location.pathname;
    });
  });

  // ---- Invite link ---------------------------------------------------------
  // The lobby renders `/join?code=ABC123` as a relative fallback. Upgrade it to
  // an absolute URL once, so the value can be copied, shared, or selected by
  // hand when the clipboard API is unavailable.
  function absoluteInvite() {
    var input = document.getElementById("invite-link");
    if (!input) return "";
    var value = input.value || "";
    if (value.charAt(0) === "/") {
      value = location.origin + value;
      input.value = value;
    }
    return value;
  }

  var statusTimer = 0;
  function setStatus(msg) {
    var status = document.getElementById("invite-status");
    if (!status) return;
    status.textContent = msg;
    window.clearTimeout(statusTimer);
    statusTimer = window.setTimeout(function () {
      status.textContent = "";
    }, 3000);
  }

  // No clipboard API (insecure origin, older browser, or a denied permission):
  // put the link in a selected text field so the player can copy it by hand.
  function selectFallback(input) {
    try {
      input.focus();
      input.select();
      input.setSelectionRange(0, input.value.length);
    } catch (err) {
      // Selection is best-effort; the field is still visible and tappable.
    }
    setStatus("Select the link above and copy it");
  }

  function copyInvite() {
    var input = document.getElementById("invite-link");
    if (!input) return;
    var url = absoluteInvite();
    if (navigator.clipboard && navigator.clipboard.writeText) {
      navigator.clipboard.writeText(url).then(
        function () {
          setStatus("Invite link copied");
        },
        function () {
          selectFallback(input);
        }
      );
    } else {
      selectFallback(input);
    }
  }

  function shareInvite() {
    if (!navigator.share) return;
    navigator.share({
      title: "Join my Hexharbor room",
      text: "Join my Hexharbor game",
      url: absoluteInvite(),
    }).catch(function () {
      // The player dismissed the native share sheet; nothing to report.
    });
  }

  document.addEventListener("click", function (e) {
    if (!e.target.closest) return;
    if (e.target.closest("#copy-invite")) {
      copyInvite();
    } else if (e.target.closest("#share-invite")) {
      shareInvite();
    }
  });

  // A deferred script runs after the document is parsed, so the controls exist.
  absoluteInvite();
  var share = document.getElementById("share-invite");
  if (share && navigator.share) share.hidden = false;
})();
