// {{name}}'s page: the team's notes, newest first, with one to add and each to tick off. It runs
// isolated (Ferrum docs/plugins.md, "Interfaces of their own"): everything goes through the bridge
// (ferrum.js), within the person's rights and what this app declared.
import { ferrum } from "./ferrum.js";

const list = document.querySelector("#notes");
const form = document.querySelector("#add");
const error = document.querySelector("#error");
const ENTITY = "{{entity}}";

function say(message) {
  error.textContent = message;
}

function note(n) {
  const li = document.createElement("li");
  const done = document.createElement("input");
  done.type = "checkbox";
  done.checked = n.status === "Done";
  done.setAttribute("aria-label", `Done: ${n.title}`);
  done.addEventListener("change", async () => {
    try {
      await ferrum.api("PATCH", `/api/v1/${ENTITY}/${encodeURIComponent(n.id)}`, { body: { status: done.checked ? "Done" : "Open" } });
    } catch (e) {
      done.checked = !done.checked;
      say(e.message);
    }
  });
  const title = document.createElement("button");
  title.type = "button";
  title.className = "title";
  title.textContent = n.title;
  title.addEventListener("click", () => ferrum.openRecord(ENTITY, n.id));
  li.append(done, title);
  return li;
}

async function load() {
  try {
    const { rows } = await ferrum.api("GET", `/api/v1/${ENTITY}`, { query: { limit: 100, fields: "title,status", sort: "created_at desc" } });
    if (!rows.length) {
      const li = document.createElement("li");
      li.className = "muted";
      li.textContent = "No notes yet: add the first one above.";
      list.replaceChildren(li);
    } else {
      list.replaceChildren(...rows.map(note));
    }
  } catch (e) {
    say(`The notes cannot be shown: ${e.message}`);
  }
}

form.addEventListener("submit", async (e) => {
  e.preventDefault();
  say("");
  const title = form.elements.title.value.trim();
  if (!title) return;
  try {
    await ferrum.api("POST", `/api/v1/${ENTITY}`, { body: { title } });
    form.reset();
    await load();
  } catch (err) {
    say(err.message);
  }
});

const ctx = await ferrum.ready;
document.querySelector("#who").textContent = ctx.user?.name ? `Signed in as ${ctx.user.name}` : "";
ferrum.title("{{name}}");
await load();
ferrum.started();
