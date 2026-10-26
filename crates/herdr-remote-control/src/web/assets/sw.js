// Herdr Remote service worker: keeps the app shell for offline starts (so
// the app opens on any network, then finds the computer at another
// address), and shows push notifications. It never caches chat content.
"use strict";

const SHELL = "herdr-shell-v3";
const FILES = [
  "/",
  "/assets/app.css",
  "/assets/app.js",
  "/assets/alpine.min.js",
  "/assets/icon.svg",
  "/assets/icon-180.png",
  "/assets/icon-192.png",
  // The scanner that adds a computer, so it opens wherever the app does.
  "/assets/jsqr.min.js",
  "/manifest.webmanifest",
];
// Not `/pair`: a scan must reach the computer itself. When this phone no
// longer trusts its certificate, a cached page would hide Safari's warning
// and leave pairing failing with nothing to accept.
const ROUTES = ["/", "/board", "/computers", "/new", "/new-chat", "/settings"];

self.addEventListener("install", (event) => {
  event.waitUntil(caches.open(SHELL).then((cache) => cache.addAll(FILES)).then(() => self.skipWaiting()));
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) => Promise.all(keys.filter((key) => key !== SHELL).map((key) => caches.delete(key))))
      .then(() => self.clients.claim()),
  );
});

// The shell and its files: network first, so a new release shows at once,
// and the cached copy when the computer cannot be reached.
self.addEventListener("fetch", (event) => {
  const url = new URL(event.request.url);
  if (event.request.method !== "GET" || url.origin !== self.location.origin) return;
  const isShell = ROUTES.includes(url.pathname) || url.pathname.startsWith("/chat/");
  const isFile = FILES.includes(url.pathname);
  if (!isShell && !isFile) return;
  const key = isShell ? "/" : url.pathname;
  event.respondWith(
    fetch(event.request)
      .then((response) => {
        if (response.ok) {
          const copy = response.clone();
          caches.open(SHELL).then((cache) => cache.put(key, copy));
        }
        return response;
      })
      .catch(() => caches.match(key)),
  );
});

async function visibleClient(url) {
  const windows = await self.clients.matchAll({ type: "window", includeUncontrolled: true });
  return windows.find((client) => client.visibilityState === "visible" && new URL(client.url).pathname === url);
}

// `push::Message`: { kind, title, body, tag, url, clear, badge }.
self.addEventListener("push", (event) => {
  let message;
  try {
    message = event.data.json();
  } catch {
    return;
  }
  event.waitUntil(
    (async () => {
      if (typeof message.badge === "number") {
        await (message.badge ? self.navigator.setAppBadge?.(message.badge) : self.navigator.clearAppBadge?.());
      }
      if (message.clear) {
        const shown = await self.registration.getNotifications({ tag: message.tag });
        shown.forEach((notification) => notification.close());
        return;
      }
      // Nothing while that chat is open in front of the user. iOS requires
      // every push to show something, so only skip where that is allowed.
      if ((await visibleClient(message.url)) && !/iPhone|iPad/.test(self.navigator.userAgent)) return;
      await self.registration.showNotification(message.title, {
        body: message.body,
        tag: message.tag,
        renotify: message.kind === "question",
        data: { url: message.url },
        icon: "/assets/icon-192.png",
        badge: "/assets/icon-192.png",
      });
    })(),
  );
});

self.addEventListener("notificationclick", (event) => {
  event.notification.close();
  const url = event.notification.data?.url || "/";
  event.waitUntil(
    (async () => {
      const windows = await self.clients.matchAll({ type: "window", includeUncontrolled: true });
      const open = windows[0];
      if (open) {
        await open.focus();
        open.postMessage({ navigate: url });
        return;
      }
      await self.clients.openWindow(url);
    })(),
  );
});
