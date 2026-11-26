# Android app (Play Store TWA)

This is the Phase 0 Android shell for Hex Ports. It packages the existing PWA as a
**Trusted Web Activity (TWA)** with [Bubblewrap](https://github.com/GoogleChromeLabs/bubblewrap).
The Rust backend is unchanged; the TWA renders `https://hexports.duckdns.org` in Chrome.

## Play Store listing copy

Final copy for the Play Console listing. It describes mechanics only: no third-party game
name, brand, or "inspired by" phrasing appears anywhere in the metadata.

**App name (title):** `Hex Ports`

**Short description (57 / 80 chars):** `Build, trade, and settle a hex island with friends online.`

**Full description:**

```text
Hex Ports is a fast, friendly board strategy game for 2-4 players. Claim a fresh hex
island, gather wood, brick, wheat, ore, and sheep, then build roads, settlements, and
cities to control the best ports and trade routes.

Play with friends online in a private room: create a game, share the short room code,
and start in seconds. Add computer opponents at Easy, Medium, or Hard to fill the table
or practise on your own.

Each turn you roll the dice for resources, then build, trade with other players or the
bank, and buy development cards. Race to 10 victory points using settlements, cities,
the Longest Road, and the Largest Army bonus.

Features:
- 2-4 players in private online rooms
- A fresh randomized hex island every game
- Player trades plus bank and port exchange rates
- Development cards, the robber, and a 10-victory-point win
- Easy, Medium, and Hard AI opponents
- Auto-rolling dice, a live game log, and an end-of-game replay
- Three turn-length options and an optional pause
- Installs as an app on Android; the app shell works offline

No accounts, no ads, no tracking. Pick a name and play.

Build, trade, and settle your island - grab the best ports and win.
```

## Identity

| Field | Value |
|-------|-------|
| Android package name | `org.hexports.twa` |
| Host | `hexports.duckdns.org` |
| `versionCode` | `2` (launcher-icon fix, OMK-464; Play upload pending) |
| `versionName` | `1.1.0` |
| `minSdkVersion` | 24 |
| `targetSdkVersion` | 36 (set by the Bubblewrap/AGP template) |
| Keystore alias | `catanou-upload` |
| Keystore store type | `JKS` |

`minSdkVersion` is **24**, not the Bubblewrap template default of 21: Play's automatic
protection rejects any bundle below API 24 ("Play automatic protection requires a minimum
SDK version of 24 or higher"). Older devices below API 24 are therefore out of scope for
the Play Store listing.

The package name is the board decision recorded for OMK-636 (the Hex Ports rebrand that
replaced the earlier provisional name) and is used consistently in the Bubblewrap manifest,
`assetlinks.json`, and this doc. It is a **new** package, so this is its first Play upload and
the name must not change afterwards.

## Launcher icon (maskable / adaptive)

Android launcher icons are **adaptive icons**: the system masks the outer third of the
108dp layer and draws a background layer underneath the foreground art. Bubblewrap's
template uses a **white** `ic_launcher_background`, so artwork without a maskable variant
was inset into the safe zone on white — the white ring reported in GitHub #47.

The repo now ships maskable artwork alongside the existing `any` icons:

| Asset | Contents |
|-------|----------|
| `static/branding/icon-maskable-192.png` | 192x192, full-bleed `#12181f`, logo at 66% |
| `static/branding/icon-maskable-512.png` | 512x512, full-bleed `#12181f`, logo at 66% |

Both are declared in `static/branding/site.webmanifest` with `"purpose": "maskable"`; the
`any` and `apple-touch` icons are unchanged. The artwork is scaled to 66% so the plaque
stays inside the adaptive-icon safe zone — a circular or squircle mask crops only the
dark wooden frame, never the logo.

The generated Android project must opt into them. After `bubblewrap init`, edit
`twa-manifest.json`:

- `"maskableIconUrl": "https://hexports.duckdns.org/static/branding/icon-maskable-512.png"`
- `"backgroundColor": "#12181f"`

If the generated project hardcodes a white `ic_launcher_background` (Bubblewrap versions
differ), change that resource to `#12181f` as well. Keep `iconUrl` pointing at the regular
`icon-512.png`.

**Limitation:** nothing in this repo can change the icon of an already-installed Play app.
The device-visible fix requires building a new `.aab` (versionCode bump) and uploading it
to Play.

## Signing

The release `.aab` is signed with a fresh upload key. The keystore and its passwords are
**never committed**. They are delivered to the board through Paperclip secret proposals:

- `catan/android/upload-keystore.b64` — base64 of the JKS keystore
- `catan/android/keystore-password`
- `catan/android/key-password`

Local **upload-key** SHA-256 fingerprint (used for local install testing):

```
B3:A7:69:04:97:60:66:AB:68:07:26:A3:2D:01:46:07:B0:2C:F0:40:81:9C:B1:13:AD:7B:50:A1:60:50:C3:59
```

### Play App Signing

When the `.aab` is uploaded to Play, Google re-signs the delivered APK with a **Play App
Signing** key. Devices therefore verify the app against the **Play App Signing**
certificate, not the local upload key. The board supplied this value (OMK-44); the Play
Console shows it under **Test and release → App integrity → App signing key certificate**:

```
F2:62:F8:12:66:14:0E:4B:97:29:DF:62:7D:41:BD:3D:41:28:BF:D8:9A:ED:FA:2C:C8:5B:71:65:51:43:9C:ED
```

## Digital Asset Links

The server serves `/.well-known/assetlinks.json` from `src/handlers.rs` with the package
name above. `DEFAULT_SHA256_FINGERPRINTS` lists **both** certificates — the local upload
key (for side-loaded builds) and the Play App Signing certificate (for Play installs) — so
the app verifies either way. Override the list at runtime with the comma-separated
`CATAN_ANDROID_SHA256_FINGERPRINTS` environment variable; no code change is needed to
rotate a certificate.

Verify with:

```bash
curl -s https://hexports.duckdns.org/.well-known/assetlinks.json
```

## Rebuilding the bundle

Bubblewrap generates the Android project from `twa-manifest.json`. To start a fresh build:

```bash
npx @bubblewrap/cli init \
  --manifest https://hexports.duckdns.org/static/branding/site.webmanifest
# edit twa-manifest.json: packageId org.hexports.twa, signingKey path/alias,
# minSdkVersion 24 (Play requires >= 24), versions,
# maskableIconUrl (icon-maskable-512.png) and backgroundColor #12181f
npx @bubblewrap/cli build
```

`bubblewrap build` writes `./app-release-bundle.aab` (signed) alongside
`./app-release-signed.apk`. Bump `appVersionCode` for every Play upload; `appVersionName`
is the user-visible version. The current source is uploaded once the `.aab` is built; the
Play upload itself is board/friend-owned.
