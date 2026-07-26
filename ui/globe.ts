import Globe from "globe.gl";
import * as THREE from "three";
import optimerTypeface from "./optimer_regular.typeface.json";
export { colorForKey } from "./path-color";
import {
  ambientRouteCandidates,
  ambientMotionAllowed,
  arcDurationScale,
  buildPulsePathPoints,
  cameraCompensatedScale,
  chooseAmbientRoute,
  classifySegment,
  gapShouldAnimate,
  isLocated,
  mapLabelColor,
  segmentVisualState,
  selectionAllowsMotion,
  segmentHasVisibleDistance,
  selectNonOverlappingLabels,
  type PulsePoint,
  type SegmentKind,
} from "./globe-geometry";

export { buildPulsePathPoints, classifySegment } from "./globe-geometry";
export type { PulsePoint } from "./globe-geometry";

export type GlobeHop = {
  ttl: number;
  addr: string | null;
  rttMs: number | null;
  hostname?: string | null;
  lat: number | null;
  lon: number | null;
  city: string | null;
  country: string | null;
  geoSource?: string | null;
  geoConfidence?: number | null;
  geoNote?: string | null;
  asn?: number | null;
  org?: string | null;
};

export type GlobePath = {
  id: string;
  appId: string;
  app: string;
  appIconUrl?: string | null;
  host: string;
  destinationOrg?: string | null;
  ip: string;
  port: number;
  protocol: string;
  domainSource?: string;
  domainConfidence?: "exact" | "high" | "low" | "none";
  domainAlternativesCount?: number;
  hits: number;
  color: string;
  hops: GlobeHop[];
  status: string;
  freshness: "fresh" | "refreshing" | "stale";
  rttMs: number | null;
  reachedTarget: boolean;
  targetRttMs: number | null;
  error?: string | null;
};

export type NetworkOrigin = {
  status: "locating" | "ready" | "unavailable";
  exit: NetworkExit | null;
  assessment:
    | "proxy_configured"
    | "tunnel_likely"
    | "proxy_and_tunnel"
    | "no_evidence"
    | "unknown";
  evidence: Array<{
    kind: "default_interface" | "system_proxy" | "environment_proxy";
    strength: "strong" | "supporting";
    label: string;
  }>;
  transition?: NetworkTransition | null;
};

export type NetworkTransition = {
  id: number;
  status: "detecting" | "ready" | "unavailable";
  ageSeconds: number;
  previousExit: NetworkExit | null;
  currentExit: NetworkExit | null;
};

export type NetworkExit = {
  ip: string | null;
  city: string | null;
  country: string | null;
  lat: number | null;
  lon: number | null;
  asn: number | null;
  organization: string | null;
  source: "hosted-egress" | "trace-fallback";
  confidence: number | null;
  ageSeconds: number;
};

export type HopRouteChoice = {
  pathId: string;
  app: string;
  host: string;
  ip: string;
  port: number;
  color: string;
  ttl: number;
  rttMs: number | null;
  isDestination: boolean;
};

export type HopSelection = {
  lat: number;
  lon: number;
  city: string | null;
  country: string | null;
  addr: string | null;
  hostname: string | null;
  asn: number | null;
  org: string | null;
  geoSource: string | null;
  geoConfidence: number | null;
  geoNote: string | null;
  routes: HopRouteChoice[];
};

export type GlobeSegmentKind = SegmentKind;

export type GlobeSegmentSelection = {
  pathId: string;
  app: string;
  host: string;
  fromTtl: number;
  toTtl: number;
  kind: GlobeSegmentKind;
  missingResponses: number;
  unlocatedHops: number;
};

export type PathSelectionOptions = {
  frame?: boolean;
  preview?: boolean;
  instant?: boolean;
};

type PathThrough = {
  pathId: string;
  app: string;
  host: string;
  ip: string;
  port: number;
  color: string;
  ttl: number;
  rttMs: number | null;
  isDestination: boolean;
  isLastMapped: boolean;
};

type Point = {
  lat: number;
  lng: number;
  label: string;
  size: number;
  color: string;
  isDestination: boolean;
  isLastMapped: boolean;
  city: string | null;
  country: string | null;
  addr: string | null;
  hostname: string | null;
  through: PathThrough[];
  dimmed: boolean;
  asn: number | null;
  org: string | null;
  geoSource: string | null;
  geoConfidence: number | null;
  geoNote: string | null;
  isOrigin: boolean;
};

const MARKER_SIZE_MULTIPLIER = 2.75;
const LINE_WIDTH_MULTIPLIER = 1.1;

type ActivityRing = {
  lat: number;
  lng: number;
  kind: "origin" | "arrival";
  color?: string;
};

type GapLabel = {
  lat: number;
  lng: number;
  altitude: number;
  label: "?";
  isGapLabel: true;
};

type Arc = {
  startLat: number;
  startLng: number;
  endLat: number;
  endLng: number;
  color: string | string[];
  brightColor: string[];
  pathId: string;
  app: string;
  host: string;
  dimmed: boolean;
  active: boolean;
  stroke: number;
  fromTtl: number;
  toTtl: number;
  kind: GlobeSegmentKind;
  missingResponses: number;
  unlocatedHops: number;
  traceStatus: string;
  revealFromStart: boolean;
  revealUntil: number | null;
  settleStartedAt: number | null;
  durationScale: number;
};

type PulsePath = {
  points: PulsePoint[];
  color: string;
  stroke: number;
  dashLength: number;
  dashGap: number;
  duration: number;
  tone: "selected" | "ambient";
};

type HighlightedHop = { pathId: string; ttl: number };

// eslint-disable-next-line @typescript-eslint/no-explicit-any
let globe: any = null;
let lastKey = "";
let showLabels = true;
let focusedApps: Set<string> = new Set();
let selectedPathId: string | null = null;
let density: "all" | "destinations" | "hubs" = "all";
let hasUserMovedCamera = false;
let lastFrameBounds: string | null = null;
let onHopClick: ((selection: HopSelection) => void) | null = null;
let onOriginClick: ((origin: NetworkOrigin) => void) | null = null;
let onSegmentClick: ((selection: GlobeSegmentSelection) => void) | null = null;
let hoveredSegmentKey: string | null = null;
let highlightedHop: HighlightedHop | null = null;
let currentPaths: GlobePath[] = [];
let reduceMotion = false;
let motionRun = 0;
let pulseTimer: ReturnType<typeof setTimeout> | null = null;
let pulseClearTimer: ReturnType<typeof setTimeout> | null = null;
let lastAmbientPathId: string | null = null;
let cameraInteracting = false;
let cameraScale = 1;
let zoomRefreshFrame: number | null = null;
let originRing: ActivityRing | null = null;
let arrivalRing: ActivityRing | null = null;
let pulseArrivalTimer: ReturnType<typeof setTimeout> | null = null;
let arrivalClearTimer: ReturnType<typeof setTimeout> | null = null;
let previousPathHits = new Map<string, number>();
let previousSegmentKeys = new Set<string>();
let hasSegmentBaseline = false;
const markerObjects = new Set<THREE.Group>();
let currentLabelPoints: Point[] = [];
let currentGapLabels: GapLabel[] = [];
let pointDataCache = new Map<string, Point>();
let arcDataCache = new Map<string, Arc>();
let pointerInteractionSuspended = false;
let arcSpeedTimer: ReturnType<typeof setTimeout> | null = null;
let arcSpeedFrame: number | null = null;

const INITIAL_ARC_REVEAL_MS = 1800;
const ARC_SPEED_SETTLE_MS = 720;

export function setFocusedApps(apps: string[]) {
  const next = new Set(apps);
  if (
    next.size === focusedApps.size &&
    [...next].every((app) => focusedApps.has(app))
  ) {
    return;
  }
  focusedApps = next;
  cancelPulse();
  scheduleAmbientPulse();
  lastKey = "";
}

export function setSelectedPath(
  pathId: string | null,
  options: PathSelectionOptions = {},
) {
  const changed = selectedPathId !== pathId;
  selectedPathId = pathId;
  lastKey = "";
  cancelPulse();
  if (!pathId || !changed) {
    scheduleAmbientPulse();
    return;
  }

  const path = currentPaths.find((candidate) => candidate.id === pathId);
  if (!path) return;
  const instant = !selectionAllowsMotion(reduceMotion, options.instant === true);
  const frameDuration = instant ? 0 : 450;
  if (options.frame) framePath(path, frameDuration);
  if (options.preview && !instant) {
    const run = motionRun;
    const delay = options.frame ? frameDuration : 0;
    pulseTimer = setTimeout(() => {
      if (run !== motionRun || cameraInteracting) return;
      playPathPreview(path, "selected");
    }, delay);
  } else scheduleAmbientPulse();
}

export function getSelectedPath(): string | null {
  return selectedPathId;
}

export function setDensity(mode: "all" | "destinations" | "hubs") {
  density = mode;
  lastKey = "";
}

export function setHopClickHandler(
  fn: ((selection: HopSelection) => void) | null,
) {
  onHopClick = fn;
}

export function setOriginClickHandler(
  fn: ((origin: NetworkOrigin) => void) | null,
) {
  onOriginClick = fn;
}

export function setSegmentClickHandler(
  fn: ((selection: GlobeSegmentSelection) => void) | null,
) {
  onSegmentClick = fn;
}

export function setHighlightedHop(hop: HighlightedHop | null) {
  if (
    highlightedHop?.pathId === hop?.pathId &&
    highlightedHop?.ttl === hop?.ttl
  ) {
    return;
  }
  highlightedHop = hop;
  lastKey = "";
  rerenderCurrentPaths();
}

export function initGlobe(container: HTMLElement) {
  if (globe) return globe;

  // Keep browser page zoom off the globe interaction surface.
  preventPageZoom(container);
  const motionQuery = window.matchMedia("(prefers-reduced-motion: reduce)");
  reduceMotion = motionQuery.matches;
  motionQuery.addEventListener("change", (event) => {
    reduceMotion = event.matches;
    cancelPulse();
    lastKey = "";
    rerenderCurrentPaths();
    scheduleAmbientPulse();
  });

  globe = new Globe(container)
    .backgroundColor("rgba(0,0,0,0)")
    .showAtmosphere(true)
    .atmosphereColor("#e0a86a")
    .atmosphereAltitude(0.2)
    .globeImageUrl("/earth-dark.webp")
    .pointAltitude(pointAltitude)
    .pointRadius(pointRadius)
    // Cylinders make useful interaction targets, but poor markers. The visible
    // nodes live in the custom Three.js layer below.
    .pointColor(() => "rgba(0,0,0,0)")
    .pointsMerge(false)
    .pointLabel((d: object) => hopTooltip(d as Point))
    .onPointClick((d: object) => selectPoint(d as Point))
    .arcColor((d: object) => {
      const a = d as Arc;
      if (a.dimmed) return ["rgba(120,140,160,0.1)", "rgba(120,140,160,0.06)"];
      if (a.kind === "unmapped") {
        return isArcEmphasized(a) ? "#f2c66d" : "rgba(217,185,110,0.76)";
      }
      if (isArcEmphasized(a)) return a.brightColor;
      return a.color;
    })
    .arcStroke(arcStroke)
    .arcAltitudeAutoScale(0.26)
    .arcDashLength(arcDashLength)
    .arcDashGap(arcDashGap)
    .arcDashInitialGap(arcDashInitialGap)
    .arcDashAnimateTime(arcDashAnimateTime)
    .arcLabel((d: object) => segmentTooltip(d as Arc))
    .onArcHover((d: object | null) => {
      const next = d ? arcKey(d as Arc) : null;
      if (hoveredSegmentKey === next) return;
      hoveredSegmentKey = next;
      lastKey = "";
      rerenderCurrentPaths();
    })
    .onArcClick((d: object) => {
      const arc = d as Arc;
      onSegmentClick?.(segmentSelection(arc));
    })
    .arcsTransitionDuration(0)
    .pointsTransitionDuration(0)
    .labelsTransitionDuration(0)
    .pathPoints("points")
    .pathPointLat("lat")
    .pathPointLng("lng")
    .pathPointAlt("altitude")
    .pathColor("color")
    .pathStroke(pathStroke)
    .pathDashLength((d: object) => (d as PulsePath).dashLength)
    .pathDashGap((d: object) => (d as PulsePath).dashGap)
    .pathDashAnimateTime((d: object) => (d as PulsePath).duration)
    .pathTransitionDuration(0)
    .ringLat("lat")
    .ringLng("lng")
    .ringColor((d: object) => {
      const ring = d as ActivityRing;
      return (time: number) => ring.kind === "origin"
        ? `rgba(125,211,199,${Math.max(0, 0.3 * (1 - time))})`
        : withAlpha(ring.color ?? "#e0a86a", Math.max(0, 0.46 * (1 - time)));
    })
    .ringMaxRadius(ringMaxRadius)
    .ringPropagationSpeed(ringPropagationSpeed)
    .ringRepeatPeriod((d: object) => (d as ActivityRing).kind === "origin" ? 2100 : 0)
    .customLayerLabel((d: object) => hopTooltip(d as Point))
    .onCustomLayerClick((d: object) => selectPoint(d as Point))
    .onCustomLayerHover((d: object | null, previous: object | null) => {
      setMarkerHovered(previous as Point | null, false);
      setMarkerHovered(d as Point | null, true);
    })
    .customThreeObject((d: object) => createMarkerObject(d as Point))
    .customThreeObjectUpdate((object: THREE.Object3D, d: object) => {
      updateMarkerObject(object as THREE.Group, d as Point);
    });

  const controls = globe.controls();
  controls.autoRotate = false;
  controls.enableDamping = true;
  controls.dampingFactor = 0.08;
  controls.minDistance = 106;
  controls.maxDistance = 800;
  // Zoom only the camera (three.js), not the page
  controls.enableZoom = true;
  controls.zoomSpeed = 0.9;

  // Remember that the user took over the camera
  const markMoved = () => {
    hasUserMovedCamera = true;
    cameraInteracting = true;
    cancelPulse();
  };
  controls.addEventListener("start", markMoved);
  controls.addEventListener("change", scheduleZoomRefresh);
  controls.addEventListener("end", () => {
    cameraInteracting = false;
    if (pointerInteractionSuspended) {
      globe.enablePointerInteraction(true);
      pointerInteractionSuspended = false;
    }
    scheduleAmbientPulse();
  });
  container.addEventListener(
    "wheel",
    (e) => {
      // Never let ctrl/meta+wheel become page zoom
      if (e.ctrlKey || e.metaKey) e.preventDefault();
      e.stopPropagation();
      hasUserMovedCamera = true;
    },
    { passive: false },
  );
  container.addEventListener("pointerdown", markMoved);
  document.addEventListener("visibilitychange", () => {
    cancelPulse();
    if (document.hidden) {
      globe?.pauseAnimation();
    } else {
      globe?.resumeAnimation();
      scheduleAmbientPulse();
    }
  });

  globe.pointOfView({ lat: 30, lng: -40, altitude: 1.9 }, 0);
  refreshCameraScale();

  resizeGlobe(container);
  const ro = new ResizeObserver(() => resizeGlobe(container));
  ro.observe(container);
  return globe;
}

function selectPoint(point: Point) {
  if (point.isOrigin) {
    if (currentOrigin) onOriginClick?.(currentOrigin);
    return;
  }
  const routes = [...new Map(
    point.through.map((route) => [route.pathId, route]),
  ).values()];
  onHopClick?.({
    lat: point.lat,
    lon: point.lng,
    city: point.city,
    country: point.country,
    addr: point.addr,
    hostname: point.hostname,
    asn: point.asn,
    org: point.org,
    geoSource: point.geoSource,
    geoConfidence: point.geoConfidence,
    geoNote: point.geoNote,
    routes,
  });
}

function preventPageZoom(container: HTMLElement) {
  // Ctrl/cmd + wheel anywhere in the app → don't zoom the whole UI
  const block = (e: WheelEvent) => {
    if (e.ctrlKey || e.metaKey) {
      e.preventDefault();
    }
  };
  document.addEventListener("wheel", block, { passive: false, capture: true });

  // Pinch-zoom gestures (trackpads / touch)
  document.addEventListener(
    "gesturestart",
    (e) => e.preventDefault(),
    { passive: false } as AddEventListenerOptions,
  );
  document.addEventListener(
    "gesturechange",
    (e) => e.preventDefault(),
    { passive: false } as AddEventListenerOptions,
  );

  container.style.touchAction = "none";
}

export function resizeGlobe(container: HTMLElement) {
  if (!globe) return;
  const { width, height } = container.getBoundingClientRect();
  if (width > 0 && height > 0) {
    globe.width(width);
    globe.height(height);
  }
}

function pointRadius(d: object): number {
  return (d as Point).size * cameraScale;
}

function pointAltitude(d: object): number {
  const point = d as Point;
  const altitude = point.isOrigin ? 0.022 : point.isDestination ? 0.015 : 0.006;
  return altitude * cameraScale;
}

function arcStroke(d: object): number {
  const arc = d as Arc;
  const stroke = isArcEmphasized(arc)
    ? Math.max(0.72, arc.stroke * 1.7)
    : Math.max(arc.dimmed ? 0.28 : 0.46, arc.stroke * 1.5);
  return stroke * LINE_WIDTH_MULTIPLIER * cameraScale;
}

function arcDashLength(d: object): number {
  const arc = d as Arc;
  return arc.kind === "unmapped" ? 0.075 : arc.active ? 0.13 : 0.1;
}

function arcDashGap(d: object): number {
  const arc = d as Arc;
  return arc.kind === "unmapped" ? 0.045 : arc.active ? 0.055 : 0.065;
}

function arcDashInitialGap(d: object): number {
  return !reduceMotion && (d as Arc).revealFromStart ? 1 : 0;
}

function arcDashAnimateTime(d: object): number {
  const arc = d as Arc;
  if (reduceMotion) return 0;
  const fastDuration = INITIAL_ARC_REVEAL_MS * arc.durationScale;
  const cruiseDuration = arcCruiseDuration(arc);
  const now = Date.now();
  if (arc.revealUntil != null && now < arc.revealUntil) return fastDuration;
  if (arc.settleStartedAt != null) {
    const progress = Math.min(1, (now - arc.settleStartedAt) / ARC_SPEED_SETTLE_MS);
    const eased = progress * progress * (3 - 2 * progress);
    // Interpolate velocity rather than duration; that is what the renderer
    // actually advances each frame and avoids a perceptible mid-ramp lurch.
    const fastStep = 1000 / fastDuration;
    const cruiseStep = 1000 / cruiseDuration;
    const step = fastStep + (cruiseStep - fastStep) * eased;
    return 1000 / step;
  }
  return cruiseDuration;
}

function arcCruiseDuration(arc: Arc): number {
  const base = isArcEmphasized(arc)
    ? 4000
    : arc.active
      ? 4800
      : arc.kind === "unmapped"
        ? 5600
        : arc.dimmed
          ? 9000
          : 6800;
  return base * arc.durationScale;
}

function scheduleArcCruiseSpeed(arcs: Arc[]) {
  if (arcSpeedTimer) clearTimeout(arcSpeedTimer);
  arcSpeedTimer = null;
  const now = Date.now();
  const nextDeadline = arcs.reduce<number | null>((nearest, arc) => {
    if (arc.revealUntil == null || arc.revealUntil <= now) return nearest;
    return nearest == null ? arc.revealUntil : Math.min(nearest, arc.revealUntil);
  }, null);
  if (nextDeadline == null) return;

  arcSpeedTimer = setTimeout(() => {
    arcSpeedTimer = null;
    const startedAt = Date.now();
    for (const arc of arcDataCache.values()) {
      if (arc.revealUntil != null && arc.revealUntil <= startedAt) {
        arc.settleStartedAt = startedAt;
      }
    }
    runArcSpeedSettle();
    scheduleArcCruiseSpeed([...arcDataCache.values()]);
  }, Math.max(0, nextDeadline - now));
}

function runArcSpeedSettle() {
  if (arcSpeedFrame != null) return;
  const tick = () => {
    arcSpeedFrame = null;
    const now = Date.now();
    let stillSettling = false;
    for (const arc of arcDataCache.values()) {
      if (arc.settleStartedAt == null) continue;
      if (now - arc.settleStartedAt >= ARC_SPEED_SETTLE_MS) {
        arc.revealUntil = null;
        arc.settleStartedAt = null;
      } else {
        stillSettling = true;
      }
      setArcDashSpeed(arc);
    }
    if (stillSettling) arcSpeedFrame = requestAnimationFrame(tick);
  };
  arcSpeedFrame = requestAnimationFrame(tick);
}

function setArcDashSpeed(arc: Arc) {
  const group = (arc as Arc & {
    __threeObjArc?: THREE.Group;
  }).__threeObjArc;
  const line = group?.children[0] as (THREE.Object3D & {
    __dashAnimateStep?: number;
  }) | undefined;
  if (!line) return;
  const duration = arcDashAnimateTime(arc);
  line.__dashAnimateStep = duration > 0 ? 1000 / duration : 0;
}

function pathStroke(d: object): number {
  return (d as PulsePath).stroke * 1.45 * LINE_WIDTH_MULTIPLIER * cameraScale;
}

function ringMaxRadius(d: object): number {
  return ((d as ActivityRing).kind === "origin" ? 2.6 : 1.45) * cameraScale;
}

function ringPropagationSpeed(d: object): number {
  return ((d as ActivityRing).kind === "origin" ? 1.1 : 1.8) * cameraScale;
}

function scheduleZoomRefresh() {
  if (cameraInteracting && !pointerInteractionSuspended) {
    // globe.gl raycasts through every arc and marker several times a second.
    // There is no useful hover/click target while the camera is moving, so
    // skip that invisible work until the controls settle.
    globe?.enablePointerInteraction(false);
    pointerInteractionSuspended = true;
  }
  if (zoomRefreshFrame != null) return;
  zoomRefreshFrame = requestAnimationFrame(() => {
    zoomRefreshFrame = null;
    refreshCameraScale();
  });
}

function refreshCameraScale() {
  if (!globe) return;
  const altitude = Number(globe.pointOfView()?.altitude ?? 1.9);
  const next = cameraCompensatedScale(altitude);
  if (Math.abs(next - cameraScale) < 0.015) return;
  cameraScale = next;
  markerObjects.forEach(updateMarkerScale);
  pointDataCache.forEach(updatePointHitTargetScale);
  // Re-applying accessors refreshes only the affected Three.js layers. It
  // avoids rebuilding route geometry on every camera frame.
  globe
    .arcStroke(arcStroke)
    .pathStroke(pathStroke)
    .ringMaxRadius(ringMaxRadius)
    .ringPropagationSpeed(ringPropagationSpeed);
  if (showLabels) {
    globe
      .labelSize(labelSize)
      .labelAltitude(labelAltitude)
      .labelDotRadius(labelDotRadius);
  }
  refreshVisibleLabels();
}

function createMarkerObject(point: Point): THREE.Group {
  const group = new THREE.Group();
  group.userData.isGlobeMarker = true;

  const color = new THREE.Color(opaqueCssColor(markerCssColor(point)));
  const opacity = markerOpacity(point);
  const coreGeometry = point.isDestination
    ? new THREE.OctahedronGeometry(0.38, 0)
    : new THREE.SphereGeometry(point.isOrigin ? 0.34 : 0.3, 12, 8);
  const core = new THREE.Mesh(
    coreGeometry,
    new THREE.MeshBasicMaterial({
      color,
      transparent: true,
      opacity,
      depthWrite: false,
    }),
  );
  if (point.isDestination) core.rotation.z = Math.PI / 4;
  group.add(core);

  const halo = markerRing(color, point.isOrigin ? 0.56 : 0.5, point.isOrigin ? 0.68 : 0.61, opacity * 0.55);
  group.add(halo);

  if (point.isOrigin || point.isDestination || point.isLastMapped) {
    const outer = markerRing(
      color,
      point.isOrigin ? 0.79 : 0.72,
      point.isOrigin ? 0.84 : 0.77,
      opacity * 0.28,
    );
    outer.rotation.z = point.isDestination ? Math.PI / 4 : 0;
    group.add(outer);
  }

  updateMarkerObject(group, point);
  return group;
}

function markerRing(
  color: THREE.Color,
  innerRadius: number,
  outerRadius: number,
  opacity: number,
): THREE.Mesh {
  return new THREE.Mesh(
    new THREE.RingGeometry(innerRadius, outerRadius, 32),
    new THREE.MeshBasicMaterial({
      color,
      transparent: true,
      opacity,
      side: THREE.DoubleSide,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
    }),
  );
}

function updateMarkerObject(group: THREE.Group, point: Point) {
  if (!globe) return;
  group.userData.point = point;
  const altitude = point.isOrigin ? 0.006 : 0.0035;
  const position = globe.getCoords(point.lat, point.lng, altitude * cameraScale);
  group.position.set(position.x, position.y, position.z);
  group.lookAt(0, 0, 0);
  markerObjects.add(group);
  updateMarkerScale(group);
}

function updateMarkerScale(group: THREE.Group) {
  const point = group.userData.point as Point | undefined;
  if (!point) return;
  const hoverScale = group.userData.hovered === true ? 1.16 : 1;
  const scale = point.size * MARKER_SIZE_MULTIPLIER * cameraScale * hoverScale;
  group.scale.setScalar(scale);
}

function updatePointHitTargetScale(point: Point) {
  const mesh = (point as Point & { __threeObjPoint?: THREE.Mesh }).__threeObjPoint;
  if (!mesh || !globe) return;
  const globeRadius = Number(globe.getGlobeRadius?.() ?? 100);
  const pixelsPerDegree = 2 * Math.PI * globeRadius / 360;
  mesh.scale.x = mesh.scale.y = Math.min(30, pointRadius(point)) * pixelsPerDegree;
  mesh.scale.z = Math.max(pointAltitude(point) * globeRadius, 0.1);
}

function disableRaycastTree(root: THREE.Object3D | undefined) {
  root?.traverse((object) => {
    object.raycast = () => undefined;
  });
}

function disableLabelRaycasts(labels: Array<Point | GapLabel>) {
  requestAnimationFrame(() => {
    for (const label of labels) {
      const object = (label as (Point | GapLabel) & {
        __threeObjLabel?: THREE.Object3D;
      }).__threeObjLabel;
      disableRaycastTree(object);
    }
  });
}

function setMarkerHovered(point: Point | null, hovered: boolean) {
  if (!point) return;
  for (const group of markerObjects) {
    if (group.userData.point !== point) continue;
    group.userData.hovered = hovered;
    updateMarkerScale(group);
    break;
  }
}

function markerCssColor(point: Point): string {
  if (point.isOrigin) return "#7dd3c7";
  if (point.isDestination) return "#f9a8d4";
  if (point.isLastMapped) return "#f2c66d";
  if (point.dimmed) return "#788ca0";
  return point.color;
}

function markerOpacity(point: Point): number {
  if (point.dimmed) return 0.28;
  const match = point.color.match(/rgba?\([^,]+,[^,]+,[^,]+,\s*([\d.]+)\)/);
  const sourceAlpha = match ? Number(match[1]) : 1;
  return Math.max(0.42, Math.min(1, sourceAlpha));
}

function markerVisualKey(point: Point): string {
  const shape = point.isOrigin
    ? "origin"
    : point.isDestination
      ? "destination"
      : point.isLastMapped
        ? "last-mapped"
        : "hop";
  return `${shape}|${markerCssColor(point)}|${markerOpacity(point)}`;
}

function reconcilePointData(points: Point[]): Point[] {
  const nextCache = new Map<string, Point>();
  const reconciled = points.map((point) => {
    const key = locKey(point.lat, point.lng);
    const previous = pointDataCache.get(key);
    // Preserve the datum identity that globe.gl uses to retain its Three.js
    // objects. If a marker's actual visual construction changes, let the
    // library replace it so the result remains pixel-for-pixel identical.
    const stable = previous && markerVisualKey(previous) === markerVisualKey(point)
      ? Object.assign(previous, point)
      : point;
    nextCache.set(key, stable);
    return stable;
  });
  pointDataCache = nextCache;
  return reconciled;
}

function reconcileArcData(arcs: Arc[]): Arc[] {
  const nextCache = new Map<string, Arc>();
  const reconciled = arcs.map((arc) => {
    const key = arcKey(arc);
    const previous = arcDataCache.get(key);
    // Keeping the same datum lets three-globe update the existing mesh. It
    // then rebuilds tube geometry only when that segment actually changed.
    // Once a segment starts feeding in, retain its leading-gap state. The
    // normal dash translation eventually outruns that gap and then continues
    // forever; resetting it on the next traceroute update would make it pop.
    const stable = previous
      ? Object.assign(previous, arc, {
          revealFromStart: previous.revealFromStart || arc.revealFromStart,
          revealUntil: previous.revealUntil ?? arc.revealUntil,
          settleStartedAt: previous.settleStartedAt,
        })
      : arc;
    nextCache.set(key, stable);
    return stable;
  });
  arcDataCache = nextCache;
  return reconciled;
}

function pruneMarkerObjectReferences(points: Point[]) {
  const livePoints = new Set(points);
  for (const group of markerObjects) {
    if (!livePoints.has(group.userData.point as Point)) markerObjects.delete(group);
  }
}

function opaqueCssColor(color: string): string {
  return color.replace(
    /^rgba\(([^,]+,[^,]+,[^,]+),\s*[\d.]+\)$/,
    "rgb($1)",
  );
}

function disposeMarkerObjects() {
  for (const group of markerObjects) {
    group.traverse((object) => {
      const mesh = object as THREE.Mesh;
      mesh.geometry?.dispose();
      const materials = Array.isArray(mesh.material)
        ? mesh.material
        : mesh.material
          ? [mesh.material]
          : [];
      materials.forEach((material) => material.dispose());
    });
  }
  markerObjects.clear();
}

export function setLabelsVisible(on: boolean) {
  showLabels = on;
  lastKey = "";
}

/** Frame camera on current paths (user-triggered). */
export function recenterOnData() {
  hasUserMovedCamera = false;
  lastFrameBounds = null;
  // next updateAllPaths will reframe; force by clearing key
  lastKey = "";
}

function hopTooltip(p: Point): string {
  if (p.isOrigin) return originTooltip(currentOrigin);
  const city = p.city
    ? `${p.city}${p.country ? ", " + p.country : ""}`
    : "Unknown location";
  const kind = p.isDestination
    ? `<span style="color:#f9a8d4;font-weight:700">★ Final destination</span>`
    : p.isLastMapped
      ? `<span style="color:#f2c66d;font-weight:700">◌ Last mapped hop · target not confirmed</span>`
    : `<span style="color:#94a3b8">Observed network hop</span>`;

  const apps = new Map<string, PathThrough[]>();
  for (const t of p.through) {
    const list = apps.get(t.app) ?? [];
    list.push(t);
    apps.set(t.app, list);
  }

  const appBlocks: string[] = [];
  for (const [app, routes] of apps) {
    const color = routes[0]?.color ?? "#5eead4";
    const destLines = routes
      .slice(0, 8)
      .map((r) => {
        const rtt = r.rttMs != null ? `${r.rttMs.toFixed(0)}ms` : "-";
        const star = r.isDestination ? " ★" : "";
        return `<div style="margin-left:14px;opacity:0.92">→ ${escapeHtml(prettyHost(r.host))} · hop ${r.ttl} · ${rtt}${star}</div>`;
      })
      .join("");
    const more =
      routes.length > 8
        ? `<div style="margin-left:14px;opacity:0.5">+${routes.length - 8} more</div>`
        : "";
    appBlocks.push(`
      <div style="margin-top:6px">
        <div style="display:flex;align-items:center;gap:6px">
          <span style="width:8px;height:8px;border-radius:50%;background:${color};box-shadow:0 0 8px ${color}"></span>
          <b style="color:${color}">${escapeHtml(app)}</b>
          <span style="opacity:0.65;font-size:14px">${routes.length} path${routes.length > 1 ? "s" : ""}</span>
        </div>
        ${destLines}${more}
      </div>`);
  }

  const addr = p.hostname || p.addr || "";
  const addrLine = addr
    ? `<div style="opacity:0.7;font-family:ui-monospace,monospace;font-size:14px;margin-top:2px">${escapeHtml(addr)}</div>`
    : "";
  const network = p.org
    ? `<div style="opacity:0.78;margin-top:3px">${escapeHtml(p.org)}${p.asn ? ` · AS${p.asn}` : ""}</div>`
    : "";
  const confidence = confidenceLabel(p.geoConfidence);
  const evidence = p.geoSource
    ? `<div style="opacity:0.62;margin-top:3px">Location: ${escapeHtml(sourceLabel(p.geoSource))}${confidence ? ` · ${confidence} confidence` : ""}</div>`
    : "";
  const note = p.geoNote
    ? `<div style="color:#f2c66d;opacity:0.78;margin-top:3px">${escapeHtml(p.geoNote)}</div>`
    : "";

  return `<div style="font-family:'IBM Plex Mono','Cascadia Mono','SFMono-Regular',monospace;font-size:16px;line-height:1.5;padding:8px 4px;max-width:420px">
    <div>${kind}</div>
    <div style="margin-top:4px;font-weight:600">${escapeHtml(city)}</div>
    ${addrLine}${network}${evidence}${note}
    <div style="margin-top:10px;padding-top:8px;border-top:1px solid rgba(255,255,255,0.12);font-size:14px;opacity:0.7">
      Network topology, not a physical cable path · click to inspect
    </div>
    ${appBlocks.join("") || `<div style="opacity:0.6;margin-top:4px">No path data</div>`}
  </div>`;
}

function originTooltip(origin: NetworkOrigin | null): string {
  if (!origin?.exit) return "Primary network exit";
  const exit = origin.exit;
  const place = exit.city
    ? `${exit.city}${exit.country ? `, ${exit.country}` : ""}`
    : "Location unavailable";
  const network = exit.organization
    ? `${exit.organization}${exit.asn ? ` · AS${exit.asn}` : ""}`
    : "Network provider unavailable";
  return `<div style="font-family:'IBM Plex Mono','Cascadia Mono','SFMono-Regular',monospace;font-size:16px;line-height:1.5;padding:8px 4px;max-width:420px">
    <div style="color:#7dd3c7;font-weight:700;letter-spacing:.06em;text-transform:uppercase;font-size:14px">Primary network exit</div>
    <div style="margin-top:5px;font-weight:650">${escapeHtml(place)}</div>
    <div style="opacity:.78;margin-top:2px">${escapeHtml(network)}</div>
    ${exit.ip ? `<div style="opacity:.62;margin-top:2px">${escapeHtml(exit.ip)}</div>` : ""}
    <div style="margin-top:10px;padding-top:8px;border-top:1px solid rgba(255,255,255,.12);opacity:.68;font-size:14px">Where the public internet sees this connection · click for evidence</div>
  </div>`;
}

/** Short readable host for UI; drop PTR junk and ultra-long labels. */
export function prettyHost(host: string): string {
  let h = host.trim();
  if (!h) return host;
  // Bare IP → keep short
  if (/^\d{1,3}(\.\d{1,3}){3}$/.test(h) || h.includes(":")) {
    return h.length > 18 ? h.slice(0, 16) + "…" : h;
  }
  // Reverse DNS like 1-2-3-4.isp.net → last 2 labels if noisy
  const parts = h.split(".").filter(Boolean);
  if (parts.length >= 3 && /^[\d-]+$/.test(parts[0])) {
    h = parts.slice(-2).join(".");
  }
  // Drop leading hex/hash tokens
  h = h.replace(/^[0-9a-f]{8,}[.-]/i, "");
  if (h.length > 28) h = h.slice(0, 26) + "…";
  return h;
}

let currentOrigin: NetworkOrigin | null = null;

function geometryKey(paths: GlobePath[], origin: NetworkOrigin | null): string {
  const focus = [...focusedApps].sort().join(",");
  const exit = origin?.exit;
  const highlighted = highlightedHop
    ? `${highlightedHop.pathId}:${highlightedHop.ttl}`
    : "";
  let s = `${showLabels ? 1 : 0}|${focus}|${selectedPathId ?? ""}|${hoveredSegmentKey ?? ""}|${highlighted}|${density}|${reduceMotion ? 1 : 0}|${origin?.status ?? ""}:${origin?.assessment ?? ""}:${exit?.lat ?? ""},${exit?.lon ?? ""}:${exit?.city ?? ""}|`;
  for (const p of paths) {
    s += `${p.id}:${p.reachedTarget ? 1 : 0}:${p.status}`;
    for (const h of p.hops) {
      if (h.lat == null || h.lon == null) continue;
      // city name matters for labels but only once geocoded
      const cityBit = h.city ? h.city.slice(0, 12) : "";
      s += `${h.ttl}@${h.lat.toFixed(2)},${h.lon.toFixed(2)}:${cityBit}:${(h.geoConfidence ?? 0).toFixed(2)};`;
    }
    s += "|";
  }
  return s;
}

function segmentKey(pathId: string, fromTtl: number, toTtl: number): string {
  return `${pathId}:${fromTtl}-${toTtl}`;
}

function detectNewSegments(paths: GlobePath[]): Set<string> {
  const next = new Set<string>();
  for (const path of paths) {
    const located = path.hops.filter(isLocated).sort((a, b) => a.ttl - b.ttl);
    for (let index = 0; index < located.length - 1; index += 1) {
      const from = located[index];
      const to = located[index + 1];
      if (segmentHasVisibleDistance(from.lat, from.lon, to.lat, to.lon)) {
        next.add(segmentKey(path.id, from.ttl, to.ttl));
      }
    }
  }
  const added = hasSegmentBaseline
    ? new Set([...next].filter((key) => !previousSegmentKeys.has(key)))
    : new Set<string>();
  previousSegmentKeys = next;
  hasSegmentBaseline = true;
  return added;
}

export function updateAllPaths(paths: GlobePath[], origin: NetworkOrigin | null = null): {
  pathCount: number;
  hopCount: number;
  destCount: number;
} {
  const newSegmentKeys = detectNewSegments(paths);
  currentPaths = paths;
  const activityPath = detectRouteActivity(paths);
  if (activityPath) scheduleActivityPulse(activityPath);
  else scheduleAmbientPulse();
  if (!globe) return { pathCount: 0, hopCount: 0, destCount: 0 };

  const pathCount = paths.filter((p) => p.hops.some((h) => h.lat != null)).length;
  let hopCount = 0;
  let destCount = 0;
  for (const p of paths) {
    const n = p.hops.filter((h) => h.lat != null).length;
    hopCount += n;
    if (p.reachedTarget && p.hops.some((h) => h.addr === p.ip && h.lat != null)) {
      destCount += 1;
    }
  }

  currentOrigin = origin;
  const key = geometryKey(paths, origin);
  if (key === lastKey) {
    return { pathCount, hopCount, destCount };
  }
  lastKey = key;

  const nodeMap = new Map<string, Point>();
  let arcs: Arc[] = [];
  const allLats: number[] = [];
  const allLngs: number[] = [];

  for (const path of paths) {
    const dimmed = selectedPathId
      ? path.id !== selectedPathId
      : focusedApps.size > 0 && !focusedApps.has(path.appId);
    const active = selectedPathId === path.id || focusedApps.has(path.appId);
    const located = path.hops.filter(
      (h) =>
        h.lat != null &&
        h.lon != null &&
        Number.isFinite(h.lat) &&
        Number.isFinite(h.lon),
    );
    if (located.length === 0) continue;

    // Density filter for arcs
    const arcHops =
      density === "destinations"
        ? located.length >= 2
          ? [located[located.length - 2], located[located.length - 1]]
          : located
        : located;

    located.forEach((h, i) => {
      const isEnd = path.reachedTarget && h.addr === path.ip;
      const isLastMapped = i === located.length - 1;
      if (density === "destinations" && !isEnd && !isLastMapped) return;
      const nkey = locKey(h.lat as number, h.lon as number);
      const through: PathThrough = {
        pathId: path.id,
        app: path.app,
        host: path.host,
        ip: path.ip,
        port: path.port,
        color: path.color,
        ttl: h.ttl,
        rttMs: h.rttMs,
        isDestination: isEnd,
        isLastMapped,
      };

      allLats.push(h.lat as number);
      allLngs.push(h.lon as number);

      const existing = nodeMap.get(nkey);
      if (existing) {
        const sameAddress = existing.addr === h.addr;
        existing.through.push(through);
        if (isEnd) {
          existing.isDestination = true;
          existing.size = Math.max(existing.size, 0.55);
          if (!dimmed) {
            existing.color = "#f9a8d4";
            existing.label = labelForDestination(path, h);
          }
        }
        if (isLastMapped && !path.reachedTarget && !existing.isDestination) {
          existing.isLastMapped = true;
          existing.size = Math.max(existing.size, 0.35);
          if (!dimmed) existing.color = "#f2c66d";
        }
        if (!dimmed) existing.dimmed = false;
        if (!existing.isDestination && !dimmed) {
          existing.color = path.color;
        }
        // Prefer a real city name if we learn one
        if (h.city && !existing.city) {
          existing.city = h.city;
          existing.country = h.country;
          if (!existing.isDestination) {
            existing.label = h.city;
          }
        }
        if ((h.geoConfidence ?? -1) > (existing.geoConfidence ?? -1)) {
          existing.geoSource = h.geoSource ?? existing.geoSource;
          existing.geoConfidence = h.geoConfidence ?? existing.geoConfidence;
          existing.geoNote = h.geoNote ?? existing.geoNote;
        }
        // Several IPs can collapse onto the same city coordinate. Do not show
        // one route's address or owner as though it described the whole point.
        if (!sameAddress) {
          existing.addr = null;
          existing.hostname = null;
          existing.asn = null;
          existing.org = null;
        }
      } else {
        nodeMap.set(nkey, {
          lat: h.lat as number,
          lng: h.lon as number,
          label: isEnd
            ? labelForDestination(path, h)
            : h.city || "",
          size: isEnd ? 0.55 : i === 0 ? 0.42 : 0.34,
          color: dimmed
            ? "rgba(120,140,160,0.25)"
            : isEnd
              ? "#f9a8d4"
              : path.color,
          isDestination: isEnd,
          isLastMapped: isLastMapped && !path.reachedTarget,
          city: h.city,
          country: h.country,
          addr: h.addr,
          hostname: h.hostname ?? null,
          through: [through],
          dimmed,
          asn: h.asn ?? null,
          org: h.org ?? null,
          geoSource: h.geoSource ?? null,
          geoConfidence: h.geoConfidence ?? null,
          geoNote: h.geoNote ?? null,
          isOrigin: false,
        });
      }
    });

    for (let i = 0; i < arcHops.length - 1; i++) {
      const a = arcHops[i];
      const b = arcHops[i + 1];
      const isLastArc = i === arcHops.length - 2;
      const bIsTarget = path.reachedTarget && b.addr === path.ip;
      if (!segmentHasVisibleDistance(
        a.lat as number,
        a.lon as number,
        b.lat as number,
        b.lon as number,
      )) continue;
      const segment = classifySegment(path.hops, a.ttl, b.ttl);
      const brightColor = bIsTarget
        ? [path.color, "#f9a8d4"]
        : [path.color, lighten(path.color, 0.15)];
      const isNewSegment = newSegmentKeys.has(segmentKey(path.id, a.ttl, b.ttl));
      const durationScale = arcDurationScale(
        a.lat as number,
        a.lon as number,
        b.lat as number,
        b.lon as number,
      );
      arcs.push({
        startLat: a.lat as number,
        startLng: a.lon as number,
        endLat: b.lat as number,
        endLng: b.lon as number,
        color: dimmed
          ? ["rgba(100,120,140,0.08)", "rgba(100,120,140,0.04)"]
          : brightColor.map((color, index) => withAlpha(
              color,
              active
                ? (index === 0 ? 0.85 : 0.95)
                : (index === 0 ? 0.5 : 0.7),
            )),
        brightColor,
        pathId: path.id,
        app: path.app,
        host: path.host,
        dimmed,
        active,
        stroke: dimmed
          ? 0.16
          : active
            ? (bIsTarget ? 0.58 : 0.46)
            : isLastArc
              ? 0.38
              : 0.3,
        fromTtl: a.ttl,
        toTtl: b.ttl,
        traceStatus: path.status,
        revealFromStart: isNewSegment,
        revealUntil: isNewSegment
          ? Date.now() + INITIAL_ARC_REVEAL_MS * durationScale
          : null,
        settleStartedAt: null,
        durationScale,
        ...segment,
      });
    }
  }

  let points = [...nodeMap.values()].map((p) => {
    if (p.dimmed) return { ...p, color: "rgba(120,140,160,0.2)" };
    if (p.isDestination) {
      const alpha = confidenceAlpha(p.geoConfidence);
      return {
        ...p,
        color: withAlpha("#f9a8d4", alpha),
        size: Math.max(p.size, 0.55),
      };
    }
    if (p.isLastMapped) {
      const alpha = confidenceAlpha(p.geoConfidence);
      return {
        ...p,
        color: withAlpha("#f2c66d", alpha),
        size: Math.max(p.size, 0.35),
      };
    }
    const uniqueApps = new Set(p.through.map((t) => t.app));
    if (uniqueApps.size > 1) {
      return { ...p, color: "#cbd5e1", size: Math.max(p.size, 0.28) };
    }
    const alpha = confidenceAlpha(p.geoConfidence);
    return alpha < 1 ? { ...p, color: withAlpha(p.color, alpha) } : p;
  });

  if (highlightedHop) {
    points = points.map((point) => {
      const highlighted = point.through.some(
        (route) =>
          route.pathId === highlightedHop?.pathId &&
          route.ttl === highlightedHop.ttl,
      );
      if (!highlighted) return point;
      return {
        ...point,
        size: Math.max(point.size * 1.32, 0.4),
        color: point.isDestination ? "#f9a8d4" : "#f4e3b2",
      };
    });
  }

  if (density === "hubs") {
    points = points.filter(
      (p) =>
        p.isDestination ||
        p.isLastMapped ||
        new Set(p.through.map((t) => t.app)).size > 1,
    );
  }

  const originExit = origin?.status === "ready" ? origin.exit : null;
  const hasOriginCoordinates =
    originExit?.lat != null &&
    originExit.lon != null &&
    Number.isFinite(originExit.lat) &&
    Number.isFinite(originExit.lon);
  let originPoint: Point | null = null;
  if (hasOriginCoordinates && originExit) {
    originPoint = {
      lat: originExit.lat as number,
      lng: originExit.lon as number,
      label: "Primary network exit",
      size: 0.65,
      color: "#7dd3c7",
      isDestination: false,
      isLastMapped: false,
      city: originExit.city,
      country: originExit.country,
      addr: originExit.ip,
      hostname: null,
      through: [],
      dimmed: false,
      asn: originExit.asn,
      org: originExit.organization,
      geoSource: originExit.source,
      geoConfidence: originExit.confidence,
      geoNote: null,
      isOrigin: true,
    };
    points.push(originPoint);
    allLats.push(originPoint.lat);
    allLngs.push(originPoint.lng);
  }

  points = reconcilePointData(points);
  arcs = reconcileArcData(arcs);

  // Only push new arrays when geometry actually changed (lastKey already gates this)
  globe.pointsData(points);
  pruneMarkerObjectReferences(points);
  globe.customLayerData(points);
  globe.arcsData(arcs);
  scheduleArcCruiseSpeed(arcs);
  originRing = originPoint && !reduceMotion
    ? { lat: originPoint.lat, lng: originPoint.lng, kind: "origin" }
    : null;
  setActivityRings();

  if ((showLabels || selectedPathId || hoveredSegmentKey) && points.length > 0) {
    const gapLabels = arcs
      .filter(
        (arc) =>
          arc.kind === "unmapped" &&
          (arcKey(arc) === hoveredSegmentKey ||
            selectedPathId === arc.pathId),
      )
      .map(gapLabelForArc);
    currentLabelPoints = points;
    currentGapLabels = gapLabels;
    const labels = pickLabels(showLabels ? points : [], gapLabels);
    globe
      .labelsData(labels)
      .labelLat("lat")
      .labelLng("lng")
      .labelText("label")
      .labelTypeFace(optimerTypeface)
      .labelSize(labelSize)
      .labelDotRadius(labelDotRadius)
      .labelColor((d: object) => {
        if (isGapLabel(d)) return mapLabelColor("gap");
        const point = d as Point;
        const kind = point.isOrigin
          ? "origin"
          : point.isDestination
            ? "destination"
            : "city";
        return mapLabelColor(kind, point.dimmed);
      })
      .labelAltitude(labelAltitude)
      .labelResolution(4);
    // Labels have no hover/click behavior, but globe.gl otherwise includes
    // their text triangles in every pointer raycast.
    disableLabelRaycasts(labels);
  } else {
    currentLabelPoints = points;
    currentGapLabels = [];
    globe.labelsData([]);
  }

  // Auto-frame once when data first appears / bounds jump a lot; never fight the user
  maybeFrameCamera(allLats, allLngs);

  return { pathCount, hopCount, destCount };
}

function labelForDestination(path: GlobePath, hop: GlobeHop): string {
  // Prefer human city; never dump reverse-DNS spaghetti on the globe
  if (hop.city) return hop.city;
  const host = prettyHost(path.host);
  // Skip if it still looks like an IP
  if (/^\d/.test(host) || host.includes(":")) return "Destination";
  return host.length > 18 ? host.slice(0, 16) + "…" : host;
}

function arcKey(arc: Arc): string {
  return segmentKey(arc.pathId, arc.fromTtl, arc.toTtl);
}

function isArcHighlightedByHop(arc: Arc): boolean {
  return (
    highlightedHop?.pathId === arc.pathId &&
    (highlightedHop.ttl === arc.fromTtl || highlightedHop.ttl === arc.toTtl)
  );
}

function isArcEmphasized(arc: Arc): boolean {
  return segmentVisualState(
    arc.dimmed,
    hoveredSegmentKey === arcKey(arc),
    isArcHighlightedByHop(arc),
  ) === "emphasized";
}

function segmentSelection(arc: Arc): GlobeSegmentSelection {
  return {
    pathId: arc.pathId,
    app: arc.app,
    host: arc.host,
    fromTtl: arc.fromTtl,
    toTtl: arc.toTtl,
    kind: arc.kind,
    missingResponses: arc.missingResponses,
    unlocatedHops: arc.unlocatedHops,
  };
}

function segmentTooltip(arc: Arc): string {
  const selection = segmentSelection(arc);
  const title = selection.kind === "observed" ? "Observed segment" : "Unmapped span";
  let details = selection.kind === "unmapped"
    ? [
        selection.missingResponses
          ? `${selection.missingResponses} no response${selection.missingResponses === 1 ? "" : "s"}`
          : "",
        selection.unlocatedHops
          ? `${selection.unlocatedHops} location${selection.unlocatedHops === 1 ? "" : "s"} unavailable`
          : "",
      ].filter(Boolean).join(" · ")
    : "Both endpoints were mapped";
  if (!arc.dimmed && gapShouldAnimate(arc.kind, arc.traceStatus, reduceMotion)) {
    details += " · traceroute in progress";
  }
  return `<div style="font-family:'IBM Plex Mono','Cascadia Mono','SFMono-Regular',monospace;font-size:16px;line-height:1.5;padding:8px 4px;max-width:420px">
    <div style="color:${selection.kind === "unmapped" ? "#f2c66d" : "#e8e3d8"};font-weight:700">${title} · hop ${selection.fromTtl} → ${selection.toTtl}</div>
    <div style="margin-top:3px;opacity:.72">${escapeHtml(selection.app)} → ${escapeHtml(prettyHost(selection.host))}</div>
    <div style="margin-top:5px;opacity:.62">${details} · click to inspect</div>
  </div>`;
}

function gapLabelForArc(arc: Arc): GapLabel {
  const from = latLngVector(arc.startLat, arc.startLng);
  const to = latLngVector(arc.endLat, arc.endLng);
  const omega = Math.acos(clamp(dot(from, to), -1, 1));
  const position = vectorLatLng(slerp(from, to, omega, 0.5));
  return {
    ...position,
    altitude: 0.026 + Math.min(0.24, Math.max(0.025, omega * 0.12)),
    label: "?",
    isGapLabel: true,
  };
}

function isGapLabel(value: object): value is GapLabel {
  return (value as Partial<GapLabel>).isGapLabel === true;
}

function labelBaseSize(d: object): number {
  return isGapLabel(d)
    ? 1.12
    : (d as Point).isOrigin
      ? 1.04
      : (d as Point).isDestination
        ? 1.17
        : 0.9;
}

function labelSize(d: object): number {
  return labelBaseSize(d) * cameraScale;
}

function labelAltitude(d: object): number {
  const base = isGapLabel(d) ? (d as GapLabel).altitude : 0.022;
  return base * cameraScale;
}

function labelDotRadius(d: object): number {
  return (isGapLabel(d) ? 0.28 : 0) * cameraScale;
}

function setActivityRings() {
  if (!globe) return;
  globe.ringsData([originRing, arrivalRing].filter(Boolean) as ActivityRing[]);
}

function confidenceLabel(score: number | null): string {
  if (score == null) return "";
  if (score >= 0.75) return "high";
  if (score >= 0.55) return "medium";
  return "low";
}

function confidenceAlpha(score: number | null): number {
  if (score == null || score >= 0.75) return 1;
  return score >= 0.55 ? 0.78 : 0.55;
}

function sourceLabel(source: string): string {
  if (source === "mmdb") return "local MaxMind database";
  if (source === "hosted" || source === "geolite") return "Network Cartographer hosted geo";
  if (source === "geoip" || source === "ipwho") return "legacy online GeoIP";
  if (source.startsWith("rdns")) return "reverse DNS hint";
  if (source.startsWith("inferred")) return "route and latency inference";
  return source;
}

function withAlpha(color: string, alpha: number): string {
  const hex = color.replace("#", "");
  if (!/^[0-9a-f]{6}$/i.test(hex)) return color;
  const r = parseInt(hex.slice(0, 2), 16);
  const g = parseInt(hex.slice(2, 4), 16);
  const b = parseInt(hex.slice(4, 6), 16);
  return `rgba(${r},${g},${b},${alpha})`;
}

function locKey(lat: number, lon: number): string {
  return `${lat.toFixed(2)},${lon.toFixed(2)}`;
}

function wrappedLongitudeDelta(a: number, b: number): number {
  const direct = Math.abs(a - b) % 360;
  return Math.min(direct, 360 - direct);
}

function pickLabels(
  points: Point[],
  gapLabels: GapLabel[] = [],
): Array<Point | GapLabel> {
  const labeledPoints = points.map((point) => {
    if (point.isOrigin || point.isDestination) return point;
    const ttl = Math.min(...point.through.map((route) => route.ttl));
    const label = point.city || point.hostname || point.addr || `Hop ${ttl}`;
    return label === point.label ? point : { ...point, label };
  });
  const candidates = [...labeledPoints, ...gapLabels].map((label, index) => ({
    ...label,
    size: labelBaseSize(label) * cameraScale,
    priority:
      labelPriority(label) +
      (labeledPoints.length + gapLabels.length - index) * 0.0001,
  }));
  return selectNonOverlappingLabels(candidates);
}

function labelPriority(label: Point | GapLabel): number {
  if (isGapLabel(label)) return 600;
  const visibilityPriority = label.dimmed ? -1_000 : 0;
  if (label.isOrigin) return visibilityPriority + 500;
  if (label.isDestination) return visibilityPriority + 400;
  if (label.isLastMapped) return visibilityPriority + 300;
  const appCount = new Set(label.through.map((route) => route.app)).size;
  return visibilityPriority + (appCount > 1 ? 200 + appCount : 100 + label.through.length);
}

function refreshVisibleLabels() {
  if (
    !globe ||
    (currentLabelPoints.length === 0 && currentGapLabels.length === 0)
  ) {
    return;
  }
  const labels = pickLabels(showLabels ? currentLabelPoints : [], currentGapLabels);
  globe.labelsData(labels);
  disableLabelRaycasts(labels);
}

type Vector3 = { x: number; y: number; z: number };

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

function rerenderCurrentPaths() {
  if (!globe || currentPaths.length === 0) return;
  updateAllPaths(currentPaths, currentOrigin);
}

function cancelPulse() {
  motionRun += 1;
  if (pulseTimer) clearTimeout(pulseTimer);
  if (pulseClearTimer) clearTimeout(pulseClearTimer);
  if (pulseArrivalTimer) clearTimeout(pulseArrivalTimer);
  if (arrivalClearTimer) clearTimeout(arrivalClearTimer);
  pulseTimer = null;
  pulseClearTimer = null;
  pulseArrivalTimer = null;
  arrivalClearTimer = null;
  arrivalRing = null;
  setActivityRings();
  globe?.pathsData([]);
}

function playPathPreview(path: GlobePath, tone: PulsePath["tone"]) {
  pulseTimer = null;
  if (!globe || reduceMotion || document.hidden || cameraInteracting) return;
  if (tone === "selected" && selectedPathId !== path.id) return;
  const points = buildPulsePathPoints(path.hops);
  if (points.length < 2) {
    scheduleAmbientPulse();
    return;
  }
  const run = motionRun;
  const selected = tone === "selected";
  const duration = selected ? 1250 : 1500;
  globe.pathsData([{
    points,
    color: selected ? "#fff0b7" : withAlpha(path.color, 0.55),
    stroke: selected ? 0.58 : 0.34,
    dashLength: selected ? 0.026 : 0.02,
    dashGap: selected ? 0.3 : 0.48,
    duration,
    tone,
  } satisfies PulsePath]);
  const terminal = points.at(-1);
  if (terminal) {
    pulseArrivalTimer = setTimeout(() => {
      if (run !== motionRun) return;
      pulseArrivalTimer = null;
      arrivalRing = {
        lat: terminal.lat,
        lng: terminal.lng,
        kind: "arrival",
        color: selected ? "#fff0b7" : path.color,
      };
      setActivityRings();
      arrivalClearTimer = setTimeout(() => {
        if (run !== motionRun) return;
        arrivalRing = null;
        arrivalClearTimer = null;
        setActivityRings();
      }, 950);
    }, Math.round(duration * 0.82));
  }
  pulseClearTimer = setTimeout(() => {
    if (run !== motionRun) return;
    globe?.pathsData([]);
    pulseClearTimer = null;
    scheduleAmbientPulse();
  }, duration);
}

function scheduleAmbientPulse(delay = ambientDelay()) {
  if (
    pulseTimer ||
    pulseClearTimer ||
    !globe ||
    !ambientMotionAllowed(reduceMotion, document.hidden, cameraInteracting)
  ) {
    return;
  }
  const candidates = ambientRouteCandidates(
    currentPaths.map((path) => ({
      id: path.id,
      appId: path.appId,
      mappedHopCount: path.hops.filter(isLocated).length,
      path,
    })),
    selectedPathId,
    focusedApps,
  );
  const next = chooseAmbientRoute(candidates, lastAmbientPathId);
  if (!next) return;
  const run = motionRun;
  pulseTimer = setTimeout(() => {
    if (run !== motionRun) return;
    pulseTimer = null;
    const eligibleNow = ambientRouteCandidates(
      currentPaths.map((path) => ({
        id: path.id,
        appId: path.appId,
        mappedHopCount: path.hops.filter(isLocated).length,
        path,
      })),
      selectedPathId,
      focusedApps,
    );
    const current = eligibleNow.find((candidate) => candidate.id === next.id);
    if (!current) {
      scheduleAmbientPulse();
      return;
    }
    lastAmbientPathId = current.id;
    playPathPreview(current.path, "ambient");
  }, delay);
}

function detectRouteActivity(paths: GlobePath[]): GlobePath | null {
  const hadBaseline = previousPathHits.size > 0;
  const eligibleIds = new Set(ambientRouteCandidates(
    paths.map((path) => ({
      id: path.id,
      appId: path.appId,
      mappedHopCount: path.hops.filter(isLocated).length,
    })),
    selectedPathId,
    focusedApps,
  ).map((path) => path.id));
  let best: { path: GlobePath; delta: number } | null = null;

  for (const path of paths) {
    const previous = previousPathHits.get(path.id);
    const delta = previous == null
      ? (hadBaseline ? path.hits : 0)
      : Math.max(0, path.hits - previous);
    if (delta > 0 && eligibleIds.has(path.id) && (!best || delta > best.delta)) {
      best = { path, delta };
    }
  }
  previousPathHits = new Map(paths.map((path) => [path.id, path.hits]));
  return best?.path ?? null;
}

function scheduleActivityPulse(path: GlobePath) {
  if (
    !globe ||
    pulseClearTimer ||
    !ambientMotionAllowed(reduceMotion, document.hidden, cameraInteracting)
  ) {
    return;
  }
  if (pulseTimer) clearTimeout(pulseTimer);
  const run = motionRun;
  pulseTimer = setTimeout(() => {
    if (run !== motionRun) return;
    pulseTimer = null;
    const current = currentPaths.find((candidate) => candidate.id === path.id);
    if (current) {
      lastAmbientPathId = current.id;
      playPathPreview(current, selectedPathId === current.id ? "selected" : "ambient");
    } else {
      scheduleAmbientPulse();
    }
  }, 180);
}

function ambientDelay(): number {
  return 1800 + Math.round(Math.random() * 1800);
}

function framePath(path: GlobePath, duration: number) {
  if (!globe) return;
  const located = path.hops.filter(isLocated);
  if (located.length === 0) return;
  hasUserMovedCamera = false;
  const lats = located.map((hop) => hop.lat as number);
  const lngs = located.map((hop) => hop.lon as number);
  const x = lngs.reduce((sum, lng) => sum + Math.cos(lng * Math.PI / 180), 0);
  const y = lngs.reduce((sum, lng) => sum + Math.sin(lng * Math.PI / 180), 0);
  const midLat = (Math.min(...lats) + Math.max(...lats)) / 2;
  const midLng = Math.atan2(y, x) * 180 / Math.PI;
  const lngSpan = Math.max(...lngs.map((lng) => Math.abs(wrappedLongitudeDelta(lng, midLng))));
  const span = Math.max(Math.max(...lats) - Math.min(...lats), lngSpan * 2, 8);
  const altitude = Math.min(2.65, Math.max(1.25, span / 34));
  globe.pointOfView({ lat: midLat, lng: midLng, altitude }, duration);
}

function maybeFrameCamera(lats: number[], lngs: number[]) {
  if (!globe || lats.length === 0) return;
  if (hasUserMovedCamera) return;

  const minLat = Math.min(...lats);
  const maxLat = Math.max(...lats);
  const minLng = Math.min(...lngs);
  const maxLng = Math.max(...lngs);
  const boundsKey = `${minLat.toFixed(1)},${maxLat.toFixed(1)},${minLng.toFixed(1)},${maxLng.toFixed(1)}`;

  // Only reframe when bounds change meaningfully (new region of activity)
  if (lastFrameBounds === boundsKey) return;
  // If we already framed once and bounds only grew a little, skip
  if (lastFrameBounds != null) {
    // allow reframe if user hit Recenter (hasUserMovedCamera false + lastFrameBounds cleared)
  }
  lastFrameBounds = boundsKey;

  const midLat = (minLat + maxLat) / 2;
  const midLng = (minLng + maxLng) / 2;
  const span = Math.max(maxLat - minLat, maxLng - minLng, 8);
  // altitude: larger span → zoom out
  const altitude = Math.min(2.8, Math.max(1.35, span / 35));

  globe.pointOfView({ lat: midLat, lng: midLng, altitude }, 900);
}

function lighten(hex: string, amount: number): string {
  const n = hex.replace("#", "");
  if (n.length !== 6) return hex;
  const r = Math.min(255, parseInt(n.slice(0, 2), 16) + Math.round(255 * amount));
  const g = Math.min(255, parseInt(n.slice(2, 4), 16) + Math.round(255 * amount));
  const b = Math.min(255, parseInt(n.slice(4, 6), 16) + Math.round(255 * amount));
  return `#${r.toString(16).padStart(2, "0")}${g.toString(16).padStart(2, "0")}${b.toString(16).padStart(2, "0")}`;
}

function escapeHtml(s: string): string {
  return s
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

export function clearGlobe() {
  cancelPulse();
  if (arcSpeedTimer) clearTimeout(arcSpeedTimer);
  arcSpeedTimer = null;
  if (arcSpeedFrame != null) cancelAnimationFrame(arcSpeedFrame);
  arcSpeedFrame = null;
  originRing = null;
  arrivalRing = null;
  lastKey = "";
  lastFrameBounds = null;
  hasUserMovedCamera = false;
  currentPaths = [];
  currentLabelPoints = [];
  currentGapLabels = [];
  previousPathHits.clear();
  previousSegmentKeys.clear();
  pointDataCache.clear();
  arcDataCache.clear();
  hasSegmentBaseline = false;
  hoveredSegmentKey = null;
  highlightedHop = null;
  if (!globe) return;
  globe.pointsData([]);
  globe.customLayerData([]);
  disposeMarkerObjects();
  globe.arcsData([]);
  globe.pathsData([]);
  globe.labelsData([]);
  globe.ringsData([]);
}
