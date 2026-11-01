/* Public static shell only. Never a session, data cache, mutation queue or authority. */
const CACHE = "__CACHE_NAME__";
const SHELL = __SHELL_PATHS__;
const isShell = (request) => {
  const url = new URL(request.url);
  return (
    request.method === "GET" &&
    url.origin === self.location.origin &&
    !url.search &&
    SHELL.includes(url.pathname) &&
    !request.headers.has("authorization")
  );
};
const cacheable = (response) =>
  response.ok &&
  response.type === "basic" &&
  response.headers.get("x-rolvya-static") === "public-shell-v1" &&
  !/private|no-store/i.test(response.headers.get("cache-control") || "");
const coherent = async (path, response) => {
  if (path !== "/" && path !== "/index.html") return true;
  const html = await response.clone().text();
  if (html.length > 40000) return false;
  const scripts = [...html.matchAll(/<script\b[^>]*\bsrc="([^"]+)"/gi)];
  return (
    scripts.length === 1 &&
    SHELL.includes(scripts[0][1]) &&
    scripts[0][1].endsWith(".js")
  );
};
self.addEventListener("install", (event) =>
  event.waitUntil(
    (async () => {
      const cache = await caches.open(CACHE);
      for (const path of SHELL) {
        const response = await fetch(
          new Request(path, { credentials: "omit", cache: "reload" }),
        );
        if (!cacheable(response) || !(await coherent(path, response)))
          throw Error("Static shell unavailable");
        await cache.put(path, response);
      }
    })(),
  ),
);
self.addEventListener("activate", (event) =>
  event.waitUntil(
    (async () => {
      for (const name of await caches.keys())
        if (name.startsWith("rolvya-shell-v1-") && name !== CACHE)
          await caches.delete(name);
      await self.clients.claim();
    })(),
  ),
);
self.addEventListener("message", (event) => {
  if (event.origin !== self.location.origin) return;
  if (
    event.data?.type === "ACTIVATE" &&
    event.source?.url &&
    new URL(event.source.url).origin === self.location.origin
  )
    event.waitUntil(self.skipWaiting());
});
self.addEventListener("fetch", (event) => {
  if (!isShell(event.request)) return;
  event.respondWith(
    (async () => {
      const url = new URL(event.request.url);
      const cache = await caches.open(CACHE);
      // HTML is network-first; hashed JS is immutable. Cache writes require the
      // server's explicit public marker, and network fetches omit all credentials.
      if (url.pathname.endsWith(".js")) {
        const hit = await cache.match(url.pathname);
        if (hit) return hit;
      }
      try {
        const response = await fetch(
          new Request(event.request, {
            credentials: "omit",
            cache: "no-cache",
          }),
        );
        if (cacheable(response) && (await coherent(url.pathname, response)))
          await cache.put(url.pathname, response.clone());
        else if (url.pathname === "/" || url.pathname === "/index.html") {
          const previous = await cache.match(url.pathname);
          return (
            previous ||
            new Response("Static shell update pending", { status: 503 })
          );
        }
        return response;
      } catch (error) {
        const hit = await cache.match(url.pathname);
        if (hit) return hit;
        throw error;
      }
    })(),
  );
});
