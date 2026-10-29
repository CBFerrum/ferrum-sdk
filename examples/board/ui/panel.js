import { ferrum } from "./ferrum.js";

const ctx = await ferrum.ready;
// A panel gets the record's entity and id; its data comes through the API, within the app's reach.
async function show() {
  const card = await ferrum.api("GET", `/api/v1/BdCard/${encodeURIComponent(ctx.record.id)}`);
  document.querySelector("#name").textContent = card.title;
  document.querySelector("#column").textContent = card.column ?? "no column";
  ferrum.resize();
}
ferrum.on("record", show);
document.querySelector("#open").onclick = () => ferrum.navigate("/x/board/board");
show();
