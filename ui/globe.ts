import Globe from "globe.gl";
import {
  ambientRouteCandidates,
  ambientMotionAllowed,
  buildPulsePathPoints,
  chooseAmbientRoute,
  classifySegment,
  gapShouldAnimate,
  isLocated,
  segmentVisualState,
  selectionAllowsMotion,
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

type OriginRing = { lat: number; lng: number };

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
  pathId: string;
  app: string;
  host: string;
  dimmed: boolean;
  stroke: number;
  fromTtl: number;
  toTtl: number;
  kind: GlobeSegmentKind;
  missingResponses: number;
  unlocatedHops: number;
  traceStatus: string;
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

const PALETTE = [
  "#e0a86a",
  "#8fb4a2",
  "#d5c07a",
  "#c98c76",
  "#9caaa2",
  "#b69ac5",
  "#d88273",
  "#82aeb1",
  "#c4a56d",
  "#8dae7f",
  "#87a1bf",
  "#bd8e9e",
  "#cf9364",
  "#789e91",
];

export function colorForKey(key: string): string {
  let h = 0;
  for (let i = 0; i < key.length; i++) {
    h = (h * 31 + key.charCodeAt(i)) >>> 0;
  }
  return PALETTE[h % PALETTE.length];
}

export function setFocusedApp(app: string | null) {
  focusedApps = new Set();
  if (app) focusedApps.add(app);
  cancelPulse();
  scheduleAmbientPulse();
  lastKey = "";
}

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

export function getFocusedApp(): string | null {
  if (focusedApps.size === 1) return [...focusedApps][0];
  return null;
}

export function getFocusedApps(): string[] {
  return [...focusedApps];
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
    .globeImageUrl("/earth-dark.jpg")
    .pointAltitude((d: object) =>
      (d as Point).isOrigin ? 0.028 : (d as Point).isDestination ? 0.018 : 0.008,
    )
    .pointRadius("size")
    .pointColor("color")
    .pointsMerge(false)
    .pointLabel((d: object) => hopTooltip(d as Point))
    .onPointClick((d: object) => {
      const p = d as Point;
      if (p.isOrigin) {
        if (currentOrigin) onOriginClick?.(currentOrigin);
        return;
      }
      const routes = [...new Map(p.through.map((route) => [route.pathId, route])).values()];
      onHopClick?.({
        lat: p.lat,
        lon: p.lng,
        city: p.city,
        country: p.country,
        addr: p.addr,
        hostname: p.hostname,
        asn: p.asn,
        org: p.org,
        geoSource: p.geoSource,
        geoConfidence: p.geoConfidence,
        geoNote: p.geoNote,
        routes,
      });
    })
    .arcColor((d: object) => {
      const a = d as Arc;
      if (a.dimmed) return ["rgba(120,140,160,0.1)", "rgba(120,140,160,0.06)"];
      if (a.kind === "unmapped") {
        return isArcEmphasized(a) ? "#f2c66d" : "rgba(217,185,110,0.76)";
      }
      return a.color;
    })
    .arcStroke((d: object) => {
      const arc = d as Arc;
      return isArcEmphasized(arc) ? Math.max(0.66, arc.stroke * 1.65) : arc.stroke;
    })
    .arcAltitudeAutoScale(0.26)
    .arcDashLength((d: object) => ((d as Arc).kind === "unmapped" ? 0.09 : 1))
    .arcDashGap((d: object) => ((d as Arc).kind === "unmapped" ? 0.055 : 0))
    .arcDashAnimateTime((d: object) => {
      const arc = d as Arc;
      return !arc.dimmed && gapShouldAnimate(arc.kind, arc.traceStatus, reduceMotion)
        ? 2800
        : 0;
    })
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
    .pathStroke((d: object) => (d as PulsePath).stroke)
    .pathDashLength((d: object) => (d as PulsePath).dashLength)
    .pathDashGap((d: object) => (d as PulsePath).dashGap)
    .pathDashAnimateTime((d: object) => (d as PulsePath).duration)
    .pathTransitionDuration(0)
    .ringLat("lat")
    .ringLng("lng")
    .ringColor(() => (time: number) => `rgba(94,234,212,${Math.max(0, 0.42 - time * 0.34)})`)
    .ringMaxRadius(3.2)
    .ringPropagationSpeed(1.25)
    .ringRepeatPeriod(1800);

  const controls = globe.controls();
  controls.autoRotate = false;
  controls.enableDamping = true;
  controls.dampingFactor = 0.08;
  controls.minDistance = 120;
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
  controls.addEventListener("end", () => {
    cameraInteracting = false;
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
    if (!document.hidden) scheduleAmbientPulse();
  });

  globe.pointOfView({ lat: 30, lng: -40, altitude: 1.9 }, 0);

  resizeGlobe(container);
  const ro = new ResizeObserver(() => resizeGlobe(container));
  ro.observe(container);
  return globe;
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
        const rtt = r.rttMs != null ? `${r.rttMs.toFixed(0)}ms` : "—";
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
          <span style="opacity:0.65;font-size:11px">${routes.length} path${routes.length > 1 ? "s" : ""}</span>
        </div>
        ${destLines}${more}
      </div>`);
  }

  const addr = p.hostname || p.addr || "";
  const addrLine = addr
    ? `<div style="opacity:0.7;font-family:ui-monospace,monospace;font-size:11px;margin-top:2px">${escapeHtml(addr)}</div>`
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

  return `<div style="font-family:system-ui;font-size:12px;line-height:1.4;padding:4px 2px;max-width:280px">
    <div>${kind}</div>
    <div style="margin-top:4px;font-weight:600">${escapeHtml(city)}</div>
    ${addrLine}${network}${evidence}${note}
    <div style="margin-top:8px;padding-top:6px;border-top:1px solid rgba(255,255,255,0.12);font-size:11px;opacity:0.7">
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
  return `<div style="font-family:ui-monospace,monospace;font-size:12px;line-height:1.45;padding:5px 3px;max-width:280px">
    <div style="color:#7dd3c7;font-weight:700;letter-spacing:.06em;text-transform:uppercase;font-size:10px">Primary network exit</div>
    <div style="margin-top:5px;font-weight:650">${escapeHtml(place)}</div>
    <div style="opacity:.78;margin-top:2px">${escapeHtml(network)}</div>
    ${exit.ip ? `<div style="opacity:.62;margin-top:2px">${escapeHtml(exit.ip)}</div>` : ""}
    <div style="margin-top:8px;padding-top:6px;border-top:1px solid rgba(255,255,255,.12);opacity:.68;font-size:11px">Where the public internet sees this connection · click for evidence</div>
  </div>`;
}

/** Short readable host for UI — drop PTR junk and ultra-long labels. */
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

export function updateAllPaths(paths: GlobePath[], origin: NetworkOrigin | null = null): {
  pathCount: number;
  hopCount: number;
  destCount: number;
} {
  currentPaths = paths;
  scheduleAmbientPulse();
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
  const arcs: Arc[] = [];
  const allLats: number[] = [];
  const allLngs: number[] = [];

  for (const path of paths) {
    const dimmed = selectedPathId
      ? path.id !== selectedPathId
      : focusedApps.size > 0 && !focusedApps.has(path.appId);
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
          existing.size = Math.max(existing.size, 0.5);
          if (!dimmed) {
            existing.color = "#f9a8d4";
            existing.label = labelForDestination(path, h);
          }
        }
        if (isLastMapped && !path.reachedTarget && !existing.isDestination) {
          existing.isLastMapped = true;
          existing.size = Math.max(existing.size, 0.3);
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
          size: isEnd ? 0.5 : i === 0 ? 0.28 : 0.16,
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
      const segment = classifySegment(path.hops, a.ttl, b.ttl);
      arcs.push({
        startLat: a.lat as number,
        startLng: a.lon as number,
        endLat: b.lat as number,
        endLng: b.lon as number,
        color: dimmed
          ? ["rgba(100,120,140,0.08)", "rgba(100,120,140,0.04)"]
          : bIsTarget
            ? [path.color, "#f9a8d4"]
            : [path.color, lighten(path.color, 0.15)],
        pathId: path.id,
        app: path.app,
        host: path.host,
        dimmed,
        stroke: dimmed ? 0.12 : bIsTarget ? 0.55 : isLastArc ? 0.42 : 0.35,
        fromTtl: a.ttl,
        toTtl: b.ttl,
        traceStatus: path.status,
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
        size: Math.max(p.size, 0.5),
      };
    }
    if (p.isLastMapped) {
      const alpha = confidenceAlpha(p.geoConfidence);
      return {
        ...p,
        color: withAlpha("#f2c66d", alpha),
        size: Math.max(p.size, 0.3),
      };
    }
    const uniqueApps = new Set(p.through.map((t) => t.app));
    if (uniqueApps.size > 1) {
      return { ...p, color: "#cbd5e1", size: Math.max(p.size, 0.24) };
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
        size: Math.max(point.size * 1.55, 0.34),
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
      size: 0.62,
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

  // Only push new arrays when geometry actually changed (lastKey already gates this)
  globe.pointsData(points);
  globe.arcsData(arcs);
  globe.ringsData(
    originPoint && !reduceMotion
      ? ([{ lat: originPoint.lat, lng: originPoint.lng }] satisfies OriginRing[])
      : [],
  );

  if ((showLabels || selectedPathId || hoveredSegmentKey) && points.length > 0) {
    const labels: Array<Point | GapLabel> = showLabels ? pickLabels(points) : [];
    const gapLabels = arcs
      .filter(
        (arc) =>
          arc.kind === "unmapped" &&
          (arcKey(arc) === hoveredSegmentKey ||
            selectedPathId === arc.pathId),
      )
      .map(gapLabelForArc);
    labels.push(...gapLabels);
    globe
      .labelsData(labels)
      .labelLat("lat")
      .labelLng("lng")
      .labelText("label")
      // Globe labels use angular units, so values around 1 become enormous
      // when the camera frames a dense metro area. Keep them secondary to
      // the nodes and routes they describe.
      .labelSize((d: object) => {
        if (isGapLabel(d)) return 0.46;
        const point = d as Point;
        return point.isOrigin ? 0.44 : point.isDestination ? 0.48 : 0.34;
      })
      .labelDotRadius((d: object) => (isGapLabel(d) ? 0.24 : 0))
      .labelColor((d: object) =>
        isGapLabel(d)
          ? "rgba(242,198,109,0.96)"
          : (d as Point).isOrigin
          ? "rgba(125,211,199,0.94)"
          : (d as Point).isDestination
          ? "rgba(249,168,212,0.84)"
          : "rgba(226,232,240,0.58)",
      )
      .labelAltitude((d: object) => (isGapLabel(d) ? (d as GapLabel).altitude : 0.018))
      .labelResolution(3);
  } else {
    globe.labelsData([]);
  }

  // Auto-frame once when data first appears / bounds jump a lot — never fight the user
  maybeFrameCamera(allLats, allLngs);

  return { pathCount, hopCount, destCount };
}

function labelForDestination(path: GlobePath, hop: GlobeHop): string {
  // Prefer human city — never dump reverse-DNS spaghetti on the globe
  if (hop.city) return hop.city;
  const host = prettyHost(path.host);
  // Skip if it still looks like an IP
  if (/^\d/.test(host) || host.includes(":")) return "Destination";
  return host.length > 18 ? host.slice(0, 16) + "…" : host;
}

function arcKey(arc: Arc): string {
  return `${arc.pathId}:${arc.fromTtl}-${arc.toTtl}`;
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
  return `<div style="font-family:ui-monospace,monospace;font-size:11px;line-height:1.45;padding:4px 2px;max-width:270px">
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

function pickLabels(points: Point[]): Point[] {
  // Labels are decoration; nodes remain visible, hoverable, and clickable.
  // Rank useful candidates, then reserve geographic space for each accepted
  // label. This handles both duplicate city names and different names inside
  // one metro area (for example São Paulo + Guarulhos).
  const maxLabels = points.length > 80 ? 10 : points.length > 35 ? 12 : 16;
  const maxTransitLabels = 4;
  const bestByName = new Map<string, Point>();

  for (const point of points) {
    if (point.dimmed) continue;

    const label = point.isOrigin
      ? point.label
      : point.isDestination
        ? point.label
        : point.city || "";
    if (!label || isNoisyLabel(label)) continue;

    const candidate = label === point.label ? point : { ...point, label };
    const key = normalizeLabel(label);
    const existing = bestByName.get(key);
    if (!existing || labelPriority(candidate) > labelPriority(existing)) {
      bestByName.set(key, candidate);
    }
  }

  const candidates = [...bestByName.values()].sort(
    (a, b) => labelPriority(b) - labelPriority(a),
  );
  const accepted: Point[] = [];
  let transitLabels = 0;

  for (const candidate of candidates) {
    if (!candidate.isDestination && transitLabels >= maxTransitLabels) continue;
    if (accepted.some((placed) => labelsCollide(candidate, placed))) continue;

    accepted.push(candidate);
    if (!candidate.isDestination) transitLabels += 1;
    if (accepted.length >= maxLabels) break;
  }

  return accepted;
}

function labelPriority(point: Point): number {
  const appCount = new Set(point.through.map((route) => route.app)).size;
  const destinationRoutes = point.through.filter(
    (route) => route.isDestination,
  ).length;
  // Destinations win, then shared network hubs, then route volume. Prefer a
  // concise name in a tie because it occupies less map space.
  return (
    (point.isOrigin ? 20_000 : point.isDestination ? 10_000 : 0) +
    destinationRoutes * 100 +
    appCount * 20 +
    point.through.length -
    point.label.length * 0.05
  );
}

function normalizeLabel(label: string): string {
  return label
    .normalize("NFKD")
    .replace(/[\u0300-\u036f]/g, "")
    .toLocaleLowerCase()
    .replace(/[^a-z0-9]/g, "");
}

function labelsCollide(a: Point, b: Point): boolean {
  // Approximate each label's footprint in angular map space. Longitude
  // degrees contract toward the poles, hence the cosine correction.
  const midLat = ((a.lat + b.lat) / 2) * (Math.PI / 180);
  const dx =
    Math.abs(wrappedLongitudeDelta(a.lng, b.lng)) *
    Math.max(0.28, Math.cos(midLat));
  const dy = Math.abs(a.lat - b.lat);
  const halfWidth =
    (labelAngularWidth(a.label) + labelAngularWidth(b.label)) / 2;

  return dx < halfWidth + 1.2 && dy < 2.6;
}

function labelAngularWidth(label: string): number {
  return Math.min(9, Math.max(3, label.length * 0.34 + 1.4));
}

function wrappedLongitudeDelta(a: number, b: number): number {
  const direct = Math.abs(a - b) % 360;
  return Math.min(direct, 360 - direct);
}

function isNoisyLabel(s: string): boolean {
  if (!s || s.length < 2) return true;
  if (/^\d/.test(s)) return true;
  if (s.includes(":") && s.split(":").length > 2) return true;
  // Mostly hex / random
  if (/^[0-9a-f.-]{10,}$/i.test(s)) return true;
  if ((s.match(/\d/g) || []).length > s.length * 0.4) return true;
  return false;
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
  pulseTimer = null;
  pulseClearTimer = null;
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
  const duration = selected ? 1200 : 1600;
  globe.pathsData([{
    points,
    color: selected ? "#fff0b7" : withAlpha(path.color, 0.55),
    stroke: selected ? 0.72 : 0.42,
    dashLength: selected ? 0.045 : 0.032,
    dashGap: selected ? 0.955 : 0.968,
    duration,
    tone,
  } satisfies PulsePath]);
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

function ambientDelay(): number {
  return 4000 + Math.round(Math.random() * 3000);
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
  lastKey = "";
  lastFrameBounds = null;
  hasUserMovedCamera = false;
  currentPaths = [];
  hoveredSegmentKey = null;
  highlightedHop = null;
  if (!globe) return;
  globe.pointsData([]);
  globe.arcsData([]);
  globe.pathsData([]);
  globe.labelsData([]);
  globe.ringsData([]);
}
