// Customer Cards: the workspace's customers as cards, searched as you type, each opening where
// the workspace shows customers. Everything goes through the bridge (ferrum.js).
import { ferrum } from "./ferrum.js";

const list = document.querySelector("#cards");
const q = document.querySelector("#q");

function card(c) {
  const li = document.createElement("li");
  const b = document.createElement("button");
  b.type = "button";
  b.className = "card";
  const name = document.createElement("strong");
  name.textContent = c.customer_name ?? "Customer";
  const more = document.createElement("span");
  more.className = "muted";
  more.textContent = [c.email, c.customer_group && c._titles?.customer_group].filter(Boolean).join(" · ") || "—";
  b.append(name, more);
  b.addEventListener("click", () => ferrum.openRecord("Customer", c.id));
  li.append(b);
  return li;
}

async function load(search) {
  try {
    const query = { limit: 60, fields: "customer_name,email,customer_group", titles: "true", sort: "customer_name asc" };
    if (search) query.search = search;
    const { rows } = await ferrum.api("GET", "/api/v1/Customer", { query });
    if (!rows.length) {
      const li = document.createElement("li");
      li.className = "muted";
      li.textContent = search ? `No customer matches “${search}”.` : "No customers yet.";
      list.replaceChildren(li);
    } else {
      list.replaceChildren(...rows.map(card));
    }
  } catch (e) {
    const li = document.createElement("li");
    li.className = "muted";
    li.textContent = `Customers cannot be shown: ${e.message}`;
    list.replaceChildren(li);
  }
}

/** What the page can and cannot do, checked live: the isolation an administrator agreed to. */
async function checks() {
  const out = document.querySelector("#checks");
  const show = (ok, text) => {
    const li = document.createElement("li");
    li.className = ok ? "ok" : "bad";
    li.textContent = text;
    out.append(li);
  };
  try {
    show(document.cookie === "", "No cookie of the workspace's site reaches this page.");
  } catch {
    show(true, "No cookie of the workspace's site can be read here (an origin of its own).");
  }
  // In a sandboxed frame the page has no origin; on an address of its own (when the server gives
  // apps one), its storage is that origin's, never the workspace's.
  if (self.origin === "null") {
    try {
      localStorage.getItem("x");
      show(false, "This page can use the site's storage.");
    } catch {
      show(true, "The site's storage is out of reach (an origin of its own).");
    }
  } else {
    let workspace = "";
    try {
      workspace = JSON.parse(document.getElementById("ferrum-bootstrap")?.textContent ?? "{}").workspace?.origin ?? "";
    } catch {}
    show(self.origin !== workspace, `Its storage is its own origin's (${self.origin}), not the workspace's.`);
  }
  const refused = async (label, run) => {
    try {
      await run();
      show(false, `${label}: allowed.`);
    } catch (e) {
      show(true, `${label}: refused (${e.status ?? e.name}).`);
    }
  };
  await refused("Reading invoices, which this app did not ask for", () => ferrum.api("GET", "/api/v1/SalesInvoice", { query: { limit: 1 } }));
  await refused("Adding a customer (it may only read them)", () => ferrum.api("POST", "/api/v1/Customer", { body: { customer_name: "Not allowed" } }));
  await refused("The workspace's administration", () => ferrum.api("GET", "/api/admin/users"));
  try {
    const r = await ferrum.fetch("/api/v1/Customer?limit=1&fields=customer_name");
    show(r.ok, `With its own short-lived token, straight to the API: ${r.status}.`);
  } catch (e) {
    show(false, `Its own token: ${e.message}.`);
  }
  try {
    const r = await fetch("/api/v1/Customer?limit=1", { credentials: "include" });
    show(r.status === 401, `Without a token, the API answers ${r.status}.`);
  } catch {
    show(true, "Without a token, the API refuses to answer this page.");
  }
  try {
    const r = await fetch("/api/admin/users");
    show(!r.ok, `The rest of the API is out of reach (${r.status}).`);
  } catch {
    show(true, "The rest of the API is out of reach (its page's policy).");
  }
}

// Before waiting for the host, so opening the panel early still runs the checks (they wait too).
document.querySelector("#apart").addEventListener("toggle", (e) => {
  if (e.target.open && !document.querySelector("#checks").children.length) checks();
});

const ctx = await ferrum.ready;
document.querySelector("#who").textContent = `For ${ctx.user.name || ctx.user.email}${ctx.workspace ? ` at ${ctx.workspace}` : ""}`;
ferrum.title("Customer cards");
const start = new URLSearchParams((ctx.path ?? "/").split("?")[1] ?? "").get("q") ?? "";
q.value = start;
await load(start);
ferrum.started();

let timer;
q.addEventListener("input", () => {
  clearTimeout(timer);
  timer = setTimeout(() => {
    const v = q.value.trim();
    ferrum.navigate(v ? `/?q=${encodeURIComponent(v)}` : "/", { replace: true });
    load(v);
  }, 250);
});
ferrum.on("location", ({ path }) => {
  const v = new URLSearchParams(path.split("?")[1] ?? "").get("q") ?? "";
  q.value = v;
  load(v);
});
