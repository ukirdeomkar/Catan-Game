# Play Store launch + offline play (issue #14)

Tracking issue: <https://github.com/ukirdeomkar/Catan-Game/issues/14>

> **Status: analysis + plan only.** No behaviour, layout, or backend code is changed in this
> PR. Implementation lands on this branch (`docs/playstore-offline`) in follow-up commits.

## 1. What is being asked

Two related goals, from the issue:

1. **Ship to the Google Play Store.** A friend has a Play developer account and publishes apps,
   so the release will go through his account.
2. **Let people play offline.** Online play with friends should keep working; solo play against
   bots should not need a server.

The constraint carried over from the discussion: **keep the same Rust backend** — do not
rewrite the game in Kotlin/Java.

## 2. Decision: TWA, not a native app

For "get it into the Play Store", **no native app is needed**. The sanctioned path is a
**Trusted Web Activity (TWA)** — Google's official way to ship a PWA as a Play Store listing.
It renders the site in the real Chrome engine, full-screen and chrome-less. It is *not* a
WebView wrapper.

The project already meets every TWA prerequisite:

| TWA requirement | Status |
|-----------------|--------|
| HTTPS with a valid cert | ✅ `playcatanou.duckdns.org` |
| Web app manifest (`start_url`, `display: standalone`, 192/512 icons) | ✅ `static/branding/site.webmanifest` |
| Service worker with a fetch handler | ✅ `static/sw.js` |
| Domain-verified via Digital Asset Links | ❌ **missing — only new server piece** |

**Why not a raw WebView wrapper:** Google's *minimum functionality / spam* policy routinely
rejects apps that are "just a website in a WebView". A TWA is manifest-driven and domain-verified,
so it is accepted as a real PWA install. If a wrapper is ever used instead, it must add native
value (offline shell, notifications, etc.) — which is exactly what §3 covers.

**Why not native (Kotlin/Compose):** it would mean porting the rules engine and re-implementing
the UI, contradicting the "same Rust backend" constraint. Only worth revisiting if we later want
deep device integration; it is not required for the store.

## 3. Offline play: compile the engine to WebAssembly

Today the game is **server-authoritative end to end** — even a solo game against bots hits the
server (`/create`, then `/action` + SSE), with `bot.rs` driven by `state::tick_bots` on the
server. So a plain TWA can install and open offline, but it **cannot play offline yet**: there is
no server in that mode.

The good news is the code is unusually ready for this. Verified in the repo:

- `src/game/` has **no** `tokio`, `std::fs`, or `std::net` usage — pure, synchronous rules.
- `src/bot.rs` is documented as "Pure and synchronous: it only reads a `GameState` and returns
  one legal `Action`".

That means **the exact same rules engine and bots can be compiled to `wasm32`** and run
on-device, giving one implementation shared by the server and the offline client.

### 3.1 Modes

| Mode | Where the engine runs | Transport | Backend |
|------|----------------------|-----------|---------|
| Online with friends | Axum server | SSE + htmx POSTs | **unchanged** |
| Offline vs bots | In-browser WASM | none (local) | not involved |

The online path on the server keeps running `game/` + `bot.rs` natively; the offline path runs
the same source compiled to WASM. No rules forking.

### 3.2 The real cost: a client-side render path

The UI is **server-rendered HTML fragments** (maud + htmx + SSE), so there is currently no
client-side rendering. Offline mode needs one. Two options:

1. **Compile `src/render.rs` to WASM too.** maud is pure Rust producing HTML strings, so the
   WASM module can emit the *same per-viewer fragments* on-device and a thin JS shim can swap
   them in exactly as htmx does for SSE today. Single source of truth for the view; larger WASM.
2. **A small JS/TS renderer used only for offline mode.** Smaller WASM, but duplicates the view
   logic and risks drift from the server rendering.

Recommendation: **(1)** — it preserves the "one implementation" property that makes this worth
doing, and the fragment shapes already exist. Decide before Phase 2.

`state::tick_bots` (which paces bot turns with timing) also needs a client-side equivalent tick
loop for offline games.

## 4. What changes and what does not

**Does not change:** `src/game/`, `src/bot.rs`, and the online request/SSE flow. The server keeps
serving the game exactly as it does now.

**Additive changes:**

- `src/handlers.rs` — serve `/.well-known/assetlinks.json` (root route) with the Android package
  name + signing-key fingerprint. Needed for TWA verification.
- A `wasm` build target for the engine (crate boundary so `game/` + `bot.rs` compile for both
  `wasm32-unknown-unknown` and the server target).
- A client render/tick path for offline mode (§3.2).
- `static/sw.js` — pre-cache the WASM module + data, and bump `CACHE` (`catanou-v4` → `v5`).
- An offline-mode entry point (e.g. a "Play offline" button on the home page) and local
  persistence (IndexedDB/localStorage) for an in-progress game.

## 5. Phased plan (checklist)

- [ ] **Phase 0 — TWA shell (online-only to start).** Generate the signed `.aab` (Bubblewrap or
      PWABuilder), add the `/.well-known/assetlinks.json` route + fingerprint, install-test on a
      real device, and get the listing into closed testing. Ships to Play *without* waiting on
      offline work.
- [ ] **Phase 1 — WASM build of the engine.** Add the `wasm` target and `wasm-bindgen`
      bindings for `game/` + `bot.rs`; prove a full bot game can be played headlessly in Node/a
      test page. No UI yet.
- [ ] **Phase 2 — client render path.** Wire the chosen option from §3.2 so offline mode renders
      the board, hand, dock, and sheets from WASM state.
- [ ] **Phase 3 — offline mode UX.** "Play offline" entry point, local bot games with a client
      tick loop, and persistence so a game survives a refresh.
- [ ] **Phase 4 — PWA offline packaging.** Add the WASM module + assets to the `sw.js` `SHELL`
      list, bump `CACHE`, and verify a cold start with the network disabled.
- [ ] **Phase 5 — release.** Promote closed testing → production on the friend's account;
      complete the Play Console checklist (§6).

## 6. Play Store checklist

- [ ] Play Console app created under the friend's developer account; app signing key generated.
- [ ] `assetlinks.json` served with the package name and the **Play App Signing** SHA-256
      (not the local debug key).
- [ ] `targetSdk` within one year of the latest Android release (Bubblewrap/PWABuilder sets this).
- [ ] Store listing: title, short/full description, screenshots (phone + tablet), feature graphic,
      and the 512×512 icon.
- [ ] Content rating questionnaire, privacy policy URL, and data-safety form.
- [ ] **Personal developer accounts created after 13 Nov 2023** must run a closed test with at
      least 12 testers for 14 continuous days before production access (verify current numbers in
      Play Console — policy has changed before).
- [ ] Closed-testing track populated, then production rollout after the testing window.

## 7. Risks & mitigations

| Risk | Mitigation |
|------|-----------|
| WebView-style rejection | Use TWA (domain-verified), not a bare WebView. |
| Asset-links misconfigured → address bar shows / verification fails | Serve `assetlinks.json` at the exact path; use the Play App Signing fingerprint; verify with Google's statement-list tool. |
| Personal-account testing gate blocks launch | Start closed testing in Phase 0 concurrently with the offline work. |
| WASM view drifts from server view | Prefer rendering `render.rs` to WASM (§3.2 option 1) — one source of truth. |
| Engine WM build pulls in IO deps later | Keep `game/`/`bot.rs` purity as an explicit invariant; add a CI check that they stay free of `tokio`/`std::fs`/`std::net`. |
| Offline cache misses WASM/art | Add to `sw.js` `SHELL` + bump `CACHE`; failures are already tolerated per-file. |
| Big-bang PR hard to review | Ship in the phases above as separate commits on this branch. |

## 8. Open questions

1. Offline render path: compile `render.rs` to WASM (option 1) or a separate JS renderer (option 2)?
2. Should offline offer bot difficulty selection + local save/resume, or just a single quick game?
3. Any desire for a native wrapper later (notifications, deep links), or is TWA the end state?
4. Package name to use for the Android app (`org.playcatanou.*`?), needed for `assetlinks.json`.
