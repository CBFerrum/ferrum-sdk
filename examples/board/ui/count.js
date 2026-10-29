import { ferrum } from "./ferrum.js";

await ferrum.ready;
const { rows } = await ferrum.api("GET", "/api/v1/BdCard", { query: { limit: 500, fields: "id,column" } });
const n = (column) => rows.filter((r) => r.column === column).length;
document.querySelector("#total").textContent = `${rows.length} cards`;
document.querySelector("#split").textContent = `${n("To do")} to do · ${n("Doing")} doing · ${n("Done")} done`;
