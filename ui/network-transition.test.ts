import { describe, expect, it } from "vitest";
import type { NetworkExit, NetworkTransition } from "./globe";
import { shouldPresentTransition, transitionCopy } from "./network-transition";

function exit(ip: string, city: string): NetworkExit {
  return {
    ip,
    city,
    country: "US",
    lat: null,
    lon: null,
    asn: null,
    organization: null,
    source: "hosted-egress",
    confidence: null,
    ageSeconds: 0,
  };
}

function transition(overrides: Partial<NetworkTransition> = {}): NetworkTransition {
  return {
    id: 2,
    status: "ready",
    ageSeconds: 1,
    previousExit: exit("1.1.1.1", "Seattle"),
    currentExit: exit("2.2.2.2", "Portland"),
    ...overrides,
  };
}

describe("network transition presentation", () => {
  it("describes a changed public exit", () => {
    expect(transitionCopy(transition()).title).toBe("Seattle, US → Portland, US");
  });

  it("distinguishes an unchanged exit", () => {
    const same = exit("1.1.1.1", "Seattle");
    expect(transitionCopy(transition({ currentExit: same })).title).toBe(
      "Exit still appears in Seattle, US",
    );
  });

  it("does not resurrect expired or older transitions", () => {
    expect(shouldPresentTransition(transition({ ageSeconds: 91 }), 0)).toBe(false);
    expect(shouldPresentTransition(transition({ id: 1 }), 2)).toBe(false);
    expect(shouldPresentTransition(transition(), 2)).toBe(true);
  });
});
