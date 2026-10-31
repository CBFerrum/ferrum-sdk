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
//
// On an origin of its own (`{app}--{workspace}.{apps base}`, when the server gives apps one) and
// opened outside the workspace's page, there is no host: the page signs in with OAuth (code and
// PKCE, the server's #ferrum-bootstrap says where) and calls the API itself with short-lived app
// tokens, renewed while it is open. An app the server trusts (installed from a store) is a page of
// the workspace itself, under the bar: its calls go to the API with the person's session, and the
// bar takes its title and its start. The same calls work all three ways.

function readBootstrap() {
  try {
    return JSON.parse(document.getElementById("ferrum-bootstrap")?.textContent ?? "null");
  } catch {
    return null;
  }
}

const boot = readBootstrap();
// Opened on its own origin, outside the workspace's page.
const standalone = window.parent === window && !!boot?.oauth;
// A page of the workspace itself (an app the server trusts): the session's and the bar's.
const inPage = window.parent === window && !boot?.oauth && !!boot?.person;
/** The bar's shell on a page of the workspace (`globalThis.ferrum.shell`, set by its runtime). */
const bar = () => globalThis.ferrum?.shell;

let port = null;
let next = 1;
const pending = new Map();
const listeners = new Map();
let started;
const ready = new Promise((resolve) => (started = resolve));

function call(method, params) {
  if (standalone || inPage) return local(method, params ?? {});
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
  if (standalone) return (await signedIn()).access;
  if (!token || Date.now() > token.until) {
    const t = await call("token");
    token = { value: t.token, until: Date.now() + (t.expires_in - 30) * 1000 };
  }
  return token.value;
}

// ───────────── on its own origin: OAuth with PKCE, and the calls made here ─────────────

const SESSION = "ferrum.app.session";
const PENDING = "ferrum.app.pkce";
const keep = {
  get(k) {
    try {
      return JSON.parse(sessionStorage.getItem(k) ?? "null");
    } catch {
      return null;
    }
  },
  set(k, v) {
    try {
      if (v == null) sessionStorage.removeItem(k);
      else sessionStorage.setItem(k, JSON.stringify(v));
    } catch {}
  },
};
const b64url = (bytes) => btoa(String.fromCharCode(...new Uint8Array(bytes))).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
const randomText = (n) => b64url(crypto.getRandomValues(new Uint8Array(n)));

async function tokens(form) {
  const r = await fetch(boot.oauth.token, {
    method: "POST",
    headers: { "content-type": "application/x-www-form-urlencoded", accept: "application/json" },
    body: new URLSearchParams({ client_id: boot.oauth.client_id, ...form }),
    credentials: "omit",
  });
  const t = await r.json().catch(() => null);
  if (!r.ok) throw Object.assign(new Error(t?.error_description ?? t?.message ?? "signing in failed"), { status: r.status });
  const s = { access: t.access_token, refresh: t.refresh_token, until: Date.now() + (t.expires_in - 30) * 1000, user: t.user ?? keep.get(SESSION)?.user ?? {} };
  keep.set(SESSION, s);
  return s;
}

/** To the workspace to sign in (and allow this app), and back to where the page was. */
async function signIn() {
  const verifier = randomText(48);
  const challenge = b64url(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(verifier)));
  const state = randomText(18);
  keep.set(PENDING, { verifier, state, back: location.pathname + location.search });
  const q = new URLSearchParams({
    response_type: "code",
    client_id: boot.oauth.client_id,
    redirect_uri: boot.oauth.redirect_uri,
    code_challenge: challenge,
    code_challenge_method: "S256",
    state,
  });
  location.assign(`${boot.oauth.authorize}?${q}`);
  return new Promise(() => {});
}

/** A session that works now: the one kept, renewed, or a new one. One renewal at a time: a refresh
 *  token works once, and presenting it twice ends the session. */
let renewing = null;
function signedIn() {
  const s = keep.get(SESSION);
  if (s && Date.now() < s.until) return Promise.resolve(s);
  renewing ??= (async () => {
    if (s?.refresh) {
      try {
        return await tokens({ grant_type: "refresh_token", refresh_token: s.refresh });
      } catch {
        keep.set(SESSION, null);
      }
    }
    return signIn();
  })().finally(() => (renewing = null));
  return renewing;
}

/** The app token stopped working (signed out everywhere, say): the next call renews it. */
function expire() {
  token = null;
  const s = keep.get(SESSION);
  if (s) keep.set(SESSION, { ...s, until: 0 });
}

async function local(method, p) {
  const ws = boot.workspace?.origin ?? "";
  switch (method) {
    case "api": {
      const q = new URLSearchParams();
      for (const [k, v] of Object.entries(p.query ?? {})) q.append(k, String(v));
      const headers = { accept: "application/json" };
      if (p.ifMatch != null) headers["if-match"] = `"${p.ifMatch}"`;
      let body;
      if (p.body !== undefined && !["GET", "DELETE"].includes((p.method ?? "GET").toUpperCase())) {
        headers["content-type"] = "application/json";
        body = JSON.stringify(p.body);
      }
      const url = `${p.path}${q.toString() ? `?${q}` : ""}`;
      const send = async () =>
        inPage
          ? fetch(url, { method: p.method ?? "GET", headers, body, credentials: "same-origin" })
          : fetch(url, { method: p.method ?? "GET", headers: { ...headers, authorization: `Bearer ${await bearer()}` }, body, credentials: "omit" });
      let r = await send();
      // Refused before it did anything: once more with a renewed token (or after signing in again);
      // on a page of the workspace, the bar signs the person in again.
      if (r.status === 401 && inPage) bar()?.session?.reauthenticate?.();
      else if (r.status === 401) {
        expire();
        r = await send();
      }
      const data = r.status === 204 ? null : await r.json().catch(() => null);
      if (!r.ok) throw Object.assign(new Error(data?.fields?.[0]?.message ?? data?.message ?? `the request failed (${r.status})`), { status: r.status });
      return data;
    }
    case "token": {
      const s = await signedIn();
      return { token: s.access, expires_in: Math.max(1, Math.round((s.until - Date.now()) / 1000)) };
    }
    case "navigate":
      if (p.replace) history.replaceState(null, "", p.to);
      else history.pushState(null, "", p.to);
      return null;
    case "open":
      location.assign(`${ws}${p.url}`);
      return null;
    case "openRecord":
      location.assign(`${ws}/open/${encodeURIComponent(p.entity)}/${encodeURIComponent(p.id)}`);
      return null;
    case "title":
      document.title = p.text || document.title;
      if (inPage) bar()?.title?.(p.text);
      return null;
    case "toast":
      if (inPage && bar()?.toast) bar().toast(p.message, { tone: p.tone === "error" ? "error" : "info" });
      else console.info(p.message);
      return null;
    case "ready":
      if (inPage) bar()?.ready?.();
      return null;
    case "confirm":
      return window.confirm(p.message ? `${p.title}\n\n${p.message}` : p.title);
    default:
      return null;
  }
}

if (inPage) {
  started({
    app: boot.app?.id,
    slot: "page",
    path: location.pathname + location.search,
    user: { id: boot.person.id, name: boot.person.name, email: boot.person.email },
    workspace: boot.workspace?.name ?? "",
    locale: boot.locale?.language ?? navigator.language,
    theme: null,
  });
  window.addEventListener("popstate", () => {
    for (const fn of listeners.get("location") ?? []) fn({ path: location.pathname + location.search });
  });
}

if (standalone) {
  (async () => {
    // Back from signing in: the code for tokens, then where the page was.
    if (location.pathname === "/_ferrum/callback") {
      const q = new URLSearchParams(location.search);
      const pending = keep.get(PENDING);
      keep.set(PENDING, null);
      const failed = (text) => {
        const again = document.createElement("a");
        again.href = pending?.back || "/";
        again.textContent = "Try again";
        const line = document.createElement("p");
        line.append(`${text} `, again);
        document.body.replaceChildren(line);
      };
      if (!q.get("code") || !pending || q.get("state") !== pending.state) {
        failed(q.get("error") === "access_denied" ? "You did not allow this app." : "Signing in did not work.");
        return;
      }
      try {
        await tokens({ grant_type: "authorization_code", code: q.get("code"), redirect_uri: boot.oauth.redirect_uri, code_verifier: pending.verifier });
      } catch (e) {
        failed(`Signing in did not work: ${e.message}.`);
        return;
      }
      history.replaceState(null, "", pending.back || "/");
    }
    const s = await signedIn();
    started({
      app: boot.app.id,
      slot: "page",
      standalone: true,
      path: location.pathname + location.search,
      user: s.user ?? {},
      workspace: boot.workspace?.name ?? "",
      locale: navigator.language,
      theme: null,
    });
  })();
  window.addEventListener("popstate", () => {
    for (const fn of listeners.get("location") ?? []) fn({ path: location.pathname + location.search });
  });
}

export const ferrum = {
  /** Resolves with the context once the host has connected. */
  ready,
  /** A call to `/api/v1/…`, `/api/meta/…` or this app's `/api/ext/<app>/…`; resolves with the JSON answer. */
  api: (method, path, { query, body, ifMatch } = {}) => call("api", { method, path, query, body, ifMatch }),
  /** Pages: the same, sent straight from the page with a short-lived app token (uploads, streams). */
  async fetch(path, init = {}) {
    if (inPage) return fetch(path, { ...init, credentials: "same-origin" });
    const headers = new Headers(init.headers);
    headers.set("authorization", `Bearer ${await bearer()}`);
    const r = await fetch(path, { ...init, headers, credentials: "omit" });
    if (r.status === 401) expire();
    return r;
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
