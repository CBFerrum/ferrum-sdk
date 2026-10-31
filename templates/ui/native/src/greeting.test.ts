import { describe, expect, it } from "vitest";
import { fakeBootstrap } from "@ferrum/testing";
import { greeting } from "./greeting";

describe("the home's greeting", () => {
  it("names the person and the workspace from the bootstrap", () => {
    expect(greeting(fakeBootstrap())).toBe("Tess Ter · Test");
  });

  it("is empty with nobody signed in", () => {
    expect(greeting(fakeBootstrap({ person: null }))).toBe("");
    expect(greeting(null)).toBe("");
  });
});
