// Ferrum's bridge for an app's pages in frames (Ferrum docs/plugins.md, "Screens" and "Interfaces of
// their own"). Copy this file into your app's interface and import it:
//
//   import { ferrum } from "./ferrum.js";
//   const ctx = await ferrum.ready;                          // app, user, locale, theme; pages: path
//   const { rows } = await ferrum.api("GET", "/api/v1/Customer", { query: { limit: 20 } });
//   ferrum.title("Customers"); ferrum.started();             // a page: its title, and "I have drawn"
//   ferrum.navigate("/cards/42");                            // a page: the address bar follows it
//   ferrum.on("location", ({ path }) => route(path));        // a page: back and forward
//
// The page runs sandboxed: no session, no storage of the workspace's site, no network but its own
// files. Every call goes to the host page, which sends it to Ferrum on the person's behalf, limited
// to what the app declared. Protocol 1: the host posts {ferrum: "init", version} with a MessagePort;
// requests {id, method, params} are answered with {id, result} or {id, error: {status, message}};
// events arrive as {event, data}.

let port = null;
let next = 1;
const pending = new Map();
const listeners = new Map();
let started;
const ready = new Promise((resolve) => (started = resolve));

function call(method, params) {
  if (!port) return ready.then(() => call(method, params));
  return new Promise((resolve, reject) => {
    const id = next++;
    pending.set(id, { resolve, reject });
    port.postMessage({ id, method, params });
  });
}

/** The host's look: light or dark, and its design tokens as `--<name>` CSS variables. */
function applyTheme(theme) {
  if (!theme) return;
  const root = document.documentElement;
  root.dataset.theme = theme.mode;
  root.style.colorScheme = theme.mode;
  for (const [name, value] of Object.entries(theme.tokens ?? {})) root.style.setProperty(`--${name}`, value);
}

window.addEventListener("message", (e) => {
  if (e.source !== window.parent || e.data?.ferrum !== "init" || !e.ports[0]) return;
  port?.close();
  port = e.ports[0];
  port.onmessage = ({ data }) => {
    if (typeof data?.id === "number" && pending.has(data.id)) {
      const { resolve, reject } = pending.get(data.id);
      pending.delete(data.id);
      if (data.error) reject(Object.assign(new Error(data.error.message), { status: data.error.status }));
      else resolve(data.result);
    } else if (typeof data?.event === "string") {
      if (data.event === "theme") applyTheme(data.data);
      for (const fn of listeners.get(data.event) ?? []) fn(data.data);
    }
  };
  call("context").then((ctx) => {
    applyTheme(ctx.theme);
    started(ctx);
  });
});

// An app token for calling the API directly, kept until shortly before it expires.
let token = null;
async function bearer() {
  if (!token || Date.now() > token.until) {
    const t = await call("token");
    token = { value: t.token, until: Date.now() + (t.expires_in - 30) * 1000 };
  }
  return token.value;
}

export const ferrum = {
  /** Resolves with the context once the host has connected. */
  ready,
  /** A call to `/api/v1/…`, `/api/meta/…` or this app's `/api/ext/<app>/…`; resolves with the JSON answer. */
  api: (method, path, { query, body, ifMatch } = {}) => call("api", { method, path, query, body, ifMatch }),
  /** Pages: the same, sent straight from the page with a short-lived app token (uploads, streams). */
  async fetch(path, init = {}) {
    const headers = new Headers(init.headers);
    headers.set("authorization", `Bearer ${await bearer()}`);
    return fetch(path, { ...init, headers, credentials: "omit" });
  },
  /** Screens: a path of the web app. Pages: where the page went, so the address bar follows. */
  navigate: (to, { replace = false } = {}) => call("navigate", { to, replace }),
  /** Pages: leave for another page of the site (another app's, say). */
  open: (url) => call("open", { url }),
  openRecord: (entity, id) => call("openRecord", { entity, id }),
  /** Pages: the title the bar and the tab show. */
  title: (text) => call("title", { text }),
  /** Pages: tell the platform the page has drawn (or it offers a reload after a while). */
  started: () => call("ready"),
  toast: (message, tone = "success") => call("toast", { message, tone }),
  /** Asks the user; resolves to true when they confirm. */
  confirm: (title, { message, confirmLabel, danger } = {}) => call("confirm", { title, message, confirmLabel, danger }),
  /** Widgets and panels: the height the frame should have (by default, the page's content). */
  resize: (height = Math.ceil(document.documentElement.getBoundingClientRect().height)) => call("resize", { height }),
  /** Events: `theme` (the look changed), `record` (a panel's record was saved), `location` (pages:
   *  back or forward, with the new `path`). Returns an unsubscribe. */
  on(event, fn) {
    if (!listeners.has(event)) listeners.set(event, new Set());
    listeners.get(event).add(fn);
    return () => listeners.get(event).delete(fn);
  },
};

export default ferrum;
