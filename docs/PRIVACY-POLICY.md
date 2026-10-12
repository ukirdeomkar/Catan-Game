# Privacy policy (Play Store listing)

Play Catanou needs a public privacy-policy URL for its Google Play listing. The policy
is served from the app itself, so it is one source of truth and stays online with the
game.

| Field | Value |
|-------|-------|
| Public URL | `https://playcatanou.duckdns.org/privacy` |
| Served from | `src/handlers.rs` → `render::privacy_page()` (`src/render.rs`) |
| Reachable without login | yes (static server-rendered HTML) |
| Applies to | website + Android app `org.playcatanou.twa` |
| Content of record | the page itself — this doc only records the URL and the data-safety answers |

## Data-safety summary (matches the on-page policy)

- **No accounts, no sign-up, no login.**
- Collects only a player-chosen **display name** (≤ 5 chars), shown to other players in
  the same room, plus a random **session cookie** (`catan_<ROOMCODE>`) to identify the
  player during a game.
- Game state (board, moves) is stored server-side so games survive refreshes; rooms are
  deleted after 24 hours idle.
- **No** ads, analytics, crash SDKs, location, contacts, camera, microphone, or files.
- No data sold or shared.

For the Play Console *Data safety* form, answer: no data collected/shared for advertising
or analytics; the display name and in-game state are collected for app functionality and
are deletable by leaving the room / automatic 24 h cleanup.

When the route ships, verify:

```bash
curl -sS -o /dev/null -w '%{http_code}\n' https://playcatanou.duckdns.org/privacy
curl -sS https://playcatanou.duckdns.org/privacy | grep -i 'Privacy Policy'
```
