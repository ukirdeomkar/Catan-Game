# Android app (Play Store TWA)

This is the Phase 0 Android shell for Catan-Game. It packages the existing PWA as a
**Trusted Web Activity (TWA)** with [Bubblewrap](https://github.com/GoogleChromeLabs/bubblewrap).
The Rust backend is unchanged; the TWA renders `https://playcatanou.duckdns.org` in Chrome.

## Identity

| Field | Value |
|-------|-------|
| Android package name | `org.playcatanou.twa` |
| Host | `playcatanou.duckdns.org` |
| `versionCode` | `1` |
| `versionName` | `1.0.0` |
| `minSdkVersion` | 24 |
| `targetSdkVersion` | 36 (set by the Bubblewrap/AGP template) |
| Keystore alias | `catanou-upload` |
| Keystore store type | `JKS` |

`minSdkVersion` is **24**, not the Bubblewrap template default of 21: Play's automatic
protection rejects any bundle below API 24 ("Play automatic protection requires a minimum
SDK version of 24 or higher"). Older devices below API 24 are therefore out of scope for
the Play Store listing.

The package name is the CEO decision recorded for issue #14 and is used consistently in
the Bubblewrap manifest, `assetlinks.json`, and this doc. It must not change after the
first Play upload.

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
curl -s https://playcatanou.duckdns.org/.well-known/assetlinks.json
```

## Rebuilding the bundle

Bubblewrap generates the Android project from `twa-manifest.json`. To start a fresh build:

```bash
npx @bubblewrap/cli init \
  --manifest https://playcatanou.duckdns.org/static/branding/site.webmanifest
# edit twa-manifest.json: packageId org.playcatanou.twa, signingKey path/alias,
# minSdkVersion 24 (Play requires >= 24), versions
npx @bubblewrap/cli build
```

`bubblewrap build` writes `./app-release-bundle.aab` (signed) alongside
`./app-release-signed.apk`. Bump `appVersionCode` for every Play upload; `appVersionName`
is the user-visible version.
