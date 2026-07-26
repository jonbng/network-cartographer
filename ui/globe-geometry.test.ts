import { describe, expect, it } from "vitest";
import {
  ambientMotionAllowed,
  ambientRouteCandidates,
  buildNewHopPulsePath,
  buildPulsePathPoints,
  cameraCompensatedScale,
  chooseAmbientRoute,
  classifySegment,
  gapShouldAnimate,
  segmentVisualState,
  selectionAllowsMotion,
  selectNonOverlappingLabels,
  segmentHasVisibleDistance,
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

  it("collapses consecutive hops geolocated to the same metro coordinate", () => {
    const points = buildPulsePathPoints([
      hop(1, { lat: 34.05, lon: -118.24 }),
      hop(2, { lat: 34.05, lon: -118.24 }),
      hop(3, { lat: 37.77, lon: -122.42 }),
    ]);
    expect(points[0].lat).toBeCloseTo(34.05);
    expect(points[0].lng).toBeCloseTo(-118.24);
    expect(points.at(-1)?.lat).toBeCloseTo(37.77);
    expect(points.at(-1)?.lng).toBeCloseTo(-122.42);
    expect(
      points.filter(
        (point) =>
          Math.abs(point.lat - 34.05) < 0.0001 &&
          Math.abs(point.lng + 118.24) < 0.0001,
      ),
    ).toHaveLength(1);
  });
});

describe("buildNewHopPulsePath", () => {
  it("builds a directional pulse over only the newly mapped hop span", () => {
    const previous = [
      hop(2, { lat: 34.05, lon: -118.24 }),
      hop(5, { lat: 39.1, lon: -94.58 }),
    ];
    const next = [
      ...previous,
      hop(8, { lat: 40.71, lon: -74.01 }),
    ];

    const points = buildNewHopPulsePath(previous, next);
    expect(points[0].lat).toBeCloseTo(39.1);
    expect(points[0].lng).toBeCloseTo(-94.58);
    expect(points.at(-1)?.lat).toBeCloseTo(40.71);
    expect(points.at(-1)?.lng).toBeCloseTo(-74.01);
  });

  it("does not replay when mapped route geometry is unchanged", () => {
    const route = [hop(2), hop(5, { lat: 40, lon: 100 })];
    expect(buildNewHopPulsePath(route, route)).toEqual([]);
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

describe("camera-aware geometry", () => {
  it("counter-scales world-sized marks as the camera zooms in", () => {
    expect(cameraCompensatedScale(1.9)).toBe(1);
    expect(cameraCompensatedScale(0.19)).toBeCloseTo(0.1);
    expect(cameraCompensatedScale(0.001)).toBe(0.06);
  });

  it("omits only segments whose mapped endpoints collapse to one location", () => {
    expect(
      segmentHasVisibleDistance(37.7749, -122.4194, 37.7749, -122.4194),
    ).toBe(false);
    expect(
      segmentHasVisibleDistance(37.7749, -122.4194, 37.7762, -122.4194),
    ).toBe(true);
    expect(segmentHasVisibleDistance(0, 179.9, 0, -179.9)).toBe(true);
  });
});

describe("map label collision selection", () => {
  const label = (
    text: string,
    lat: number,
    lng: number,
    priority = 100,
  ) => ({ lat, lng, label: text, size: 0.9, priority });

  it("deduplicates the same city across nearby datacenter coordinates", () => {
    const labels = selectNonOverlappingLabels([
      label("Los Angeles", 34.0522, -118.2437, 100),
      label("  LOS   ANGELES ", 34.12, -118.18, 200),
    ]);
    expect(labels).toHaveLength(1);
    expect(labels[0].priority).toBe(200);
  });

  it("keeps identical city names when they refer to distant places", () => {
    const labels = selectNonOverlappingLabels([
      label("Springfield", 39.78, -89.65),
      label("Springfield", 44.05, -123.02),
    ]);
    expect(labels).toHaveLength(2);
  });

  it("keeps the higher-priority label when different nearby names overlap", () => {
    const labels = selectNonOverlappingLabels([
      label("Ordinary hop", 37.77, -122.42, 100),
      label("Final destination", 37.78, -122.4, 400),
    ]);
    expect(labels.map((candidate) => candidate.label)).toEqual(["Final destination"]);
  });

  it("handles collisions across the antimeridian", () => {
    const labels = selectNonOverlappingLabels([
      label("West edge", 10, 179.9),
      label("East edge", 10.05, -179.9, 200),
    ]);
    expect(labels).toHaveLength(1);
    expect(labels[0].label).toBe("East edge");
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
