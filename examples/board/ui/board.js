import { ferrum } from "./ferrum.js";

const list = document.querySelector("#cards");
const title = document.querySelector("#title");

async function load() {
  const { rows } = await ferrum.api("GET", "/api/v1/BdCard", { query: { limit: 50, fields: "id,title,column" } });
  list.replaceChildren(
    ...rows.map((card) => {
      const li = document.createElement("li");
      const open = document.createElement("button");
      open.type = "button";
      open.className = "link";
      open.textContent = card.title;
      open.onclick = () => ferrum.openRecord("BdCard", card.id);
      li.append(open, ` · ${card.column ?? ""}`);
      return li;
    }),
  );
  if (!rows.length) list.innerHTML = '<li class="muted">No cards yet.</li>';
}

// A sandboxed page cannot submit forms: a button and the Enter key do it.
async function add() {
  const t = title.value.trim();
  if (!t) return;
  try {
    await ferrum.api("POST", "/api/v1/BdCard", { body: { title: t } });
    title.value = "";
    ferrum.toast(`Added ${t}`);
    load();
  } catch (e) {
    ferrum.toast(e.message, "error");
  }
}
document.querySelector("#add").onclick = add;
title.addEventListener("keydown", (e) => e.key === "Enter" && add());

const ctx = await ferrum.ready;
document.querySelector("#who").textContent = `Signed in as ${ctx.user.name || ctx.user.email}.`;
// What the app did not declare stays out of its reach, whoever the user is.
ferrum.api("GET", "/api/v1/Customer", { query: { limit: 1 } }).then(
  () => (document.querySelector("#note").textContent = "Customers are readable here."),
  (e) => (document.querySelector("#note").textContent = `Customers are out of this app's reach (${e.status}).`),
);
load();
