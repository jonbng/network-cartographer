import {
  forceTrace,
  getVersion,
  invoke,
  listen,
  listenToStreamStatus,
  type StreamStatus,
} from "./api";
import type {
  GlobePath,
  GlobeSegmentSelection,
  HopSelection,
  NetworkOrigin,
} from "./globe";
import { introStatus, INTRO_LOCK_MS } from "./intro-state";
import { shouldPresentTransition, transitionCopy } from "./network-transition";
import { colorForKey } from "./path-color";
import { RouteInspector } from "./route-inspector";
import { mergeVisibleSettings } from "./settings-state";
import { isRenderableTrace } from "./trace-progress";

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
  instanceCount?: number;
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
  instanceCount: number;
};

let snapshot: SnapshotDto | null = null;
let filter = "";
let globeReady = false;
const expanded = new Set<string>();
const focused = new Set<string>();
let pendingSnap: SnapshotDto | null = null;
let paintScheduled = false;
let paintTimer: number | null = null;
let paintFrame: number | null = null;
let lastPaintAt = 0;
const MIN_PAINT_MS = 1000;
let lastSidebarSig = "";
let lastHeaderSig = "";
let streamStatus: StreamStatus = "connecting";
let reconnectTimer: number | null = null;
let streamHasOpened = false;
let lastMonitorError: string | null = null;
let transitionTimer: number | null = null;
let toastTimer: number | null = null;
let shownTransitionId = 0;
let shownTransitionStatus = "";
let queuedSettings: SettingsDto | null = null;
let settingsWriteRunning = false;
let persistedSettings: SettingsDto | null = null;
let introUnlocked = false;
let globeApi: typeof import("./globe") | null = null;

const UNATTRIBUTED_NAME = "Unattributed traffic";
const UNATTRIBUTED_ID = "__unattributed__";
const UNATTRIBUTED_COLOR = "#8a8680";

const el = {
  globe: document.getElementById("globe")!,
  globeStatus: document.getElementById("globe-status")!,
  mapEmptyState: document.getElementById("map-empty-state")!,
  mapEmptyTitle: document.getElementById("map-empty-title")!,
  mapEmptyDetail: document.getElementById("map-empty-detail")!,
  introModal: document.getElementById("intro-modal")!,
  introStatusTitle: document.getElementById("intro-status-title")!,
  introStatusDetail: document.getElementById("intro-status-detail")!,
  btnIntroDismiss: document.getElementById("btn-intro-dismiss") as HTMLButtonElement,
  networkOrigin: document.getElementById("network-origin") as HTMLButtonElement,
  networkOriginPlace: document.getElementById("network-origin-place")!,
  networkOriginAssessment: document.getElementById("network-origin-assessment")!,
  appList: document.getElementById("app-list")!,
  sidebarSub: document.getElementById("sidebar-sub")!,
  btnClearFocus: document.getElementById("btn-clear-focus") as HTMLButtonElement,
  statApps: document.getElementById("stat-apps")!,
  statPaths: document.getElementById("stat-paths")!,
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
  togUdp: document.getElementById("tog-udp") as HTMLInputElement,
  udpStatus: document.getElementById("udp-status")!,
  togTraces: document.getElementById("tog-traces") as HTMLInputElement,
  togLabels: document.getElementById("tog-labels") as HTMLInputElement,
  togLocalGeo: document.getElementById("tog-local-geo") as HTMLInputElement,
  togEnhanced: document.getElementById("tog-enhanced") as HTMLInputElement,
  trafficSetting: document.getElementById("traffic-setting")!,
  trafficStatus: document.getElementById("traffic-status")!,
  togDomains: document.getElementById("tog-domains") as HTMLInputElement,
  domainsStatus: document.getElementById("domains-status")!,
  selDensity: document.getElementById("sel-density") as HTMLSelectElement,
  btnReset: document.getElementById("btn-reset")!,
  btnRecenter: document.getElementById("btn-recenter")!,
  toast: document.getElementById("toast")!,
  appVersion: document.getElementById("app-version")!,
  aboutModal: document.getElementById("about-modal")!,
  aboutVersion: document.getElementById("about-version")!,
  btnAbout: document.getElementById("btn-about") as HTMLButtonElement,
  btnAboutClose: document.getElementById("btn-about-close") as HTMLButtonElement,
  inspector: document.getElementById("route-inspector")!,
  networkTransition: document.getElementById("network-transition") as HTMLButtonElement,
  transitionTitle: document.getElementById("transition-title")!,
  transitionDetail: document.getElementById("transition-detail")!,
};

const routeInspector = new RouteInspector(el.inspector, {
  onClose: () => {
    globeApi?.setSelectedPath(null);
    paint(true);
  },
  onSelectRoute: (routeId, instant) => {
    globeApi?.setSelectedPath(routeId, { frame: true, preview: true, instant });
    paint(true);
  },
  onHighlightHop: (hop) => globeApi?.setHighlightedHop(hop),
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

function destinationKey(dest: DestDto): string {
  return `${dest.protocol}\u0000${dest.ip}\u0000${dest.port}`;
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

function applicationGroupId(name: string): string {
  return `app-group:${name.trim().replace(/\s+/g, " ").toLocaleLowerCase()}`;
}

function tracePriority(trace: TraceDto): number {
  if (trace.status === "done" && trace.hops.length > 0) return 5;
  if (trace.status === "running") return 4;
  if (trace.status === "queued") return 3;
  if (trace.status === "done") return 2;
  if (trace.status === "failed") return 1;
  return 0;
}

function mergeApplications(apps: AppDto[]): AppDto[] {
  const grouped = new Map<string, AppDto>();
  const destinationIndexes = new Map<string, Map<string, DestDto>>();
  for (const app of apps) {
    const id = applicationGroupId(app.name);
    const existing = grouped.get(id);
    if (!existing) {
      const destinations = app.destinations.map((dest) => ({
        ...dest,
        processIds: [...(dest.processIds ?? [])],
      }));
      grouped.set(id, {
        ...app,
        id,
        pids: [...app.pids],
        processes: [...(app.processes ?? [])],
        destinations,
        instanceCount: 1,
      });
      const index = new Map<string, DestDto>();
      for (const destination of destinations) {
        const key = destinationKey(destination);
        if (!index.has(key)) index.set(key, destination);
      }
      destinationIndexes.set(id, index);
      continue;
    }

    existing.instanceCount = (existing.instanceCount ?? 1) + 1;
    existing.iconUrl ||= app.iconUrl;
    existing.path ||= app.path;
    existing.pids = [...new Set([...existing.pids, ...app.pids])];
    existing.processes = [...new Map(
      [...(existing.processes ?? []), ...(app.processes ?? [])]
        .map((process) => [process.id, process]),
    ).values()];
    existing.hits += app.hits;
    existing.hitsPerSec = (existing.hitsPerSec ?? 0) + (app.hitsPerSec ?? 0);
    existing.activity = (existing.activity ?? 0) + (app.activity ?? app.hitsPerSec ?? 0);
    existing.currentConnections = (existing.currentConnections ?? 0) + (app.currentConnections ?? 0);
    existing.newConnectionsPerSec =
      (existing.newConnectionsPerSec ?? 0) + (app.newConnectionsPerSec ?? app.hitsPerSec ?? 0);
    if (app.traffic) {
      if (existing.traffic) {
        existing.traffic = {
          ...existing.traffic,
          rxBytesPerSec: existing.traffic.rxBytesPerSec + app.traffic.rxBytesPerSec,
          txBytesPerSec: existing.traffic.txBytesPerSec + app.traffic.txBytesPerSec,
          totalBytesPerSec: existing.traffic.totalBytesPerSec + app.traffic.totalBytesPerSec,
          sampleWindowMs: Math.max(existing.traffic.sampleWindowMs, app.traffic.sampleWindowMs),
        };
      } else {
        existing.traffic = { ...app.traffic };
      }
    }

    const destinationIndex = destinationIndexes.get(id)!;
    for (const dest of app.destinations) {
      const key = destinationKey(dest);
      const match = destinationIndex.get(key);
      if (!match) {
        const destination = {
          ...dest,
          processIds: [...(dest.processIds ?? [])],
        };
        existing.destinations.push(destination);
        destinationIndex.set(key, destination);
        continue;
      }
      match.hits += dest.hits;
      match.lastSeenSecs = Math.min(match.lastSeenSecs, dest.lastSeenSecs);
      match.processIds = [...new Set([...(match.processIds ?? []), ...(dest.processIds ?? [])])];
      match.pathChanged = !!match.pathChanged || !!dest.pathChanged;
      if (tracePriority(dest.trace) > tracePriority(match.trace)) {
        match.trace = dest.trace;
      }
    }
    existing.destCount = existing.destinations.length;
  }
  return [...grouped.values()];
}

function collectPaths(apps: AppDto[]): { allPaths: GlobePath[]; paths: GlobePath[] } {
  if (!snapshot) return { allPaths: [], paths: [] };
  const allPaths: GlobePath[] = [];
  const visiblePathIds = new Set<string>();
  for (const app of apps) {
    for (const dest of app.destinations) {
      const path =
        pathForDestination(
          app.id,
          app.name,
          app.iconUrl ?? null,
          colorForKey(app.id),
          dest,
        );
      allPaths.push(path);
      if (matchesFilter(app.name, dest)) visiblePathIds.add(path.id);
    }
  }
  for (const dest of snapshot.unattributed?.destinations ?? []) {
    const path =
      pathForDestination(
        UNATTRIBUTED_ID,
        UNATTRIBUTED_NAME,
        null,
        UNATTRIBUTED_COLOR,
        dest,
      );
    allPaths.push(path);
    if (matchesFilter(UNATTRIBUTED_NAME, dest)) visiblePathIds.add(path.id);
  }
  allPaths.sort(
    (a, b) =>
      a.app.localeCompare(b.app) || b.hits - a.hits || a.host.localeCompare(b.host),
  );
  return {
    allPaths,
    paths: allPaths.filter((path) => visiblePathIds.has(path.id)),
  };
}

function collectAppGroups(apps: AppDto[], paths: GlobePath[]): AppGroup[] {
  if (!snapshot) return [];
  const byId = new Map(paths.map((p) => [p.id, p]));
  const groups: AppGroup[] = [];
  for (const app of apps) {
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
      instanceCount: app.instanceCount ?? 1,
    };
    for (const dest of app.destinations) {
      const id = routeId(app.id, dest);
      const path = byId.get(id);
      if (!path) continue;
      group.totalDests += 1;
      group.paths.push(path);
      if (path.status === "done") group.traced += 1;
      group.mappedHops += path.hops.filter((h) => h.lat != null).length;
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
      // Hits affect destination order, but are not displayed for attributed
      // routes. Key the rendered order instead of the raw counter so ordinary
      // traffic does not replace the entire sidebar DOM every second.
      const dests = g.paths
        .slice()
        .sort((a, b) => b.hits - a.hits)
        .map(
          (p) =>
            `${p.id}:${p.host}:${p.port}:${p.status}:${p.freshness}:${p.domainSource}:${p.domainConfidence}:${p.domainAlternativesCount}:${p.hops.length}:${p.hops.filter((h) => h.lat != null).length}:${p.rttMs == null ? "" : Math.round(p.rttMs)}`,
        )
        .join(";");
      const processes = g.processes
        .map((process) => `${process.id}:${process.pid}:${process.name}:${process.path ?? ""}`)
        .sort()
        .join(",");
      return `${g.id}|${g.name}|${g.instanceCount}|${g.iconUrl ?? ""}|${g.traced}/${g.totalDests}|${processes}|${dests}`;
    })
    .join("||");
  const unattributed = snapshot?.unattributed;
  const unattributedBody = unattributed?.destinations
    .map((dest) => `${destName(dest)}:${dest.port}:${dest.protocol}:${dest.hits}:${dest.trace.status}`)
    .join(";") ?? "";
  return `${foc}#${exp}#${filter}#${body}#${unattributed?.currentConnections ?? 0}:${unattributedBody}`;
}

function currentSettings(): SettingsDto {
  const base = persistedSettings ?? snapshot?.settings ?? {
    includeUdp: true,
    tracesEnabled: true,
    pollIntervalMs: 1000,
  };
  return mergeVisibleSettings(base, {
    settingsVersion: 3,
    includeUdp: el.togUdp.checked,
    tracesEnabled: el.togTraces.checked,
    pollIntervalMs: persistedSettings?.pollIntervalMs ?? snapshot?.settings?.pollIntervalMs ?? 1000,
    geoLocalOnly: el.togLocalGeo.checked,
    globeDensity: el.selDensity.value,
    identifyDomains: el.togDomains.checked,
    enhancedMonitoring: el.togEnhanced.checked,
    privacyAccepted: true,
  });
}

async function pushSettings() {
  queuedSettings = currentSettings();
  if (settingsWriteRunning) return;

  settingsWriteRunning = true;
  while (queuedSettings) {
    const settings = queuedSettings;
    queuedSettings = null;
    try {
      persistedSettings = await invoke<SettingsDto>("set_settings", { settings });
    } catch {
      /* preview */
    }
  }
  settingsWriteRunning = false;
}

type HealthState = "ready" | "degraded" | "unavailable" | "waiting";

function setText(element: HTMLElement, text: string): void {
  if (element.textContent !== text) element.textContent = text;
}

function setHidden(element: HTMLElement, hidden: boolean): void {
  if (element.hidden !== hidden) element.hidden = hidden;
}

function setClassName(element: HTMLElement, className: string): void {
  if (element.className !== className) element.className = className;
}

function setHealthValue(element: HTMLElement, text: string, state: HealthState = "ready") {
  setText(element, text);
  element.classList.toggle("degraded", state === "degraded");
  element.classList.toggle("unavailable", state === "unavailable");
}

function renderCapabilities(): void {
  const trafficRates = snapshot?.capabilities?.trafficRates ?? true;
  setHidden(el.trafficSetting, !trafficRates);
  if (!trafficRates) {
    el.togEnhanced.checked = false;
  } else {
    setText(el.trafficStatus, "Native per-app upload and download");
  }
}

function renderHealth(mappedPaths: GlobePath[]): void {
  if (!snapshot) {
    setText(el.status, "Starting monitor…");
    setClassName(el.healthDot, "health-dot waiting");
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

  setText(el.status, streamStatus === "reconnecting"
    ? "Reconnecting · showing last snapshot"
    : `Near-live · ${snapshot.appCount} ${snapshot.appCount === 1 ? "app" : "apps"} · ${mappedPaths.length} ${mappedPaths.length === 1 ? "route" : "routes"}`);
  setClassName(el.healthDot, `health-dot ${overall}`);
  setHealthValue(
    el.healthOverall,
    overall === "ready" ? "Ready" : overall === "degraded" ? "Limited" : "Unavailable",
    overall,
  );
  setHidden(el.healthDetail, details.length === 0);
  setText(el.healthDetail, [...new Set(details)].join(" · "));
}

function renderEntryStates(visibleRoutes: number, allRoutes: number): void {
  const state = introStatus(snapshot && {
    appCount: snapshot.appCount,
    destCount: snapshot.destCount,
    tracesEnabled: snapshot.tracesEnabled,
    queued: snapshot.traceStats.queued,
    running: snapshot.traceStats.running,
    done: snapshot.traceStats.done,
    failed: snapshot.traceStats.failed,
    mappedRoutes: allRoutes,
  });
  setText(el.introStatusTitle, state.title);
  setText(el.introStatusDetail, state.detail);

  setHidden(el.mapEmptyState, visibleRoutes > 0);
  if (visibleRoutes === 0) {
    setText(el.mapEmptyTitle, state.emptyTitle);
    setText(el.mapEmptyDetail, state.emptyDetail);
  }
}

function dismissIntro(): void {
  if (!introUnlocked || el.introModal.hidden) return;
  el.introModal.hidden = true;
  el.btnRecenter.focus({ preventScroll: true });
}

function renderNetworkTransition(origin: NetworkOrigin | null): void {
  const transition = origin?.transition;
  if (!shouldPresentTransition(transition, shownTransitionId) || !transition) return;

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
  cancelScheduledPaint();
  paintScheduled = false;
  lastPaintAt = performance.now();
  if (pendingSnap) {
    snapshot = pendingSnap;
    pendingSnap = null;
  }
  if (snapshot?.udpMonitoring) {
    setText(el.udpStatus, snapshot.udpMonitoring.message);
  }
  if (snapshot?.destinationNaming) {
    setText(el.domainsStatus, snapshot.destinationNaming.message);
  }

  const apps = mergeApplications(snapshot?.apps ?? []);
  const { allPaths, paths } = collectPaths(apps);
  const completedMappedPaths = allPaths.filter(
    (p) => p.status === "done" && p.hops.length > 0,
  );
  const mapPaths = paths.filter(isRenderableTrace);
  const groups = collectAppGroups(apps, paths);
  const networkOrigin = snapshot?.networkOrigin ?? null;
  globeApi?.setFocusedApps([...focused]);
  routeInspector.update(allPaths, networkOrigin);
  const selectedRoute = routeInspector.selectedRouteId;
  const nextSelectedRoute = selectedRoute && allPaths.some((path) => path.id === selectedRoute)
    ? selectedRoute
    : null;
  // User actions apply selection directly. Snapshot paints should only sync
  // it when route availability actually changes; re-applying it invalidates
  // unchanged globe geometry and cancels the current path animation.
  if (globeApi && globeApi.getSelectedPath() !== nextSelectedRoute) {
    globeApi.setSelectedPath(nextSelectedRoute);
  }

  if (snapshot) {
    const t = snapshot.traceStats;
    const headerSig = [
      apps.length,
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
      setText(el.statApps, `${apps.length}`);
      setHidden(el.btnClearFocus, focused.size === 0);
      const focusedNames = apps
        .filter((app) => focused.has(app.id))
        .map((app) => app.name);
      setText(el.sidebarSub,
        focused.size === 0
          ? "Select an app to isolate traffic"
          : `Isolating ${focusedNames.join(", ")}`);
      renderNetworkOrigin(networkOrigin);
    }
  }

  let pathCount = 0;
  let hopCount = 0;
  let destCount = 0;
  if (globeReady) {
    const stats = globeApi!.updateAllPaths(mapPaths, networkOrigin);
    pathCount = stats.pathCount;
    hopCount = stats.hopCount;
    destCount = stats.destCount;
  }
  setText(el.statPaths, `${pathCount}`);
  setText(el.globeStatus, pathCount > 0
    ? `${pathCount} paths · ${destCount} destinations · ${hopCount} hops`
    : completedMappedPaths.length > 0
      ? "No mapped routes match this view"
    : !snapshot
      ? "Starting monitor…"
      : snapshot.appCount === 0
        ? "Watching for traffic · try websites hosted in a few different countries"
        : snapshot.destCount === 0
          ? "Finding destinations · new activity appears after a short delay"
          : snapshot.tracesEnabled
            ? "Mapping routes · traceroutes take a little time"
            : "Connections detected · route mapping is off");
  renderCapabilities();
  renderHealth(completedMappedPaths);
  renderEntryStates(pathCount, completedMappedPaths.length);
  renderNetworkTransition(networkOrigin);

  const sig = sidebarSignature(groups);
  if (forceSidebar || sig !== lastSidebarSig) {
    lastSidebarSig = sig;
    const scrollTop = el.appList.scrollTop;
    const activeSidebarControl = sidebarControlIdentity(document.activeElement);
    renderSidebar(groups, apps);
    el.appList.scrollTop = scrollTop;
    restoreSidebarFocus(activeSidebarControl);
  }
  updateSidebarMetrics(groups);
}

function renderNetworkOrigin(origin: NetworkOrigin | null) {
  el.networkOrigin.classList.toggle("locating", !origin || origin.status === "locating");
  el.networkOrigin.classList.toggle("unavailable", origin?.status === "unavailable");
  const exit = origin?.exit;
  if (!origin || origin.status === "locating") {
    setText(el.networkOriginPlace, "Locating…");
    setHidden(el.networkOriginAssessment, false);
    setText(el.networkOriginAssessment, "Inspecting route");
    return;
  }
  setHidden(el.networkOriginAssessment, origin.assessment === "no_evidence");
  if (!exit) {
    setText(el.networkOriginPlace, "Unavailable");
    setText(el.networkOriginAssessment, assessmentText(origin.assessment));
    return;
  }
  setText(el.networkOriginPlace, exit.city
    ? `${exit.city}${exit.country ? `, ${exit.country}` : ""}`
    : exit.ip || "Location unavailable");
  setText(el.networkOriginAssessment, assessmentText(origin.assessment));
  const title = exit.organization
    ? `${exit.organization}${exit.asn ? ` · AS${exit.asn}` : ""}`
    : "Inspect primary network exit";
  if (el.networkOrigin.title !== title) el.networkOrigin.title = title;
}

function assessmentText(assessment: NetworkOrigin["assessment"]): string {
  if (assessment === "proxy_and_tunnel") return "Proxy + tunnel signals";
  if (assessment === "proxy_configured") return "Proxy configured";
  if (assessment === "tunnel_likely") return "VPN / tunnel likely";
  if (assessment === "no_evidence") return "";
  return "Evidence unavailable";
}

function schedulePaint(snap?: SnapshotDto, immediate = false) {
  if (snap) pendingSnap = snap;
  if (immediate) {
    cancelScheduledPaint();
    paint(true);
    return;
  }
  if (paintScheduled) return;
  const wait = Math.max(0, MIN_PAINT_MS - (performance.now() - lastPaintAt));
  paintScheduled = true;
  paintTimer = window.setTimeout(() => {
    paintTimer = null;
    paintFrame = requestAnimationFrame(() => {
      paintFrame = null;
      paint(false);
    });
  }, wait);
}

function cancelScheduledPaint(): void {
  if (paintTimer != null) {
    window.clearTimeout(paintTimer);
    paintTimer = null;
  }
  if (paintFrame != null) {
    cancelAnimationFrame(paintFrame);
    paintFrame = null;
  }
  paintScheduled = false;
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

function renderSidebar(groups: AppGroup[], apps: AppDto[]) {
  const retainedIcons = new Map<string, HTMLImageElement>();
  for (const card of el.appList.querySelectorAll<HTMLElement>("[data-app]")) {
    const id = card.dataset.app;
    const icon = card.querySelector<HTMLImageElement>(".app-icon-image");
    if (id && icon) retainedIcons.set(id, icon);
  }

  if (groups.length === 0 && !snapshot?.unattributed) {
    el.appList.innerHTML = `<div class="empty">No internet activity detected yet.<br>Open websites hosted in a few different countries, then give the map a moment.</div>`;
    return;
  }

  const applications = groups
    .map((g) => {
      const isOpen = expanded.has(g.id) || focused.has(g.id);
      const isFocused = focused.has(g.id);
      const dim = focused.size > 0 && !isFocused;
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
              const rtt = p.rttMs != null ? `${Math.round(p.rttMs)}ms` : "-";
              const destCity = lastCity ? ` · ${escapeHtml(lastCity)}` : "";
              // org from snapshot if available
              const destMeta = apps
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
            <span class="app-meta">${appMetaMarkup(g)}</span>
          </span>
          <span class="chev">${isOpen ? "▾" : "▸"}</span>
        </button>
        ${isOpen ? `${processRows}<div class="dest-list">${destRows || `<div class="empty sm">No destinations</div>`}</div>` : ""}
      </div>`;
    })
    .join("");
  el.appList.innerHTML = applications + renderUnattributed(snapshot?.unattributed ?? null);

  // Keep decoded app icons alive across live-data refreshes. Recreating image
  // nodes every second causes a visible fallback/icon flash on some browsers.
  for (const card of el.appList.querySelectorAll<HTMLElement>("[data-app]")) {
    const id = card.dataset.app;
    const nextIcon = card.querySelector<HTMLImageElement>(".app-icon-image");
    const retained = id ? retainedIcons.get(id) : null;
    if (nextIcon && retained && nextIcon.src === retained.src) nextIcon.replaceWith(retained);
  }
}

function appMetaMarkup(group: AppGroup): string {
  const instances = group.instanceCount > 1
    ? ` · ${group.instanceCount} instances`
    : "";
  const activity = group.traffic
    ? `<span class="act"> · ↓${formatByteRate(group.traffic.rxBytesPerSec)} ↑${formatByteRate(group.traffic.txBytesPerSec)}</span>`
    : group.newConnectionsPerSec > 0.05
      ? `<span class="act"> · ${group.newConnectionsPerSec.toFixed(1)} new/s</span>`
      : "";
  return `${group.traced}/${group.totalDests} dests · ${group.currentConnections} current${instances}${activity}`;
}

function updateSidebarMetrics(groups: AppGroup[]): void {
  const cards = new Map(
    [...el.appList.querySelectorAll<HTMLElement>("[data-app]")]
      .map((card) => [card.dataset.app, card] as const),
  );
  for (const group of groups) {
    const meta = cards.get(group.id)?.querySelector<HTMLElement>(".app-row-native .app-meta");
    if (!meta) continue;
    const markup = appMetaMarkup(group);
    if (meta.innerHTML !== markup) meta.innerHTML = markup;
  }
}

function renderUnattributed(group: TrafficGroupDto | null): string {
  if (!group) return "";
  const isOpen = expanded.has(UNATTRIBUTED_ID);
  const stats = snapshot?.attribution;
  const reasons = [
    stats?.ownerGone ? `${stats.ownerGone} owner unavailable` : "",
    stats?.ambiguous ? `${stats.ambiguous} ambiguous` : "",
    stats?.accessLimited ? `${stats.accessLimited} access-limited` : "",
  ]
    .filter(Boolean)
    .join(" · ");
  const rows = isOpen
    ? group.destinations
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
        .join("")
    : "";
  return `<div class="app-card unattributed-card">
    <button type="button" class="app-row" data-unattributed-toggle aria-expanded="${isOpen}">
      <span class="swatch unattributed-swatch"></span>
      <span class="app-main">
        <span class="app-name">Unattributed traffic</span>
        <span class="app-meta">${group.destinations.length} dests · ${group.currentConnections} current${reasons ? ` · ${reasons}` : ""}</span>
      </span>
      <span class="chev">${isOpen ? "▾" : "▸"}</span>
    </button>
    ${isOpen ? `<div class="dest-list">${rows}</div>` : ""}
  </div>`;
}

type SidebarControlIdentity =
  | { kind: "app"; value: string }
  | { kind: "route"; value: string }
  | { kind: "unattributed"; value: "" };

function sidebarControlIdentity(active: Element | null): SidebarControlIdentity | null {
  if (!(active instanceof HTMLElement) || !el.appList.contains(active)) return null;
  if (active.dataset.appToggle) return { kind: "app", value: active.dataset.appToggle };
  if (active.dataset.routeId) return { kind: "route", value: active.dataset.routeId };
  if (active.hasAttribute("data-unattributed-toggle")) {
    return { kind: "unattributed", value: "" };
  }
  return null;
}

function restoreSidebarFocus(identity: SidebarControlIdentity | null): void {
  if (!identity) return;
  const candidates = identity.kind === "app"
    ? el.appList.querySelectorAll<HTMLElement>("[data-app-toggle]")
    : identity.kind === "route"
      ? el.appList.querySelectorAll<HTMLElement>("[data-route-id]")
      : el.appList.querySelectorAll<HTMLElement>("[data-unattributed-toggle]");
  const match = [...candidates].find((element) => {
    if (identity.kind === "app") return element.dataset.appToggle === identity.value;
    if (identity.kind === "route") return element.dataset.routeId === identity.value;
    return true;
  });
  match?.focus({ preventScroll: true });
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
  if (toastTimer != null) window.clearTimeout(toastTimer);
  el.toast.hidden = false;
  el.toast.textContent = msg;
  toastTimer = window.setTimeout(() => {
    toastTimer = null;
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
  el.btnIntroDismiss.addEventListener("click", dismissIntro);
  el.introModal.addEventListener("click", (event) => {
    if (event.target === el.introModal) dismissIntro();
  });
  window.setTimeout(() => {
    introUnlocked = true;
    el.btnIntroDismiss.disabled = false;
    el.btnIntroDismiss.focus({ preventScroll: true });
  }, INTRO_LOCK_MS);
  requestAnimationFrame(() => el.introModal.focus({ preventScroll: true }));
  el.networkTransition.addEventListener("click", () => {
    if (!snapshot?.networkOrigin) return;
    el.networkTransition.hidden = true;
    globeApi?.setSelectedPath(null);
    routeInspector.showOrigin(snapshot.networkOrigin, el.networkTransition);
    paint(true);
  });
  el.networkOrigin.addEventListener("click", () => {
    if (snapshot?.networkOrigin) {
      globeApi?.setSelectedPath(null);
      routeInspector.showOrigin(snapshot.networkOrigin, el.networkOrigin);
      paint(true);
    }
  });
  el.appList.addEventListener("click", (ev) => {
    const unattributedToggle = (ev.target as HTMLElement).closest<HTMLElement>(
      "[data-unattributed-toggle]",
    );
    if (unattributedToggle) {
      if (expanded.has(UNATTRIBUTED_ID)) expanded.delete(UNATTRIBUTED_ID);
      else expanded.add(UNATTRIBUTED_ID);
      paint(true);
      return;
    }
    const routeButton = (ev.target as HTMLElement).closest<HTMLButtonElement>(
      "[data-route-id]",
    );
    if (routeButton?.dataset.routeId) {
      globeApi?.setSelectedPath(routeButton.dataset.routeId, {
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
    el.togUdp,
    el.togTraces,
    el.togLocalGeo,
    el.togEnhanced,
    el.togDomains,
  ]) {
    t.addEventListener("change", () => {
      void pushSettings();
    });
  }

  el.togLabels.addEventListener("change", () => {
    globeApi?.setLabelsVisible(el.togLabels.checked);
    paint(true);
  });

  el.selDensity.addEventListener("change", () => {
    globeApi?.setDensity(el.selDensity.value as "all" | "destinations" | "hubs");
    void pushSettings();
    paint(true);
  });

  el.btnReset.addEventListener("click", async () => {
    await invoke("reset_monitor");
    globeApi?.clearGlobe();
    focused.clear();
    expanded.clear();
    lastSidebarSig = "";
    lastHeaderSig = "";
    showToast("Current apps, routes, and caches cleared");
    schedulePaint(undefined, true);
  });

  el.btnRecenter.addEventListener("click", () => {
    globeApi?.recenterOnData();
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
    if (ev.key === "Tab" && !el.introModal.hidden) {
      ev.preventDefault();
      (introUnlocked ? el.btnIntroDismiss : el.introModal).focus({ preventScroll: true });
      return;
    }
    if (
      ev.key === "/" &&
      el.introModal.hidden &&
      target?.tagName !== "INPUT" &&
      target?.tagName !== "TEXTAREA"
    ) {
      ev.preventDefault();
      el.filter.focus();
      return;
    }
    if (ev.key === "Escape" && !el.introModal.hidden) {
      dismissIntro();
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
    globeApi = await import("./globe");
    globeApi.initGlobe(el.globe);
    globeApi.setHopClickHandler((selection: HopSelection) => {
      const routeIds = [...new Set(selection.routes.map((route) => route.pathId))];
      if (routeIds.length === 1) {
        globeApi?.setSelectedPath(routeIds[0], { frame: true, preview: true });
        routeInspector.showRoute(routeIds[0]);
      } else {
        globeApi?.setSelectedPath(null);
        routeInspector.showNode(selection);
      }
      paint(true);
    });
    globeApi.setSegmentClickHandler((segment: GlobeSegmentSelection) => {
      globeApi?.setSelectedPath(segment.pathId, { frame: true, preview: true });
      routeInspector.showRoute(segment.pathId, null, segment);
      paint(true);
    });
    globeApi.setOriginClickHandler((origin: NetworkOrigin) => {
      globeApi?.setSelectedPath(null);
      routeInspector.showOrigin(origin);
      paint(true);
    });
    globeReady = true;
  } catch (e) {
    el.globeStatus.textContent = `globe failed: ${String(e)}`;
  }

  try {
    const settings = await invoke<SettingsDto>("get_settings");
    persistedSettings = settings;
    el.togEnhanced.checked = !!settings.enhancedMonitoring;
    el.togDomains.checked = settings.identifyDomains !== false;
    el.togUdp.checked = settings.includeUdp;
    el.togTraces.checked = settings.tracesEnabled;
    el.togLocalGeo.checked = !!settings.geoLocalOnly;
    if (settings.globeDensity) {
      el.selDensity.value = settings.globeDensity;
      globeApi?.setDensity(settings.globeDensity as "all" | "destinations" | "hubs");
    }
  } catch {
    /* preview */
  }

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
