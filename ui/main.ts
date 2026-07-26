import {
  forceTrace,
  getVersion,
  invoke,
  listen,
  listenToStreamStatus,
  type StreamStatus,
} from "./api";
import {
  clearGlobe,
  colorForKey,
  initGlobe,
  recenterOnData,
  setDensity,
  setFocusedApps,
  setHighlightedHop,
  setHopClickHandler,
  setLabelsVisible,
  setOriginClickHandler,
  setSegmentClickHandler,
  setSelectedPath,
  updateAllPaths,
  type GlobePath,
  type GlobeSegmentSelection,
  type HopSelection,
  type NetworkOrigin,
} from "./globe";
import { mountOnboarding } from "./onboarding";
import { shouldPresentTransition, transitionCopy } from "./network-transition";
import { RouteInspector } from "./route-inspector";

type HopDto = {
  ttl: number;
  addr: string | null;
  rttMs: number | null;
  hostname: string | null;
  lat: number | null;
  lon: number | null;
  city: string | null;
  country: string | null;
  geoSource: string | null;
  geoConfidence: number | null;
  geoNote: string | null;
  asn?: number | null;
  org?: string | null;
};

type TraceDto = {
  status: string;
  freshness?: "fresh" | "refreshing" | "stale";
  label: string;
  hops: HopDto[];
  error: string | null;
  reachedTarget: boolean;
  targetRttMs: number | null;
};

type DestDto = {
  host: string;
  displayHost?: string;
  ip: string;
  port: number;
  protocol: string;
  hits: number;
  lastSeenSecs: number;
  sni?: string | null;
  domain?: string | null;
  domainSource?: "tls-sni" | "os-dns" | "reverse-dns" | "ip";
  domainConfidence?: "exact" | "high" | "low" | "none";
  domainAlternativesCount?: number;
  processIds?: string[];
  asn?: number | null;
  org?: string | null;
  pathChanged?: boolean;
  trace: TraceDto;
};

type ProcessDto = {
  id: string;
  pid: number;
  startTime: number;
  name: string;
  path?: string | null;
  parentPid?: number | null;
  isAppRoot: boolean;
};

type AppDto = {
  id: string;
  name: string;
  iconUrl?: string | null;
  path?: string | null;
  pids: number[];
  processes?: ProcessDto[];
  destCount: number;
  hits: number;
  hitsPerSec?: number;
  activity?: number;
  currentConnections?: number;
  newConnectionsPerSec?: number;
  traffic?: TrafficRateDto | null;
  destinations: DestDto[];
};

type TrafficRateDto = {
  rxBytesPerSec: number;
  txBytesPerSec: number;
  totalBytesPerSec: number;
  sampleWindowMs: number;
  source: string;
};

type TrafficGroupDto = {
  name: string;
  currentConnections: number;
  connections: number;
  destinations: DestDto[];
};

type SettingsDto = {
  settingsVersion?: number;
  externalOnly: boolean;
  includeUdp: boolean;
  tracesEnabled: boolean;
  pollIntervalMs: number;
  geoLocalOnly?: boolean;
  showLowConfidence?: boolean;
  confidenceMin?: number;
  globeDensity?: string;
  identifyDomains?: boolean;
  historyEnabled?: boolean;
  enhancedMonitoring?: boolean;
  privacyAccepted?: boolean;
};

type SnapshotDto = {
  apps: AppDto[];
  appCount: number;
  destCount: number;
  liveConnections: number;
  missingPid: number;
  attribution?: {
    direct: number;
    recovered: number;
    unattributed: number;
    ambiguous: number;
    ownerGone: number;
    accessLimited?: number;
    ratio: number;
  };
  capabilities?: { trafficRates: boolean };
  unattributed?: TrafficGroupDto | null;
  monitoring?: { mode: string; status: string; message: string };
  udpMonitoring?: {
    enabled: boolean;
    coverage: "connected";
    status: "disabled" | "ready" | "degraded" | "unavailable";
    message: string;
  };
  collection?: {
    mode: string;
    source: string;
    capturesOpens: boolean;
    capturesCloses: boolean;
    droppedEvents: number;
    status: string;
    message: string;
    udpRemote?: boolean;
    accessLimited?: number;
    truncatedSockets?: number;
    pollPhase?: "active" | "warm" | "idle";
    effectivePollIntervalMs?: number;
    observedOpens?: number;
    observedCloses?: number;
    recoveredOwners?: number;
    unattributedOwnerGone?: number;
    unattributedAmbiguous?: number;
    unattributedAccessLimited?: number;
  };
  destinationNaming?: {
    enabled: boolean;
    status: string;
    sources: string[];
    message: string;
  };
  externalOnly: boolean;
  includeUdp: boolean;
  tracesEnabled: boolean;
  traceStats: { queued: number; running: number; done: number; failed: number };
  geoBackend?: string;
  geoMmdb?: boolean;
  geoAsnMmdb?: boolean;
  settings?: SettingsDto;
  networkOrigin?: NetworkOrigin;
};

type AppGroup = {
  id: string;
  name: string;
  color: string;
  iconUrl: string | null;
  paths: GlobePath[];
  totalDests: number;
  traced: number;
  mappedHops: number;
  activity: number;
  currentConnections: number;
  newConnectionsPerSec: number;
  traffic: TrafficRateDto | null;
  processes: ProcessDto[];
};

let snapshot: SnapshotDto | null = null;
let filter = "";
let globeReady = false;
const expanded = new Set<string>();
const focused = new Set<string>();
let pendingSnap: SnapshotDto | null = null;
let paintScheduled = false;
let lastPaintAt = 0;
const MIN_PAINT_MS = 1200;
let lastSidebarSig = "";
let lastHeaderSig = "";
let streamStatus: StreamStatus = "connecting";
let reconnectTimer: number | null = null;
let streamHasOpened = false;
let lastMonitorError: string | null = null;
let firstRouteRevealed = false;
let revealTimer: number | null = null;
let transitionTimer: number | null = null;
let shownTransitionId = 0;
let shownTransitionStatus = "";

const UNATTRIBUTED_NAME = "Unattributed traffic";
const UNATTRIBUTED_ID = "__unattributed__";
const UNATTRIBUTED_COLOR = "#8a8680";

const el = {
  globe: document.getElementById("globe")!,
  globeStatus: document.getElementById("globe-status")!,
  sessionSummary: document.getElementById("session-summary")!,
  networkOrigin: document.getElementById("network-origin") as HTMLButtonElement,
  networkOriginPlace: document.getElementById("network-origin-place")!,
  networkOriginAssessment: document.getElementById("network-origin-assessment")!,
  traceProgress: document.getElementById("trace-progress")!,
  traceProgressTitle: document.getElementById("trace-progress-title")!,
  traceProgressDetail: document.getElementById("trace-progress-detail")!,
  traceProgressCount: document.getElementById("trace-progress-count")!,
  appList: document.getElementById("app-list")!,
  sidebarSub: document.getElementById("sidebar-sub")!,
  btnClearFocus: document.getElementById("btn-clear-focus") as HTMLButtonElement,
  statApps: document.getElementById("stat-apps")!,
  statPaths: document.getElementById("stat-paths")!,
  statHops: document.getElementById("stat-hops")!,
  statTraces: document.getElementById("stat-traces")!,
  statGeo: document.getElementById("stat-geo")!,
  status: document.getElementById("status-msg")!,
  healthDot: document.getElementById("health-dot")!,
  healthOverall: document.getElementById("health-overall")!,
  healthCollector: document.getElementById("health-collector")!,
  healthAttribution: document.getElementById("health-attribution")!,
  healthRoutes: document.getElementById("health-routes")!,
  healthGeo: document.getElementById("health-geo")!,
  healthStream: document.getElementById("health-stream")!,
  healthDetail: document.getElementById("health-detail")!,
  filter: document.getElementById("filter") as HTMLInputElement,
  togExternal: document.getElementById("tog-external") as HTMLInputElement,
  togUdp: document.getElementById("tog-udp") as HTMLInputElement,
  udpStatus: document.getElementById("udp-status")!,
  togTraces: document.getElementById("tog-traces") as HTMLInputElement,
  togLabels: document.getElementById("tog-labels") as HTMLInputElement,
  togLocalGeo: document.getElementById("tog-local-geo") as HTMLInputElement,
  togHistory: document.getElementById("tog-history") as HTMLInputElement,
  togEnhanced: document.getElementById("tog-enhanced") as HTMLInputElement,
  trafficSetting: document.getElementById("traffic-setting")!,
  trafficStatus: document.getElementById("traffic-status")!,
  togDomains: document.getElementById("tog-domains") as HTMLInputElement,
  domainsStatus: document.getElementById("domains-status")!,
  selDensity: document.getElementById("sel-density") as HTMLSelectElement,
  btnReset: document.getElementById("btn-reset")!,
  btnTraceAll: document.getElementById("btn-trace-all")!,
  traceAllLabel: document.getElementById("trace-all-label")!,
  btnRecenter: document.getElementById("btn-recenter")!,
  toast: document.getElementById("toast")!,
  onboardingHost: document.getElementById("onboarding-host")!,
  appVersion: document.getElementById("app-version")!,
  aboutModal: document.getElementById("about-modal")!,
  aboutVersion: document.getElementById("about-version")!,
  btnAbout: document.getElementById("btn-about") as HTMLButtonElement,
  btnAboutClose: document.getElementById("btn-about-close") as HTMLButtonElement,
  inspector: document.getElementById("route-inspector")!,
  firstRouteReveal: document.getElementById("first-route-reveal")!,
  firstRouteTitle: document.getElementById("first-route-title")!,
  firstRouteDetail: document.getElementById("first-route-detail")!,
  networkTransition: document.getElementById("network-transition") as HTMLButtonElement,
  transitionTitle: document.getElementById("transition-title")!,
  transitionDetail: document.getElementById("transition-detail")!,
};

const routeInspector = new RouteInspector(el.inspector, {
  onClose: () => {
    setSelectedPath(null);
    paint(true);
  },
  onSelectRoute: (routeId, instant) => {
    setSelectedPath(routeId, { frame: true, preview: true, instant });
    paint(true);
  },
  onHighlightHop: (hop) => setHighlightedHop(hop),
  onTraceRoute: async (path) => {
    try {
      await forceTrace(path.ip);
      showToast(`Traceroute queued for ${path.host}`);
    } catch (error) {
      showToast(`Could not start traceroute: ${String(error)}`);
      throw error;
    }
  },
});

let appVersion = "0.1.0";

function destName(d: DestDto): string {
  return d.displayHost || d.sni || d.host || d.ip;
}

function matchesFilter(appName: string, dest: DestDto): boolean {
  const q = filter.trim().toLowerCase();
  if (!q) return true;
  if (appName.toLowerCase().includes(q)) return true;
  if (destName(dest).toLowerCase().includes(q)) return true;
  if (dest.ip.includes(q)) return true;
  if (dest.org?.toLowerCase().includes(q)) return true;
  return dest.trace.hops.some(
    (h) =>
      (h.city && h.city.toLowerCase().includes(q)) ||
      (h.org && h.org.toLowerCase().includes(q)) ||
      (h.addr && h.addr.includes(q)),
  );
}

function finalRtt(hops: HopDto[]): number | null {
  for (let i = hops.length - 1; i >= 0; i--) {
    if (hops[i].rttMs != null) return hops[i].rttMs;
  }
  return null;
}

function routeId(ownerId: string, dest: DestDto): string {
  return `${ownerId}|${dest.protocol}|${dest.ip}|${dest.port}`;
}

function pathForDestination(
  ownerId: string,
  ownerName: string,
  appIconUrl: string | null,
  color: string,
  dest: DestDto,
): GlobePath {
  return {
    id: routeId(ownerId, dest),
    appId: ownerId,
    app: ownerName,
    appIconUrl,
    host: destName(dest),
    destinationOrg: dest.org,
    ip: dest.ip,
    port: dest.port,
    protocol: dest.protocol,
    domainSource: dest.domainSource,
    domainConfidence: dest.domainConfidence,
    domainAlternativesCount: dest.domainAlternativesCount,
    hits: dest.hits,
    color,
    hops: dest.trace.hops.map((h) => ({
      ttl: h.ttl,
      addr: h.addr,
      rttMs: h.rttMs,
      hostname: h.hostname,
      lat: h.lat,
      lon: h.lon,
      city: h.city,
      country: h.country,
      geoSource: h.geoSource,
      geoConfidence: h.geoConfidence,
      geoNote: h.geoNote,
      asn: h.asn,
      org: h.org,
    })),
    status: dest.trace.status,
    freshness: dest.trace.freshness ?? "fresh",
    rttMs: finalRtt(dest.trace.hops),
    reachedTarget: !!dest.trace.reachedTarget,
    targetRttMs: dest.trace.targetRttMs ?? null,
    error: dest.trace.error,
  };
}

function collectPaths(applyFilter = true): GlobePath[] {
  if (!snapshot) return [];
  const paths: GlobePath[] = [];
  for (const app of snapshot.apps) {
    for (const dest of app.destinations) {
      if (applyFilter && !matchesFilter(app.name, dest)) continue;
      paths.push(
        pathForDestination(
          app.id,
          app.name,
          app.iconUrl ?? null,
          colorForKey(app.id),
          dest,
        ),
      );
    }
  }
  for (const dest of snapshot.unattributed?.destinations ?? []) {
    if (applyFilter && !matchesFilter(UNATTRIBUTED_NAME, dest)) continue;
    paths.push(
      pathForDestination(
        UNATTRIBUTED_ID,
        UNATTRIBUTED_NAME,
        null,
        UNATTRIBUTED_COLOR,
        dest,
      ),
    );
  }
  paths.sort(
    (a, b) =>
      a.app.localeCompare(b.app) || b.hits - a.hits || a.host.localeCompare(b.host),
  );
  return paths;
}

function collectAppGroups(paths: GlobePath[]): AppGroup[] {
  if (!snapshot) return [];
  const byId = new Map(paths.map((p) => [p.id, p]));
  const groups: AppGroup[] = [];
  for (const app of snapshot.apps) {
    const color = colorForKey(app.id);
    const group: AppGroup = {
      id: app.id,
      name: app.name,
      color,
      iconUrl: app.iconUrl ?? null,
      paths: [],
      totalDests: 0,
      traced: 0,
      mappedHops: 0,
      activity: app.activity ?? app.hitsPerSec ?? 0,
      currentConnections: app.currentConnections ?? 0,
      newConnectionsPerSec: app.newConnectionsPerSec ?? app.hitsPerSec ?? 0,
      traffic: app.traffic ?? null,
      processes: app.processes ?? [],
    };
    for (const dest of app.destinations) {
      if (!matchesFilter(app.name, dest)) continue;
      group.totalDests += 1;
      const id = routeId(app.id, dest);
      const path = byId.get(id);
      if (path) {
        group.paths.push(path);
        if (path.status === "done") group.traced += 1;
        group.mappedHops += path.hops.filter((h) => h.lat != null).length;
      } else {
        group.paths.push({
          id,
          appId: app.id,
          app: app.name,
          appIconUrl: app.iconUrl ?? null,
          host: destName(dest),
          destinationOrg: dest.org,
          ip: dest.ip,
          port: dest.port,
          protocol: dest.protocol,
          domainSource: dest.domainSource,
          domainConfidence: dest.domainConfidence,
          domainAlternativesCount: dest.domainAlternativesCount,
          hits: dest.hits,
          color,
          hops: [],
          status: dest.trace.status,
          freshness: dest.trace.freshness ?? "fresh",
          rttMs: finalRtt(dest.trace.hops),
          reachedTarget: !!dest.trace.reachedTarget,
          targetRttMs: dest.trace.targetRttMs ?? null,
          error: dest.trace.error,
        });
        if (dest.trace.status === "done") group.traced += 1;
      }
    }
    if (group.totalDests > 0) groups.push(group);
  }
  groups.sort(
    (a, b) => b.activity - a.activity || b.paths.length - a.paths.length,
  );
  return groups;
}

function sidebarSignature(groups: AppGroup[]): string {
  const exp = [...expanded].sort().join(",");
  const foc = [...focused].sort().join(",");
  const body = groups
    .map((g) => {
      const dests = g.paths
        .map(
          (p) =>
            `${p.host}:${p.port}:${p.status}:${p.domainSource}:${p.domainConfidence}:${p.domainAlternativesCount}:${p.hops.filter((h) => h.lat != null).length}`,
        )
        .join(";");
      const processes = g.processes.map((process) => process.id).sort().join(",");
      return `${g.id}|${g.name}|${g.iconUrl ?? ""}|${g.traced}/${g.totalDests}|${g.activity.toFixed(1)}|${g.currentConnections}|${processes}|${dests}`;
    })
    .join("||");
  const unattributed = snapshot?.unattributed;
  return `${foc}#${exp}#${filter}#${body}#${unattributed?.connections ?? 0}`;
}

function currentSettings(): SettingsDto {
  return {
    settingsVersion: 3,
    externalOnly: el.togExternal.checked,
    includeUdp: el.togUdp.checked,
    tracesEnabled: el.togTraces.checked,
    pollIntervalMs: 1000,
    geoLocalOnly: el.togLocalGeo.checked,
    showLowConfidence: true,
    confidenceMin: 0.45,
    globeDensity: el.selDensity.value,
    identifyDomains: el.togDomains.checked,
    historyEnabled: el.togHistory.checked,
    enhancedMonitoring: el.togEnhanced.checked,
    privacyAccepted: true,
  };
}

async function pushSettings() {
  try {
    await invoke("set_settings", { settings: currentSettings() });
  } catch {
    /* preview */
  }
}

type HealthState = "ready" | "degraded" | "unavailable" | "waiting";

function setHealthValue(element: HTMLElement, text: string, state: HealthState = "ready") {
  element.textContent = text;
  element.classList.toggle("degraded", state === "degraded");
  element.classList.toggle("unavailable", state === "unavailable");
}

function renderCapabilities(): void {
  const trafficRates = snapshot?.capabilities?.trafficRates ?? true;
  el.togEnhanced.disabled = !trafficRates;
  el.trafficSetting.classList.toggle("is-unavailable", !trafficRates);
  if (!trafficRates) {
    el.togEnhanced.checked = false;
    el.trafficStatus.textContent = "Unavailable on macOS";
  } else {
    el.trafficStatus.textContent = "Native per-app upload and download";
  }
}

function renderHealth(mappedPaths: GlobePath[]): void {
  if (!snapshot) {
    el.status.textContent = "Starting monitor…";
    el.healthDot.className = "health-dot waiting";
    setHealthValue(el.healthOverall, "Starting", "waiting");
    return;
  }

  const collection = snapshot.collection;
  const trace = snapshot.traceStats;
  const details: string[] = [];
  let overall: HealthState = "ready";

  const collectorState: HealthState = collection?.status === "unavailable"
    ? "unavailable"
    : collection?.status === "degraded"
      ? "degraded"
      : "ready";
  setHealthValue(
    el.healthCollector,
    collectorState === "ready" ? "Ready" : collectorState === "degraded" ? "Limited" : "Unavailable",
    collectorState,
  );
  if (collectorState !== "ready") overall = collectorState;
  if (collection?.status === "degraded" && collection.message) details.push(collection.message);
  if (collection?.accessLimited) details.push(`${collection.accessLimited} protected processes could not be inspected`);
  if (collection?.truncatedSockets) details.push(`${collection.truncatedSockets} socket records were truncated`);

  if (snapshot.appCount === 0) {
    setHealthValue(el.healthAttribution, "Waiting", "waiting");
  } else {
    const ratio = Math.round((snapshot.attribution?.ratio ?? 1) * 100);
    setHealthValue(el.healthAttribution, `${ratio}% identified`, ratio < 70 ? "degraded" : "ready");
  }

  if (!snapshot.tracesEnabled) {
    setHealthValue(el.healthRoutes, "Off", "waiting");
  } else if (trace.running + trace.queued > 0) {
    setHealthValue(el.healthRoutes, `${trace.running + trace.queued} mapping`, "waiting");
  } else if (mappedPaths.length > 0) {
    setHealthValue(el.healthRoutes, `${mappedPaths.length} mapped`);
  } else if (trace.failed > 0) {
    setHealthValue(el.healthRoutes, "Unavailable", "unavailable");
    overall = "unavailable";
    details.push("Traceroute did not return a usable route; connection monitoring is still live");
  } else {
    setHealthValue(el.healthRoutes, "Waiting", "waiting");
  }

  const locatedHops = mappedPaths.reduce(
    (count, path) => count + path.hops.filter((hop) => hop.lat != null && hop.lon != null).length,
    0,
  );
  if (locatedHops > 0) {
    setHealthValue(el.healthGeo, snapshot.geoBackend ?? "Ready");
  } else if (trace.done > 0) {
    setHealthValue(el.healthGeo, "Limited", "degraded");
    if (overall === "ready") overall = "degraded";
  } else {
    setHealthValue(el.healthGeo, "Waiting", "waiting");
  }

  const browserState: HealthState = streamStatus === "open" ? "ready" : "degraded";
  setHealthValue(
    el.healthStream,
    streamStatus === "open" ? "Connected" : streamStatus === "reconnecting" ? "Reconnecting" : "Connecting",
    browserState,
  );
  if (streamStatus === "reconnecting") {
    if (overall === "ready") overall = "degraded";
    details.push("Dashboard connection interrupted; showing the last received snapshot");
  }
  if (lastMonitorError) {
    if (overall === "ready") overall = "degraded";
    details.push(lastMonitorError);
  }

  el.status.textContent = streamStatus === "reconnecting"
    ? "Reconnecting · showing last snapshot"
    : `Near-live · ${snapshot.appCount} ${snapshot.appCount === 1 ? "app" : "apps"} · ${mappedPaths.length} ${mappedPaths.length === 1 ? "route" : "routes"}`;
  el.healthDot.className = `health-dot ${overall}`;
  setHealthValue(
    el.healthOverall,
    overall === "ready" ? "Ready" : overall === "degraded" ? "Limited" : "Unavailable",
    overall,
  );
  el.healthDetail.hidden = details.length === 0;
  el.healthDetail.textContent = [...new Set(details)].join(" · ");
}

function renderSessionExperience(mappedPaths: GlobePath[]): void {
  if (!snapshot || mappedPaths.length === 0) {
    el.sessionSummary.hidden = true;
    return;
  }

  const organizations = new Set<string>();
  for (const app of snapshot.apps) {
    for (const destination of app.destinations) {
      if (destination.org && destination.org.toLowerCase() !== "unknown") {
        organizations.add(destination.org);
      }
    }
  }
  const countries = new Set(
    mappedPaths.flatMap((path) => path.hops.map((hop) => hop.country).filter(Boolean) as string[]),
  );
  const parts = [`${snapshot.appCount} ${snapshot.appCount === 1 ? "app" : "apps"}`];
  if (organizations.size > 0) parts.push(`${organizations.size} destination ${organizations.size === 1 ? "network" : "networks"}`);
  if (countries.size > 0) parts.push(`${countries.size} mapped ${countries.size === 1 ? "country" : "countries"}`);
  el.sessionSummary.textContent = parts.join(" · ");
  el.sessionSummary.hidden = false;

  if (firstRouteRevealed) return;
  const candidate = mappedPaths.find(
    (path) => path.appId !== UNATTRIBUTED_ID && path.hops.filter((hop) => hop.lat != null && hop.lon != null).length >= 2,
  ) ?? mappedPaths.find((path) => path.hops.filter((hop) => hop.lat != null && hop.lon != null).length >= 2);
  if (!candidate) return;

  const located = candidate.hops.filter((hop) => hop.lat != null && hop.lon != null);
  const last = located.at(-1)!;
  const place = last.city
    ? `${last.city}${last.country ? `, ${last.country}` : ""}`
    : last.country || "Across the public internet";
  const answeredHops = candidate.hops.filter((hop) => hop.addr != null).length;
  firstRouteRevealed = true;
  el.onboardingHost.replaceChildren();
  el.networkTransition.hidden = true;
  shownTransitionStatus = "";
  if (transitionTimer != null) window.clearTimeout(transitionTimer);
  el.firstRouteTitle.textContent = `${candidate.app} → ${candidate.destinationOrg || candidate.host}`;
  el.firstRouteDetail.textContent = `${place} · ${answeredHops} answering ${answeredHops === 1 ? "hop" : "hops"}`;
  el.firstRouteReveal.hidden = false;
  if (revealTimer != null) window.clearTimeout(revealTimer);
  revealTimer = window.setTimeout(() => {
    el.firstRouteReveal.hidden = true;
    renderNetworkTransition(snapshot?.networkOrigin ?? null);
  }, 4500);
}

function renderNetworkTransition(origin: NetworkOrigin | null): void {
  const transition = origin?.transition;
  if (!shouldPresentTransition(transition, shownTransitionId) || !transition || !el.firstRouteReveal.hidden) return;

  const isNew = transition.id > shownTransitionId;
  const statusChanged = transition.status !== shownTransitionStatus;
  if (!isNew && !statusChanged) return;
  shownTransitionId = transition.id;
  shownTransitionStatus = transition.status;

  const copy = transitionCopy(transition);
  el.transitionTitle.textContent = copy.title;
  el.transitionDetail.textContent = copy.detail;

  el.networkTransition.hidden = false;
  if (transitionTimer != null) window.clearTimeout(transitionTimer);
  if (transition.status !== "detecting") {
    transitionTimer = window.setTimeout(() => {
      el.networkTransition.hidden = true;
    }, 8000);
  }
}

function paint(forceSidebar = false) {
  paintScheduled = false;
  lastPaintAt = performance.now();
  if (pendingSnap) {
    snapshot = pendingSnap;
    pendingSnap = null;
  }
  if (snapshot?.udpMonitoring) {
    el.udpStatus.textContent = snapshot.udpMonitoring.message;
  }
  if (snapshot?.destinationNaming) {
    el.domainsStatus.textContent = snapshot.destinationNaming.message;
  }

  const allPaths = collectPaths(false);
  const allMappedPaths = allPaths.filter((p) => p.status === "done" && p.hops.length > 0);
  const paths = collectPaths(true);
  const mapPaths = paths.filter((p) => p.status === "done" && p.hops.length > 0);
  const groups = collectAppGroups(paths);
  const networkOrigin = snapshot?.networkOrigin ?? null;
  setFocusedApps([...focused]);
  routeInspector.update(allPaths, networkOrigin);
  const selectedRoute = routeInspector.selectedRouteId;
  setSelectedPath(
    selectedRoute && allPaths.some((path) => path.id === selectedRoute)
      ? selectedRoute
      : null,
  );

  if (snapshot) {
    const t = snapshot.traceStats;
    const headerSig = [
      snapshot.appCount,
      snapshot.liveConnections,
      t.queued,
      t.running,
      t.done,
      t.failed,
      snapshot.tracesEnabled,
      mapPaths.length,
      snapshot.geoBackend,
      snapshot.monitoring?.status,
      snapshot.collection?.status,
      snapshot.collection?.droppedEvents,
      networkOrigin?.status,
      networkOrigin?.assessment,
      networkOrigin?.exit?.ip,
      networkOrigin?.exit?.city,
      snapshot.collection?.pollPhase,
      snapshot.collection?.effectivePollIntervalMs,
      snapshot.collection?.observedOpens,
      snapshot.collection?.observedCloses,
      snapshot.collection?.recoveredOwners,
      [...focused].join(","),
      el.selDensity.value,
    ].join("|");

    if (headerSig !== lastHeaderSig) {
      lastHeaderSig = headerSig;
      el.statApps.textContent = `${snapshot.appCount}`;
      el.statTraces.textContent = snapshot.tracesEnabled
        ? `Q${t.queued} · R${t.running} · ${t.done} done`
        : "offline";
      const tracesRemaining = t.queued + t.running;
      const tracesArePending = snapshot.tracesEnabled && tracesRemaining > 0;
      el.traceProgress.hidden = !tracesArePending;
      el.traceProgress.classList.toggle("actively-running", t.running > 0);
      el.btnTraceAll.toggleAttribute("disabled", tracesArePending);
      el.btnTraceAll.setAttribute("aria-busy", String(tracesArePending));
      el.traceAllLabel.textContent = tracesArePending
        ? `Tracing ${tracesRemaining}`
        : "Trace all";
      if (tracesArePending) {
        el.traceProgressTitle.textContent = t.running > 0
          ? "Traceroute in progress"
          : "Traceroutes queued";
        el.traceProgressDetail.textContent = `Results arrive gradually — ${tracesRemaining} ${tracesRemaining === 1 ? "route is" : "routes are"} still being measured`;
        el.traceProgressCount.textContent = t.running > 0
          ? `${t.running} active · ${t.queued} queued`
          : `${t.queued} waiting`;
      }
      if (el.statGeo) {
        const backend = snapshot.geoBackend ?? "api";
        el.statGeo.textContent = backend;
        el.statGeo.classList.toggle("accent", !!snapshot.geoMmdb);
        el.statGeo.title = snapshot.geoMmdb
          ? "Local MaxMind city DB loaded"
          : "Online geo only";
      }
      el.btnClearFocus.hidden = focused.size === 0;
      const focusedNames = snapshot.apps
        .filter((app) => focused.has(app.id))
        .map((app) => app.name);
      el.sidebarSub.textContent =
        focused.size === 0
          ? "Select an app to isolate traffic"
          : `Isolating ${focusedNames.join(", ")}`;
      renderNetworkOrigin(networkOrigin);
    }
  }

  let pathCount = 0;
  let hopCount = 0;
  let destCount = 0;
  if (globeReady) {
    const stats = updateAllPaths(mapPaths, networkOrigin);
    pathCount = stats.pathCount;
    hopCount = stats.hopCount;
    destCount = stats.destCount;
  }
  el.statPaths.textContent = `${pathCount}`;
  el.statHops.textContent = `${hopCount}`;
  el.globeStatus.textContent = pathCount > 0
    ? `${pathCount} paths · ${destCount} destinations · ${hopCount} hops`
    : allMappedPaths.length > 0
      ? "No mapped routes match this view"
    : !snapshot
      ? "Starting monitor…"
      : snapshot.appCount === 0
        ? "Watching for traffic · try websites hosted in a few different countries"
        : snapshot.destCount === 0
          ? "Finding destinations · new activity appears after a short delay"
          : snapshot.tracesEnabled
            ? "Mapping routes · traceroutes take a little time"
            : "Connections detected · route mapping is off";
  renderCapabilities();
  renderHealth(allMappedPaths);
  renderSessionExperience(allMappedPaths);
  renderNetworkTransition(networkOrigin);

  const sig = sidebarSignature(groups);
  if (forceSidebar || sig !== lastSidebarSig) {
    lastSidebarSig = sig;
    const scrollTop = el.appList.scrollTop;
    renderSidebar(groups);
    el.appList.scrollTop = scrollTop;
  }
}

function renderNetworkOrigin(origin: NetworkOrigin | null) {
  el.networkOrigin.classList.toggle("locating", !origin || origin.status === "locating");
  el.networkOrigin.classList.toggle("unavailable", origin?.status === "unavailable");
  const exit = origin?.exit;
  if (!origin || origin.status === "locating") {
    el.networkOriginPlace.textContent = "Locating…";
    el.networkOriginAssessment.textContent = "Inspecting route";
    return;
  }
  if (!exit) {
    el.networkOriginPlace.textContent = "Unavailable";
    el.networkOriginAssessment.textContent = assessmentText(origin.assessment);
    return;
  }
  el.networkOriginPlace.textContent = exit.city
    ? `${exit.city}${exit.country ? `, ${exit.country}` : ""}`
    : exit.ip || "Location unavailable";
  el.networkOriginAssessment.textContent = assessmentText(origin.assessment);
  el.networkOrigin.title = exit.organization
    ? `${exit.organization}${exit.asn ? ` · AS${exit.asn}` : ""}`
    : "Inspect primary network exit";
}

function assessmentText(assessment: NetworkOrigin["assessment"]): string {
  if (assessment === "proxy_and_tunnel") return "Proxy + tunnel signals";
  if (assessment === "proxy_configured") return "Proxy configured";
  if (assessment === "tunnel_likely") return "VPN / tunnel likely";
  if (assessment === "no_evidence") return "No VPN / proxy evidence";
  return "Evidence unavailable";
}

function schedulePaint(snap?: SnapshotDto, immediate = false) {
  if (snap) pendingSnap = snap;
  if (immediate) {
    paint(true);
    return;
  }
  if (paintScheduled) return;
  const wait = Math.max(0, MIN_PAINT_MS - (performance.now() - lastPaintAt));
  paintScheduled = true;
  window.setTimeout(() => requestAnimationFrame(() => paint(false)), wait);
}

function handleStreamStatus(next: StreamStatus): void {
  if (next === "open") {
    const reconnected = streamHasOpened;
    streamHasOpened = true;
    if (reconnectTimer != null) {
      window.clearTimeout(reconnectTimer);
      reconnectTimer = null;
    }
    streamStatus = "open";
    if (reconnected) {
      void invoke<SnapshotDto>("get_snapshot")
        .then((fresh) => schedulePaint(fresh, true))
        .catch(() => undefined);
    }
    paint(false);
    return;
  }

  if (next === "reconnecting") {
    if (reconnectTimer != null) return;
    reconnectTimer = window.setTimeout(() => {
      reconnectTimer = null;
      streamStatus = "reconnecting";
      paint(false);
    }, 750);
    return;
  }

  streamStatus = "connecting";
  paint(false);
}

function statusBadge(status: string): string {
  if (status === "done") return `<span class="badge ok">observed</span>`;
  if (status === "running") return `<span class="badge run">tracing…</span>`;
  if (status === "queued") return `<span class="badge queue">queued</span>`;
  if (status === "deferred") return `<span class="badge">not auto-traced</span>`;
  if (status === "failed") return `<span class="badge fail">fail</span>`;
  return `<span class="badge">${escapeHtml(status)}</span>`;
}

function renderSidebar(groups: AppGroup[]) {
  if (groups.length === 0 && !snapshot?.unattributed) {
    el.appList.innerHTML = `<div class="empty">No internet activity detected yet.<br>Open websites hosted in a few different countries, then give the map a moment.</div>`;
    return;
  }

  const applications = groups
    .map((g) => {
      const isOpen = expanded.has(g.id) || focused.has(g.id);
      const isFocused = focused.has(g.id);
      const dim = focused.size > 0 && !isFocused;
      const act = g.traffic
        ? `<span class="act"> · ↓${formatByteRate(g.traffic.rxBytesPerSec)} ↑${formatByteRate(g.traffic.txBytesPerSec)}</span>`
        : g.newConnectionsPerSec > 0.05
          ? `<span class="act"> · ${g.newConnectionsPerSec.toFixed(1)} new/s</span>`
          : "";
      const processRows = isOpen && g.processes.length
        ? `<div class="process-list" aria-label="Owning processes">${g.processes
            .slice()
            .sort((a, b) => a.pid - b.pid)
            .map(
              (process) => `<div class="process-row" title="${escapeHtml(process.path ?? process.name)}">
                <span>${escapeHtml(process.name)}</span><code>PID ${process.pid}</code>
              </div>`,
            )
            .join("")}</div>`
        : "";

      const destRows = isOpen
        ? g.paths
            .slice()
            .sort((a, b) => b.hits - a.hits)
            .map((p) => {
              const mapped = p.hops.filter((h) => h.lat != null).length;
              const lastCity =
                [...p.hops].reverse().find((h) => h.city)?.city ?? null;
              const rtt = p.rttMs != null ? `${Math.round(p.rttMs)}ms` : "—";
              const destCity = lastCity ? ` · ${escapeHtml(lastCity)}` : "";
              // org from snapshot if available
              const destMeta = snapshot?.apps
                .find((a) => a.id === g.id)
                ?.destinations.find(
                  (d) => d.ip === p.ip && d.port === p.port,
                );
              const org = destMeta?.org
                ? ` <span class="org">· ${escapeHtml(destMeta.org)}</span>`
                : "";
              const changed = destMeta?.pathChanged
                ? ` <span class="badge run">path Δ</span>`
                : "";
              const nameSource = destMeta?.domainSource
                ? ` · name: ${destMeta.domainSource}`
                : "";
              const nameConfidence = destMeta?.domainConfidence === "low"
                ? ` <span class="badge name-guess">best guess</span>`
                : "";
              const owners = g.processes.filter((process) =>
                destMeta?.processIds?.includes(process.id),
              );
              const ownerLabel = owners.length
                ? ` · ${owners.map((process) => `${process.name} (${process.pid})`).join(", ")}`
                : "";
              const marker = p.reachedTarget ? "★" : "◌";
              const traceState = p.freshness === "refreshing"
                ? "refreshing"
                : p.freshness === "stale"
                  ? "last known"
                  : p.reachedTarget
                ? `${mapped} mapped`
                : p.status === "done"
                  ? "partial"
                  : statusBadge(p.status);
              return `<button type="button" class="dest-row${destMeta?.pathChanged ? " flash" : ""}${routeInspector.selectedRouteId === p.id ? " selected" : ""}" data-route-id="${escapeHtml(p.id)}" data-dest-host="${escapeHtml(p.host)}" title="Inspect route to ${escapeHtml(p.ip)}${escapeHtml(nameSource)}" aria-pressed="${routeInspector.selectedRouteId === p.id}">
                <span class="dest-star${p.reachedTarget ? "" : " partial"}">${marker}</span>
                <span class="dest-main">
                  <span class="dest-host">${escapeHtml(p.host)}${nameConfidence}${org}${changed}</span>
                  <span class="dest-meta">:${p.port} · ${escapeHtml(p.protocol)} · ${p.reachedTarget ? rtt : `last reply ${rtt}`}${destCity}${escapeHtml(ownerLabel)}</span>
                </span>
                <span class="dest-side">${mapped > 0 ? traceState : statusBadge(p.status)}</span>
              </button>`;
            })
            .join("")
        : "";

      const icon = g.iconUrl
        ? `<img class="app-icon-image" src="${escapeHtml(g.iconUrl)}" alt="" decoding="async">`
        : "";
      return `<div class="app-card${isFocused ? " focused" : ""}${dim ? " dim" : ""}" data-app="${escapeHtml(g.id)}">
        <button type="button" class="app-row app-row-native" data-app-toggle="${escapeHtml(g.id)}" aria-expanded="${isOpen}">
          <span class="app-icon-shell" style="--app-color:${g.color}"><span class="app-icon-fallback"></span>${icon}<i></i></span>
          <span class="app-main">
            <span class="app-name">${escapeHtml(g.name)}</span>
            <span class="app-meta">${g.traced}/${g.totalDests} dests · ${g.currentConnections} current${act}</span>
          </span>
          <span class="chev">${isOpen ? "▾" : "▸"}</span>
        </button>
        ${isOpen ? `${processRows}<div class="dest-list">${destRows || `<div class="empty sm">No destinations</div>`}</div>` : ""}
      </div>`;
    })
    .join("");
  el.appList.innerHTML = applications + renderUnattributed(snapshot?.unattributed ?? null);
}

function renderUnattributed(group: TrafficGroupDto | null): string {
  if (!group) return "";
  const stats = snapshot?.attribution;
  const reasons = [
    stats?.ownerGone ? `${stats.ownerGone} owner unavailable` : "",
    stats?.ambiguous ? `${stats.ambiguous} ambiguous` : "",
    stats?.accessLimited ? `${stats.accessLimited} access-limited` : "",
  ]
    .filter(Boolean)
    .join(" · ");
  const rows = group.destinations
    .slice()
    .sort((a, b) => b.hits - a.hits)
    .map((dest) => {
      const id = routeId(UNATTRIBUTED_ID, dest);
      return `<button type="button" class="dest-row${routeInspector.selectedRouteId === id ? " selected" : ""}" data-route-id="${escapeHtml(id)}" title="Inspect unattributed route to ${escapeHtml(dest.ip)}" aria-pressed="${routeInspector.selectedRouteId === id}">
        <span class="dest-star muted-star">?</span>
        <span class="dest-main">
          <span class="dest-host">${escapeHtml(destName(dest))}</span>
          <span class="dest-meta">:${dest.port} · ${escapeHtml(dest.protocol)} · ${dest.hits} connection${dest.hits === 1 ? "" : "s"}</span>
        </span>
        <span class="dest-side">${statusBadge(dest.trace.status)}</span>
      </button>`;
    })
    .join("");
  return `<details class="app-card unattributed-card">
    <summary class="app-row">
      <span class="swatch unattributed-swatch"></span>
      <span class="app-main">
        <span class="app-name">Unattributed traffic</span>
        <span class="app-meta">${group.destinations.length} dests · ${group.currentConnections} current${reasons ? ` · ${reasons}` : ""}</span>
      </span>
      <span class="chev">▾</span>
    </summary>
    <div class="dest-list">${rows}</div>
  </details>`;
}

function formatByteRate(bytes: number): string {
  if (bytes < 1024) return `${Math.round(bytes)} B/s`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB/s`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB/s`;
}

function escapeHtml(s: string): string {
  return s
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

function showToast(msg: string) {
  el.toast.hidden = false;
  el.toast.textContent = msg;
  window.setTimeout(() => {
    el.toast.hidden = true;
  }, 4500);
}

function wireUi() {
  document.addEventListener("error", (event) => {
    const image = event.target as HTMLImageElement | null;
    if (!image?.classList.contains("app-icon-image")) return;
    image.hidden = true;
    image.parentElement?.classList.add("icon-missing");
  }, true);
  el.networkTransition.addEventListener("click", () => {
    if (!snapshot?.networkOrigin) return;
    el.networkTransition.hidden = true;
    setSelectedPath(null);
    routeInspector.showOrigin(snapshot.networkOrigin, el.networkTransition);
    paint(true);
  });
  el.networkOrigin.addEventListener("click", () => {
    if (snapshot?.networkOrigin) {
      setSelectedPath(null);
      routeInspector.showOrigin(snapshot.networkOrigin, el.networkOrigin);
      paint(true);
    }
  });
  el.appList.addEventListener("click", (ev) => {
    const routeButton = (ev.target as HTMLElement).closest<HTMLButtonElement>(
      "[data-route-id]",
    );
    if (routeButton?.dataset.routeId) {
      setSelectedPath(routeButton.dataset.routeId, {
        frame: true,
        preview: true,
        instant: (ev as MouseEvent).detail === 0,
      });
      routeInspector.showRoute(routeButton.dataset.routeId, routeButton);
      paint(true);
      return;
    }
    const btn = (ev.target as HTMLElement).closest<HTMLButtonElement>(
      "[data-app-toggle]",
    );
    if (!btn) return;
    const appId = btn.dataset.appToggle!;
    const multi = (ev as MouseEvent).shiftKey;

    if (multi) {
      if (focused.has(appId)) focused.delete(appId);
      else focused.add(appId);
      expanded.add(appId);
    } else {
      if (expanded.has(appId) && focused.has(appId) && focused.size === 1) {
        expanded.delete(appId);
        focused.clear();
      } else {
        expanded.add(appId);
        focused.clear();
        focused.add(appId);
      }
    }
    paint(true);
  });

  el.filter.addEventListener("input", () => {
    filter = el.filter.value;
    paint(true);
  });

  for (const t of [
    el.togExternal,
    el.togUdp,
    el.togTraces,
    el.togLocalGeo,
    el.togHistory,
    el.togEnhanced,
    el.togDomains,
  ]) {
    t.addEventListener("change", () => {
      void pushSettings();
    });
  }

  el.togLabels.addEventListener("change", () => {
    setLabelsVisible(el.togLabels.checked);
    paint(true);
  });

  el.selDensity.addEventListener("change", () => {
    setDensity(el.selDensity.value as "all" | "destinations" | "hubs");
    void pushSettings();
    paint(true);
  });

  el.btnReset.addEventListener("click", async () => {
    await invoke("reset_monitor");
    clearGlobe();
    focused.clear();
    expanded.clear();
    lastSidebarSig = "";
    lastHeaderSig = "";
    showToast("Reset history and traceroute cache");
    schedulePaint(undefined, true);
  });

  el.btnTraceAll.addEventListener("click", async () => {
    await invoke("force_trace_all");
    showToast("Re-tracing all destinations…");
  });

  el.btnRecenter.addEventListener("click", () => {
    recenterOnData();
    paint(true);
    showToast("Camera recentered on active paths");
  });

  el.btnClearFocus.addEventListener("click", () => {
    focused.clear();
    paint(true);
  });

  el.btnAbout.addEventListener("click", () => {
    el.aboutVersion.textContent = `Version ${appVersion}`;
    el.aboutModal.hidden = false;
  });
  el.btnAboutClose.addEventListener("click", () => {
    el.aboutModal.hidden = true;
  });
  el.aboutModal.addEventListener("click", (ev) => {
    if (ev.target === el.aboutModal) el.aboutModal.hidden = true;
  });
  window.addEventListener("keydown", (ev) => {
    const target = ev.target as HTMLElement | null;
    if (
      ev.key === "/" &&
      target?.tagName !== "INPUT" &&
      target?.tagName !== "TEXTAREA"
    ) {
      ev.preventDefault();
      el.filter.focus();
      return;
    }
    if (ev.key === "Escape" && !el.aboutModal.hidden) {
      el.aboutModal.hidden = true;
      return;
    }
    if (ev.key === "Escape" && !el.inspector.hidden) {
      routeInspector.close();
      return;
    }
    if (ev.key === "Escape" && document.activeElement === el.filter) {
      el.filter.value = "";
      filter = "";
      el.filter.blur();
      paint(true);
    }
  });
}

async function loadVersion() {
  try {
    appVersion = await getVersion();
  } catch {
    appVersion = "0.1.0";
  }
  el.appVersion.textContent = `v${appVersion}`;
  el.aboutVersion.textContent = `Version ${appVersion}`;
}

async function boot() {
  wireUi();
  void loadVersion();

  try {
    initGlobe(el.globe);
    setHopClickHandler((selection: HopSelection) => {
      const routeIds = [...new Set(selection.routes.map((route) => route.pathId))];
      if (routeIds.length === 1) {
        setSelectedPath(routeIds[0], { frame: true, preview: true });
        routeInspector.showRoute(routeIds[0]);
      } else {
        setSelectedPath(null);
        routeInspector.showNode(selection);
      }
      paint(true);
    });
    setSegmentClickHandler((segment: GlobeSegmentSelection) => {
      setSelectedPath(segment.pathId, { frame: true, preview: true });
      routeInspector.showRoute(segment.pathId, null, segment);
      paint(true);
    });
    setOriginClickHandler((origin: NetworkOrigin) => {
      setSelectedPath(null);
      routeInspector.showOrigin(origin);
      paint(true);
    });
    globeReady = true;
  } catch (e) {
    el.globeStatus.textContent = `globe failed: ${String(e)}`;
  }

  try {
    const settings = await invoke<SettingsDto>("get_settings");
    el.togEnhanced.checked = !!settings.enhancedMonitoring;
    el.togDomains.checked = settings.identifyDomains !== false;
    el.togExternal.checked = settings.externalOnly;
    el.togUdp.checked = settings.includeUdp;
    el.togTraces.checked = settings.tracesEnabled;
    el.togLocalGeo.checked = !!settings.geoLocalOnly;
    el.togHistory.checked = !!settings.historyEnabled;
    if (settings.globeDensity) {
      el.selDensity.value = settings.globeDensity;
      setDensity(settings.globeDensity as "all" | "destinations" | "hubs");
    }
  } catch {
    /* preview */
  }

  mountOnboarding(el.onboardingHost);
  listenToStreamStatus(handleStreamStatus);

  try {
    snapshot = await invoke<SnapshotDto>("get_snapshot");
    paint(true);
  } catch (e) {
    lastMonitorError = `Waiting for backend: ${String(e)}`;
    el.status.textContent = "Waiting for backend…";
  }

  try {
    await listen<SnapshotDto>("monitor-update", (event) => {
      if (event.payload.collection?.status === "ready") lastMonitorError = null;
      schedulePaint(event.payload, false);
    });
    await listen<string>("monitor-error", (event) => {
      lastMonitorError = event.payload;
      paint(false);
    });
    await listen<{ app: string; host: string; ip: string; summary: string }>(
      "path-changed",
      (event) => {
        showToast(
          `Path changed · ${event.payload.app} → ${event.payload.host}`,
        );
      },
    );
  } catch {
    /* preview */
  }

  try {
    const snap = await invoke<SnapshotDto>("refresh_now");
    schedulePaint(snap, true);
  } catch {
    /* ignore */
  }
}

void boot();
