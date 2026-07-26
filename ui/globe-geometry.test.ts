import { describe, expect, it } from "vitest";
import {
  ambientMotionAllowed,
  ambientRouteCandidates,
  buildPulsePathPoints,
  chooseAmbientRoute,
  classifySegment,
  gapShouldAnimate,
  segmentVisualState,
  selectionAllowsMotion,
  type GeoHop,
} from "./globe-geometry";

function hop(
  ttl: number,
  overrides: Partial<GeoHop> = {},
): GeoHop {
  return {
    ttl,
    addr: `203.0.113.${ttl}`,
    lat: ttl * 4,
    lon: ttl * 7,
    ...overrides,
  };
}

describe("classifySegment", () => {
  it("marks consecutive mapped hops as observed", () => {
    expect(classifySegment([hop(4), hop(5)], 4, 5)).toEqual({
      kind: "observed",
      missingResponses: 0,
      unlocatedHops: 0,
    });
  });

  it("separates missing responses from unavailable locations", () => {
    const result = classifySegment(
      [
        hop(4),
        hop(5, { addr: null, lat: null, lon: null }),
        hop(6, { lat: null, lon: null }),
        hop(7),
      ],
      4,
      7,
    );
    expect(result).toEqual({
      kind: "unmapped",
      missingResponses: 1,
      unlocatedHops: 1,
    });
  });

  it("counts an absent TTL as a missing response", () => {
    expect(classifySegment([hop(2), hop(4)], 2, 4).missingResponses).toBe(1);
  });
});

describe("buildPulsePathPoints", () => {
  it("orders hops by TTL and creates an elevated great-circle path", () => {
    const points = buildPulsePathPoints([
      hop(9, { lat: 40, lon: 100 }),
      hop(3, { lat: 10, lon: 20 }),
    ]);
    expect(points.length).toBeGreaterThan(8);
    expect(points[0]).toMatchObject({ lat: 10, lng: 20 });
    expect(points.at(-1)).toMatchObject({ lat: 40, lng: 100 });
    expect(Math.max(...points.map((point) => point.altitude))).toBeGreaterThan(
      points[0].altitude,
    );
  });

  it("takes the short route across the antimeridian", () => {
    const points = buildPulsePathPoints([
      hop(1, { lat: 5, lon: 170 }),
      hop(2, { lat: 5, lon: -170 }),
    ]);
    expect(points.every((point) => Math.abs(point.lng) > 160)).toBe(true);
  });

  it("returns no preview for fewer than two mapped hops", () => {
    expect(buildPulsePathPoints([hop(1)])).toEqual([]);
  });
});

describe("interaction state", () => {
  it("gives dimming precedence over transient emphasis", () => {
    expect(segmentVisualState(true, true, true)).toBe("dimmed");
    expect(segmentVisualState(false, true, false)).toBe("emphasized");
    expect(segmentVisualState(false, false, true)).toBe("emphasized");
    expect(segmentVisualState(false, false, false)).toBe("normal");
  });

  it("disables motion for reduced-motion and keyboard selections", () => {
    expect(selectionAllowsMotion(false, false)).toBe(true);
    expect(selectionAllowsMotion(true, false)).toBe(false);
    expect(selectionAllowsMotion(false, true)).toBe(false);
  });

  it("animates only unresolved spans from running traces", () => {
    expect(gapShouldAnimate("unmapped", "running", false)).toBe(true);
    expect(gapShouldAnimate("observed", "running", false)).toBe(false);
    expect(gapShouldAnimate("unmapped", "queued", false)).toBe(false);
    expect(gapShouldAnimate("unmapped", "done", false)).toBe(false);
    expect(gapShouldAnimate("unmapped", "running", true)).toBe(false);
  });

  it("pauses ambient motion for hidden tabs, camera gestures, and reduced motion", () => {
    expect(ambientMotionAllowed(false, false, false)).toBe(true);
    expect(ambientMotionAllowed(true, false, false)).toBe(false);
    expect(ambientMotionAllowed(false, true, false)).toBe(false);
    expect(ambientMotionAllowed(false, false, true)).toBe(false);
  });
});

describe("ambient route choice", () => {
  const routes = [
    { id: "a", appId: "browser", mappedHopCount: 4 },
    { id: "b", appId: "browser", mappedHopCount: 3 },
    { id: "c", appId: "music", mappedHopCount: 1 },
    { id: "d", appId: "music", mappedHopCount: 5 },
  ];

  it("uses only visible routes with enough mapped geometry", () => {
    expect(ambientRouteCandidates(routes, "a", new Set()).map((route) => route.id)).toEqual(["a"]);
    expect(
      ambientRouteCandidates(routes, null, new Set(["music"])).map((route) => route.id),
    ).toEqual(["d"]);
    expect(ambientRouteCandidates(routes, null, new Set()).map((route) => route.id)).toEqual([
      "a",
      "b",
      "d",
    ]);
  });

  it("avoids immediately repeating a route when alternatives exist", () => {
    expect(chooseAmbientRoute(routes.slice(0, 2), "a", 0)?.id).toBe("b");
    expect(chooseAmbientRoute([routes[0]], "a", 0)?.id).toBe("a");
    expect(chooseAmbientRoute([], "a", 0)).toBeNull();
  });
});
