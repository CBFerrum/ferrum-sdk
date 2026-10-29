// Ferrum screens: the bridge between a page of an app's ui/ and the Ferrum web app that frames it
// (docs/plugins.md in Ferrum, "Screens"). Copy this file into your app's ui/ and import it:
//
//   import { ferrum } from "./ferrum.js";
//   const ctx = await ferrum.ready;                         // app, slot, user, locale, theme, record (panels)
//   const { rows } = await ferrum.api("GET", "/api/v1/Customer", { query: { limit: 20 } });
//   ferrum.on("record", (r) => reload(r.id));                // panels: the record was saved
//   ferrum.toast("Saved"); ferrum.resize();                  // widgets and panels size to their content
//
// The page runs sandboxed with no network of its own: every call goes to the host, which sends it
// to Ferrum on the user's behalf, limited to what the app declared. Protocol 1: the host posts
// {ferrum: "init", version} with a MessagePort; requests {id, method, params} are answered with
// {id, result} or {id, error: {status, message}}; events arrive as {event, data}.

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

export const ferrum = {
  /** Resolves with the context once the host has connected. */
  ready,
  /** A call to `/api/v1/…`, `/api/meta/…` or this app's `/api/ext/<app>/…`; resolves with the JSON answer. */
  api: (method, path, { query, body } = {}) => call("api", { method, path, query, body }),
  navigate: (to) => call("navigate", { to }),
  openRecord: (entity, id) => call("openRecord", { entity, id }),
  toast: (message, tone = "success") => call("toast", { message, tone }),
  /** Asks the user; resolves to true when they confirm. */
  confirm: (title, { message, confirmLabel, danger } = {}) => call("confirm", { title, message, confirmLabel, danger }),
  /** Widgets and panels: the height the frame should have (by default, the page's content). */
  resize: (height = Math.ceil(document.documentElement.getBoundingClientRect().height)) => call("resize", { height }),
  /** Events: `theme` (the look changed), `record` (a panel's record was saved). Returns an unsubscribe. */
  on(event, fn) {
    if (!listeners.has(event)) listeners.set(event, new Set());
    listeners.get(event).add(fn);
    return () => listeners.get(event).delete(fn);
  },
};

export default ferrum;
