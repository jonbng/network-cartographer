import { describe, expect, it } from "vitest";
import { INTRO_LOCK_MS, introStatus } from "./intro-state";

const base = {
  appCount: 1,
  destCount: 2,
  tracesEnabled: true,
  queued: 0,
  running: 0,
  done: 0,
  failed: 0,
  mappedRoutes: 0,
};

describe("introStatus", () => {
  it("uses honest startup and no-traffic copy", () => {
    expect(introStatus(null).title).toBe("Starting the local monitor");
    expect(introStatus({ ...base, appCount: 0, destCount: 0 }).emptyDetail).toContain("continent");
  });

  it("reports real traceroute work without synthetic percentages", () => {
    const state = introStatus({ ...base, queued: 2, running: 1 });
    expect(state.title).toBe("Mapping 3 routes");
    expect(state.detail).toContain("few seconds");
  });

  it("explains disabled, failed, and completed mapping states", () => {
    expect(introStatus({ ...base, tracesEnabled: false }).emptyTitle).toBe("Route mapping is off");
    expect(introStatus({ ...base, failed: 2 }).emptyTitle).toBe("No route replies yet");
    expect(introStatus({ ...base, mappedRoutes: 1 }).title).toBe("1 route ready");
  });

  it("locks the introduction for two seconds", () => {
    expect(INTRO_LOCK_MS).toBe(2_000);
  });
});
