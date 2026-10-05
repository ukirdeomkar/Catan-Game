# Browser testing with the opencode Chrome DevTools MCP

How to exercise the running app end-to-end in a real browser from an agent
session. This is the workflow to use instead of (or in addition to) curl: it
catches the client-side issues curl cannot, such as htmx/SSE swapping, the
`HX-Retarget` handling, tap targets, and layout.

Everything below assumes the app is running locally (see §1) and the
`chrome-devtools_*` MCP tools are available.

---

## 1. Build and run the server in the background

Never block the session on `cargo run`. Build synchronously, then launch the
binary **detached** and poll for readiness.

```powershell
# Build (synchronous — fine)
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
cargo build --release

# Launch detached via the helper script (see scripts/serve-bg.ps1)
Start-Process pwsh -ArgumentList '-NoProfile','-File','.\scripts\serve-bg.ps1' -WindowStyle Hidden

# Poll readiness
Start-Sleep -Seconds 3
(Invoke-WebRequest http://127.0.0.1:8090/ -UseBasicParsing).StatusCode   # expect 200
```

`scripts/serve-bg.ps1` kills any previous `catan` process, then starts the
release binary with redirections and writes its PID to `%TEMP%\opencode\catan.pid`.
It uses a scratch data dir by default so test rooms don't pollute `./data`.

Env vars: `CATAN_ADDR` (default `0.0.0.0:8080`), `CATAN_DATA_DIR` (default `data`).
Use a non-8080 port if something else is bound.

Stop the server:

```powershell
Stop-Process -Id (Get-Content "$env:TEMP\opencode\catan.pid") -Force
```

---

## 2. Open Chrome and add a second player

Use two pages for 2-player tests. The second player **must** be in an
isolated browser context so its cookies don't overwrite the first player's.

```
chrome-devtools_list_pages
chrome-devtools_new_page(url="http://127.0.0.1:8090/")                      # player 1 (default context)
chrome-devtools_new_page(url="http://127.0.0.1:8090/", isolatedContext="p2") # player 2
```

Keep the returned `pageId`s; pass them to every later call. Use
`chrome-devtools_select_page` to focus, and `chrome-devtools_take_snapshot`
for accessibility-tree uids (works for HTML buttons/inputs).

---

## 3. Register players (UI first)

Prefer the real UI — it exercises the forms and validation.

- Create: fill the name input, pick a bit, click **Create room**; the page
  navigates to `/room/{CODE}`.
- Join (page 2, isolated context): fill room code + name, click **Join**.
- Start: on the host page click **Start game (n/4)**.

If a player is stuck on the Lobby after the host starts, that's a bug — the
lobby is live: joining players, bots, and the Start button all update via SSE,
and everyone auto-navigates into the game when the host starts. No reload should
be needed.

Fast path when you only care about game state (room is already created):

```js
// evaluate_script on the host page: submit the start form
() => { Array.from(document.querySelectorAll('form')).find(f=>/\/start$/.test(f.getAttribute('action'))).submit(); }
```

Name rules: 1–5 chars, unique per room (case-insensitive). Same-name join in a
started game **rejoins that seat** (fresh token).

---

## 4. Drive the board — SVG click gotcha

Board interactions live on SVG elements:

| Selector | Meaning | `hx-vals` |
|----------|---------|-----------|
| `#board circle.spot` | settlement/city vertex | `{"action":"place_vertex","vertex":N}` |
| `#board line.spot`   | road edge (thin line + invisible hit line) | `{"action":"place_edge","edge":N}` |
| `#board polygon.spot`| robber hex | `{"action":"move_robber","hex":N}` |

`SVGElement` has **no `.click()`**. Dispatch a bubbling `MouseEvent` instead —
this is exactly what htmx listens for:

```js
() => {
  const el = document.querySelector('#board circle.spot')   // or line.spot
          || document.querySelector('#board line.spot');
  if (!el) return { error: 'no legal spot', status: document.getElementById('status').textContent };
  el.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, view: window }));
  return { hxVals: el.getAttribute('hx-vals') };
}
```

Wait ~1s between actions for the SSE round-trip, then read the DOM.

---

## 5. Read state from the SSE-swapped fragments

The page keeps these elements up to date over SSE:

`#status`, `#board`, `#controls`, `#trades`, `#players`, `#hand`, `#log`,
`#toasts` (errors).

```js
() => ({
  status:  document.getElementById('status').textContent.replace(/\s+/g,' ').trim(),
  controls: Array.from(document.querySelectorAll('#controls button')).map(b=>({t:b.textContent.trim(), dis:b.disabled})),
  boardHasSvg: !!document.querySelector('#board svg'),
  hand: document.getElementById('hand').textContent.replace(/\s+/g,' ').trim(),
  toasts: document.getElementById('toasts').textContent.trim(),
  log: document.getElementById('log').textContent.replace(/\s+/g,' ').trim().slice(0,200),
})
```

A successful action returns an empty 200 with header `HX-Retarget: #toasts`
(the retarget is set server-side so action feedback always lands in `#toasts`
regardless of which element was clicked).

---

## 6. Trade flow

- Open the modal: click `[data-open-trade]` (the **⇄** button).
- Tap resource cards to build the offer:
  `.pick[data-group="give"][data-res="brick"]`, `... data-group="want" ...`.
  Each tap increments (wraps at 5).
- **📨 Offer** (`#offer-btn`) → a shared `.trade-card` with a `⏱ Ns` countdown.
- Others accept with the **✔ Accept** button in `.trade-card`.
- Bank exchange: select exactly one give + one want, then **🔁 Bank** (`#bank-btn`).

```js
() => {
  document.querySelector('[data-open-trade]').click();
  const g = document.querySelector('.pick[data-group="give"][data-res="brick"]');
  const w = document.querySelector('.pick[data-group="want"][data-res="ore"]');
  g.click(); g.click(); w.click();
  document.getElementById('offer-btn').click();
}
```

Offers auto-expire after 30s (`deadline_ms`), end early when everyone declines,
and the first accept wins. Countdown is driven by `static/timer.js` reading
`[data-deadline]` + `[data-secs]`.

---

## 7. Per-turn timer

Rooms can be created with a 1/3/5/10-minute turn limit (select on the home
page). The status bar shows `⏱ Ns`; when it hits zero the server force-ends
the turn (`"Turn ended by timer."` in the log) and the next player's timer arms.

```js
() => { const t=document.querySelector('.turn-timer'); return t && { text:t.textContent.trim(), deadline:t.getAttribute('data-deadline') }; }
```

---

## 8. Mobile check

```js
// chrome-devtools_emulate
{ pageId: <id>, viewport: "390x844x3,mobile,touch" }
```

Then screenshot and verify: the whole game fits one screen with **no page scroll** —
top bar (menu + resource bank + your dev/VP), the player strip (you leftmost, active
player shows `⏱ Ns`), the board filling the middle, the bottom dock (dice + build/trade/
dev/end buttons) and the hand strip. Open the Build / Trade / Menu buttons and check the
bottom sheets appear and are scrollable within themselves. Reset with
`viewport: "1280x800x1"` when done.

Note: `beforeinstallprompt` may pop the PWA install bar over the dock while testing; it is
raised above the dock via `#install-bar{bottom:160px}` but is harmless.

---

## 9. Useful introspection

- `chrome-devtools_list_network_requests` — confirm the POST to
  `/room/{code}/action` and the long-lived `GET .../events` (SSE).
- `chrome-devtools_get_network_request` — inspect request body/headers/status
  (timeout if pointed at the still-open SSE stream; use a POST reqid).
- `chrome-devtools_list_console_messages` — htmx/JS errors.
- `chrome-devtools_take_screenshot` (optionally `fullPage:true`) for visual proof.

---

## 10. Checklist for a full pass

1. Home → create room (try an over-long name → rejected; duplicate name in a
   room → rejoin or rejected for new names).
2. Join with a second isolated context; the host's lobby updates live (no
   reload); start the game and confirm other lobby viewers auto-enter the game.
   Host can also pick a bot difficulty and **Add bot** to fill seats (solo play),
   and bots then act on their own with a visible pace.
3. Setup: settlement + road for each player (snake order); board persists and
   pieces render (not cleared) after each click.
4. Play: roll (two dice with pips + total), build road/settlement/city, buy dev.
5. Unaffordable build buttons are **disabled**; error toast shows for invalid
   actions.
6. Turn enforcement: End-turn disabled before rolling.
7. Trade: picker modal, shared offer, first-accept, 30s expiry, all-decline.
8. Robber: hidden until moved; sits in a hex corner without covering the
   number/terrain; appears for all players.
9. Turn timer (if enabled): counts down and auto-ends.
10. Mobile viewport screenshot.

---

## 11. Bottom dock and sheets

The in-game screen is a fixed, no-scroll shell (`body.game`, `#app` in `render.rs`).
Per-viewer regions, all swapped over SSE:

- `#status` — top bar: menu button (`[data-sheet-open="menu"]`), resource bank from
  `game.bank`, and your dev-card / VP chips.
- `#players` — player strip, **rotated so the viewer is leftmost**; `.pcard.turn` is the
  active seat and carries the `[data-deadline]` / `[data-secs]` turn timer.
- `#board` — the SVG, flex-filled (`preserveAspectRatio="xMidYMid meet"`).
- `#controls` — a floating status pill (waiting / robber / win); empty when it is your move.
- `#turn` — the **dock**: dice + action buttons. `[data-my-turn]` marks your actionable
  turn (audio.js uses it). On your turn, before rolling it shows a single `.dockbtn.primary`
  Roll button; after rolling it shows build / trade / dev / end buttons. The dock also
  **contains the bottom sheets** (`.sheet[data-sheet=...]`): `build`, `dev`, `steal`
  (mandatory, `[data-auto]`), and `discard` (mandatory).
- `#hand` — the always-visible resource hand strip (`.rcard[data-res]` + `.rcard-count`).
- `#trades` — the floating active-offer card.

Sheets are toggled by `static/ui.js`: `[data-sheet-open="name"]` opens, `[data-sheet-close]`
closes, any action fired from inside a sheet closes it, and `ui.js` re-applies sheet
visibility after every SSE swap so a sheet survives the fragment re-render. Mandatory
sheets (`data-auto`) force-open and cannot be dismissed by the user.

The trade picker (`#trade-modal.modal`) is still managed by `static/trade.js`, but is now
styled as a bottom sheet. `ui.js` leaves it alone.

---

## 12. Gotchas (learned the hard way)

- **`hx-target` inheritance:** do not put `hx-target="#toasts"` on `#app` —
  the SSE `sse-swap` children inherit it and every fragment lands in the toast
  box. Action feedback is retargeted server-side with `HX-Retarget` instead.
- **Empty SSE data:** an SSE event with no `data:` line is never dispatched;
  the server sends a space for empty fragments (e.g. no active trade).
- **`evaluate_script` timeouts:** don't `await` the SSE stream in a page script;
  it never ends. Read state from the DOM instead.
- **Viewport resets between sessions** can leave you on a mobile layout; always
  reset with `chrome-devtools_emulate` `1280x800x1` before a desktop assertion.
- **The service worker caches `/static/` (stale-while-revalidate), so CSS/JS edits
  appear one reload late.** When iterating on `app.css`/`ui.js`, either clear it in
  the page (`await caches.keys().then(k=>Promise.all(k.map(caches.delete)))` +
  `navigator.serviceWorker.getRegistrations().then(r=>r.forEach(x=>x.unregister()))`)
  and reload with `ignoreCache: true`, or bump `CACHE` in `static/sw.js`.
