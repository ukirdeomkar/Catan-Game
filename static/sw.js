// Play Catanou service worker.
//
// Its job is to make the app installable (Chrome fires `beforeinstallprompt`
// only when a service worker with a real fetch handler controls the page) and
// to give a small offline fallback. The game itself is live over SSE, so we
// never cache game traffic — only the static app shell.

const CACHE = "catanou-v5";

// App-shell assets, pre-cached on install. Failures are ignored per-file so a
// single missing asset can never wedge the worker install.
const SHELL = [
  "/",
  "/static/app.css",
  "/static/fonts/nunito-latin.woff2",
  "/static/icons/wood.png",
  "/static/icons/brick.png",
  "/static/icons/wheat.png",
  "/static/icons/ore.png",
  "/static/icons/sheep.png",
  "/static/htmx.min.js",
  "/static/sse.js",
  "/static/timer.js",
  "/static/trade.js",
  "/static/ui.js",
  "/static/discard.js",
  "/static/audio.js",
  "/static/pwa.js",
  "/static/branding/logo-128.webp",
  "/static/branding/landscape.webp",
  "/static/branding/potrait.webp",
  "/static/branding/icon-192.png",
  "/static/branding/icon-512.png",
];

self.addEventListener("install", (event) => {
  event.waitUntil(
    caches
      .open(CACHE)
      .then((cache) => Promise.all(SHELL.map((url) => cache.add(url).catch(() => {}))))
      .then(() => self.skipWaiting())
  );
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) => Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k))))
      .then(() => self.clients.claim())
  );
});

self.addEventListener("fetch", (event) => {
  const req = event.request;
  if (req.method !== "GET") return;

  const url = new URL(req.url);
  if (url.origin !== self.location.origin) return;
  // Live streams and game actions must always hit the network untouched.
  if (url.pathname.includes("/events")) return;

  event.respondWith(handle(req, url));
});

async function handle(req, url) {
  const cache = await caches.open(CACHE);

  if (url.pathname.startsWith("/static/")) {
    // Stale-while-revalidate: instant from cache, refreshed in the background.
    const cached = await cache.match(req);
    const network = fetch(req)
      .then((res) => {
        if (res && res.status === 200) cache.put(req, res.clone());
        return res;
      })
      .catch(() => cached);
    return cached || network;
  }

  // Pages and everything else: network first, cache as an offline fallback.
  try {
    const res = await fetch(req);
    if (req.mode === "navigate" && res && res.status === 200) {
      cache.put("/", res.clone());
    }
    return res;
  } catch (_) {
    const cached = (await cache.match(req)) || (await cache.match("/"));
    return cached || new Response("Offline", { status: 503, headers: { "Content-Type": "text/plain" } });
  }
}
