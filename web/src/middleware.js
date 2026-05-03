import { defineMiddleware } from "astro:middleware";

const ALLOWED_HOSTS = new Set(["amonhen.legit.place"]);

export const onRequest = defineMiddleware((context, next) => {
  const host = context.request.headers.get("host")?.split(":")[0]?.toLowerCase();

  if (host && !ALLOWED_HOSTS.has(host)) {
    return new Response("Not found", {
      status: 404,
      headers: {
        "content-type": "text/plain; charset=utf-8",
        "x-robots-tag": "noindex",
      },
    });
  }

  return next();
});
