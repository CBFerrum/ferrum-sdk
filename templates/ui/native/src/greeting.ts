import type { Bootstrap } from "@ferrum/client";

/** The line under the home's heading: who is signed in, in which workspace. */
export function greeting(boot: Bootstrap | null): string {
  if (!boot?.person) return "";
  return `${boot.person.name} · ${boot.workspace.name}`;
}
