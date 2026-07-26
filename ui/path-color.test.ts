import { describe, expect, it } from "vitest";
import { colorForKey } from "./path-color";

describe("colorForKey", () => {
  it("keeps application colors stable", () => {
    expect(colorForKey("app-group:firefox")).toBe("#d88273");
    expect(colorForKey("app-group:spotify")).toBe("#789e91");
    expect(colorForKey("app-group:firefox")).toBe(colorForKey("app-group:firefox"));
  });
});
