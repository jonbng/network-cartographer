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

const DEFAULT_CAMERA_ALTITUDE = 1.9;

export type AmbientRouteCandidate = {
  id: string;
  appId: string;
  mappedHopCount: number;
};

export type GlobeLabelCandidate = {
  lat: number;
  lng: number;
  label: string;
  /** Text height in angular degrees, matching globe.gl's labelSize unit. */
  size: number;
  priority: number;
};

const NEARBY_DUPLICATE_LABEL_DEGREES = 1.5;

/**
 * Keep the most useful label when nearby map text would collide.
 *
 * Labels live on the globe's tangent plane and their size is expressed in
 * angular degrees, so collision checks can happen before Three.js builds the
 * text meshes. The longitude distance is latitude-compensated and wrapped at
 * the antimeridian.
 */
export function selectNonOverlappingLabels<T extends GlobeLabelCandidate>(
  candidates: T[],
): T[] {
  const ranked = candidates
    .map((candidate, index) => ({ candidate, index }))
    .sort(
      (a, b) => b.candidate.priority - a.candidate.priority || a.index - b.index,
    );
  const kept: T[] = [];

  for (const { candidate } of ranked) {
    const normalized = normalizeMapLabel(candidate.label);
    const isNearbyDuplicate = kept.some(
      (other) =>
        normalizeMapLabel(other.label) === normalized &&
        angularDistance(candidate, other) < NEARBY_DUPLICATE_LABEL_DEGREES,
    );
    if (
      isNearbyDuplicate ||
      kept.some((other) => labelsCollide(candidate, other))
    ) {
      continue;
    }
    kept.push(candidate);
  }

  return kept;
}

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

/** Counter-scale world-sized marks as the camera approaches the globe. */
export function cameraCompensatedScale(
  altitude: number,
  referenceAltitude = DEFAULT_CAMERA_ALTITUDE,
): number {
  if (!Number.isFinite(altitude) || altitude <= 0) return 1;
  return clamp(altitude / referenceAltitude, 0.06, 1.55);
}

/** Omit arcs whose city-level endpoints collapse to the same coordinate. */
export function segmentHasVisibleDistance(
  startLat: number,
  startLng: number,
  endLat: number,
  endLng: number,
): boolean {
  const latDelta = Math.abs(startLat - endLat);
  const lngDelta = Math.abs(((startLng - endLng + 540) % 360) - 180);
  return latDelta > 0.001 || lngDelta > 0.001;
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
  const located = hops
    .filter(isLocated)
    .sort((a, b) => a.ttl - b.ttl)
    .filter(
      (hop, index, sorted) =>
        index === 0 ||
        segmentHasVisibleDistance(
          sorted[index - 1].lat,
          sorted[index - 1].lon,
          hop.lat,
          hop.lon,
        ),
    );
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

/** Build only the newly revealed span of a progressively growing route. */
export function buildNewHopPulsePath(
  previousHops: GeoHop[],
  nextHops: GeoHop[],
): PulsePoint[] {
  const previous = new Set(
    previousHops.filter(isLocated).map(mappedHopKey),
  );
  const located = nextHops
    .filter(isLocated)
    .sort((a, b) => a.ttl - b.ttl);
  const addedIndexes = located
    .map((hop, index) => previous.has(mappedHopKey(hop)) ? -1 : index)
    .filter((index) => index >= 0);
  if (addedIndexes.length === 0 || located.length < 2) return [];

  let start = Math.max(0, addedIndexes[0] - 1);
  let end = addedIndexes[addedIndexes.length - 1];
  // A location can arrive out of TTL order. Use the following known hop so
  // the reveal still has a clear source and destination.
  if (start === end && end < located.length - 1) end += 1;
  return buildPulsePathPoints(located.slice(start, end + 1));
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

function mappedHopKey(hop: GeoHop & { lat: number; lon: number }): string {
  return `${hop.ttl}@${hop.lat.toFixed(4)},${hop.lon.toFixed(4)}`;
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

function normalizeMapLabel(label: string): string {
  return label.trim().replace(/\s+/g, " ").toLocaleLowerCase();
}

function angularDistance(
  a: GlobeLabelCandidate,
  b: GlobeLabelCandidate,
): number {
  const latA = a.lat * (Math.PI / 180);
  const latB = b.lat * (Math.PI / 180);
  const latDelta = latB - latA;
  const lngDelta = wrappedLongitudeDelta(a.lng, b.lng) * (Math.PI / 180);
  const haversine =
    Math.sin(latDelta / 2) ** 2 +
    Math.cos(latA) * Math.cos(latB) * Math.sin(lngDelta / 2) ** 2;
  return 2 * Math.asin(Math.min(1, Math.sqrt(haversine))) * (180 / Math.PI);
}

function labelsCollide(
  a: GlobeLabelCandidate,
  b: GlobeLabelCandidate,
): boolean {
  const averageLat = ((a.lat + b.lat) / 2) * (Math.PI / 180);
  const horizontalDistance =
    wrappedLongitudeDelta(a.lng, b.lng) *
    Math.max(0.08, Math.abs(Math.cos(averageLat)));
  const verticalDistance = Math.abs(a.lat - b.lat);
  const horizontalClearance = labelHalfWidth(a) + labelHalfWidth(b) + 0.2;
  const verticalClearance = a.size * 0.55 + b.size * 0.55 + 0.16;
  return (
    horizontalDistance < horizontalClearance &&
    verticalDistance < verticalClearance
  );
}

function labelHalfWidth(candidate: GlobeLabelCandidate): number {
  // Optimer's average glyph is a little over half its cap height. Spaces and
  // narrow punctuation contribute less, which keeps this conservative without
  // hiding an excessive number of labels.
  const visualCharacters = [...candidate.label].reduce(
    (width, character) => width + (/\s|[.,'!:|]/.test(character) ? 0.35 : 0.58),
    0,
  );
  return Math.max(candidate.size * 0.7, candidate.size * visualCharacters * 0.5);
}

function wrappedLongitudeDelta(a: number, b: number): number {
  const direct = Math.abs(a - b) % 360;
  return Math.min(direct, 360 - direct);
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
