import { describe, expect, it } from "vitest";
import { mergeVisibleSettings } from "./settings-state";

describe("mergeVisibleSettings", () => {
  it("preserves settings hidden from the interface", () => {
    const result = mergeVisibleSettings(
      { historyEnabled: true, includeUdp: true, tracesEnabled: true },
      { includeUdp: false },
    );
    expect(result).toEqual({ historyEnabled: true, includeUdp: false, tracesEnabled: true });
  });
});
