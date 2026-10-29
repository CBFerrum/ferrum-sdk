import { ferrum } from "./ferrum.js";

const ctx = await ferrum.ready;
const shown = { app: ctx.app, slot: ctx.slot, user: ctx.user, locale: ctx.locale, theme: ctx.theme.mode };
document.querySelector("#ctx").textContent = JSON.stringify(shown, null, 2);
document.querySelector("#hello").onclick = () => ferrum.toast(`Hello from ${ctx.app}`, "info");
