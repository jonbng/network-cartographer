export type GeoHop = {
  ttl: number;
  addr: string | null;
  lat: number | null;
  lon: number | null;
};

export type SegmentKind = "observed" | "unmapped";

export type SegmentClassification = {
  kind: SegmentKind;
  missingResponses: number;
  unlocatedHops: number;
};

export type PulsePoint = { lat: number; lng: number; altitude: number };

export type SegmentVisualState = "dimmed" | "emphasized" | "normal";

export type AmbientRouteCandidate = {
  id: string;
  appId: string;
  mappedHopCount: number;
};

type Vector3 = { x: number; y: number; z: number };

export function selectionAllowsMotion(reduceMotion: boolean, instant: boolean): boolean {
  return !reduceMotion && !instant;
}

export function ambientMotionAllowed(
  reduceMotion: boolean,
  documentHidden: boolean,
  cameraInteracting: boolean,
): boolean {
  return !reduceMotion && !documentHidden && !cameraInteracting;
}

export function gapShouldAnimate(
  kind: SegmentKind,
  traceStatus: string,
  reduceMotion: boolean,
): boolean {
  return kind === "unmapped" && traceStatus === "running" && !reduceMotion;
}

export function ambientRouteCandidates<T extends AmbientRouteCandidate>(
  routes: T[],
  selectedPathId: string | null,
  focusedAppIds: ReadonlySet<string>,
): T[] {
  const eligible = routes.filter((route) => route.mappedHopCount >= 2);
  if (selectedPathId) {
    return eligible.filter((route) => route.id === selectedPathId);
  }
  if (focusedAppIds.size > 0) {
    return eligible.filter((route) => focusedAppIds.has(route.appId));
  }
  return eligible;
}

export function chooseAmbientRoute<T extends AmbientRouteCandidate>(
  routes: T[],
  previousPathId: string | null,
  randomValue = Math.random(),
): T | null {
  if (routes.length === 0) return null;
  const alternatives = routes.length > 1
    ? routes.filter((route) => route.id !== previousPathId)
    : routes;
  const pool = alternatives.length > 0 ? alternatives : routes;
  const index = Math.min(pool.length - 1, Math.floor(randomValue * pool.length));
  return pool[index] ?? null;
}

export function segmentVisualState(
  dimmed: boolean,
  hovered: boolean,
  hopHighlighted: boolean,
): SegmentVisualState {
  if (dimmed) return "dimmed";
  if (hovered || hopHighlighted) return "emphasized";
  return "normal";
}

export function classifySegment(
  hops: GeoHop[],
  fromTtl: number,
  toTtl: number,
): SegmentClassification {
  const byTtl = new Map(hops.map((hop) => [hop.ttl, hop]));
  let missingResponses = 0;
  let unlocatedHops = 0;
  for (let ttl = fromTtl + 1; ttl < toTtl; ttl += 1) {
    const hop = byTtl.get(ttl);
    if (!hop || hop.addr == null) missingResponses += 1;
    else if (!isLocated(hop)) unlocatedHops += 1;
  }
  return {
    kind: missingResponses + unlocatedHops > 0 ? "unmapped" : "observed",
    missingResponses,
    unlocatedHops,
  };
}

export function buildPulsePathPoints(hops: GeoHop[]): PulsePoint[] {
  const located = hops.filter(isLocated).sort((a, b) => a.ttl - b.ttl);
  if (located.length < 2) return [];
  const points: PulsePoint[] = [];
  for (let i = 0; i < located.length - 1; i += 1) {
    const from = located[i];
    const to = located[i + 1];
    const fromVector = latLngVector(from.lat, from.lon);
    const toVector = latLngVector(to.lat, to.lon);
    const omega = Math.acos(clamp(dot(fromVector, toVector), -1, 1));
    const steps = Math.max(8, Math.ceil(omega * 18));
    const peakAltitude = Math.min(0.24, Math.max(0.025, omega * 0.12));
    for (let step = i === 0 ? 0 : 1; step <= steps; step += 1) {
      const t = step / steps;
      const vector = slerp(fromVector, toVector, omega, t);
      const position = vectorLatLng(vector);
      points.push({
        ...position,
        altitude: 0.012 + Math.sin(Math.PI * t) * peakAltitude,
      });
    }
  }
  return points;
}

export function isLocated<T extends GeoHop>(
  hop: T,
): hop is T & { lat: number; lon: number } {
  return (
    hop.lat != null &&
    hop.lon != null &&
    Number.isFinite(hop.lat) &&
    Number.isFinite(hop.lon)
  );
}

function latLngVector(lat: number, lng: number): Vector3 {
  const phi = (90 - lat) * (Math.PI / 180);
  const theta = (lng + 180) * (Math.PI / 180);
  return {
    x: -Math.sin(phi) * Math.cos(theta),
    y: Math.cos(phi),
    z: Math.sin(phi) * Math.sin(theta),
  };
}

function vectorLatLng(vector: Vector3): { lat: number; lng: number } {
  const length = Math.hypot(vector.x, vector.y, vector.z) || 1;
  const x = vector.x / length;
  const y = vector.y / length;
  const z = vector.z / length;
  const rawLng = Math.atan2(z, -x) * (180 / Math.PI) - 180;
  return {
    lat: 90 - Math.acos(clamp(y, -1, 1)) * (180 / Math.PI),
    lng: ((rawLng + 540) % 360) - 180,
  };
}

function dot(a: Vector3, b: Vector3): number {
  return a.x * b.x + a.y * b.y + a.z * b.z;
}

function slerp(a: Vector3, b: Vector3, omega: number, t: number): Vector3 {
  if (omega < 0.00001) return a;
  const sinOmega = Math.sin(omega);
  if (Math.abs(sinOmega) < 0.00001) {
    return {
      x: a.x + (b.x - a.x) * t,
      y: a.y + (b.y - a.y) * t,
      z: a.z + (b.z - a.z) * t,
    };
  }
  const fromWeight = Math.sin((1 - t) * omega) / sinOmega;
  const toWeight = Math.sin(t * omega) / sinOmega;
  return {
    x: a.x * fromWeight + b.x * toWeight,
    y: a.y * fromWeight + b.y * toWeight,
    z: a.z * fromWeight + b.z * toWeight,
  };
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}
