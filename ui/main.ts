import { getVersion, invoke, listen } from "./api";
import {
  clearGlobe,
  colorForKey,
  initGlobe,
  recenterOnData,
  setDensity,
  setFocusedApps,
  setHopClickHandler,
  setLabelsVisible,
  setSelectedPath,
  updateAllPaths,
  type GlobePath,
  type HopSelection,
} from "./globe";
import { mountOnboarding } from "./onboarding";
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
  id?: string;
  name: string;
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
};

type AppGroup = {
  name: string;
  color: string;
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
/** Last known privacyAccepted so set_settings does not reset it. */
let privacyAccepted = false;

let pendingSnap: SnapshotDto | null = null;
let paintScheduled = false;
let lastPaintAt = 0;
const MIN_PAINT_MS = 1200;
let lastSidebarSig = "";
let lastHeaderSig = "";

const UNATTRIBUTED_NAME = "Unattributed traffic";
const UNATTRIBUTED_ID = "__unattributed__";
const UNATTRIBUTED_COLOR = "#8a8680";

const el = {
  globe: document.getElementById("globe")!,
  globeStatus: document.getElementById("globe-status")!,
  appList: document.getElementById("app-list")!,
  sidebarSub: document.getElementById("sidebar-sub")!,
  btnClearFocus: document.getElementById("btn-clear-focus") as HTMLButtonElement,
  statApps: document.getElementById("stat-apps")!,
  statPaths: document.getElementById("stat-paths")!,
  statHops: document.getElementById("stat-hops")!,
  statTraces: document.getElementById("stat-traces")!,
  statGeo: document.getElementById("stat-geo")!,
  status: document.getElementById("status-msg")!,
  filter: document.getElementById("filter") as HTMLInputElement,
  togExternal: document.getElementById("tog-external") as HTMLInputElement,
  togUdp: document.getElementById("tog-udp") as HTMLInputElement,
  udpStatus: document.getElementById("udp-status")!,
  togTraces: document.getElementById("tog-traces") as HTMLInputElement,
  togLabels: document.getElementById("tog-labels") as HTMLInputElement,
  togLocalGeo: document.getElementById("tog-local-geo") as HTMLInputElement,
  togHistory: document.getElementById("tog-history") as HTMLInputElement,
  togEnhanced: document.getElementById("tog-enhanced") as HTMLInputElement,
  togDomains: document.getElementById("tog-domains") as HTMLInputElement,
  domainsStatus: document.getElementById("domains-status")!,
  selDensity: document.getElementById("sel-density") as HTMLSelectElement,
  btnReset: document.getElementById("btn-reset")!,
  btnTraceAll: document.getElementById("btn-trace-all")!,
  btnRecenter: document.getElementById("btn-recenter")!,
  toast: document.getElementById("toast")!,
  onboardingHost: document.getElementById("onboarding-host")!,
  appVersion: document.getElementById("app-version")!,
  aboutModal: document.getElementById("about-modal")!,
  aboutVersion: document.getElementById("about-version")!,
  btnAbout: document.getElementById("btn-about") as HTMLButtonElement,
  btnAboutClose: document.getElementById("btn-about-close") as HTMLButtonElement,
  inspector: document.getElementById("route-inspector")!,
};

const routeInspector = new RouteInspector(el.inspector, {
  onClose: () => {
    setSelectedPath(null);
    paint(true);
  },
  onSelectRoute: (routeId) => {
    setSelectedPath(routeId);
    paint(true);
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
  color: string,
  dest: DestDto,
): GlobePath {
  return {
    id: routeId(ownerId, dest),
    app: ownerName,
    host: destName(dest),
    ip: dest.ip,
    port: dest.port,
    protocol: dest.protocol,
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
          app.id || app.name,
          app.name,
          colorForKey(app.name),
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
    const color = colorForKey(app.name);
    const group: AppGroup = {
      name: app.name,
      color,
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
      const id = routeId(app.id || app.name, dest);
      const path = byId.get(id);
      if (path) {
        group.paths.push(path);
        if (path.status === "done") group.traced += 1;
        group.mappedHops += path.hops.filter((h) => h.lat != null).length;
      } else {
        group.paths.push({
          id,
          app: app.name,
          host: destName(dest),
          ip: dest.ip,
          port: dest.port,
          protocol: dest.protocol,
          hits: dest.hits,
          color,
          hops: [],
          status: dest.trace.status,
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
            `${p.host}:${p.port}:${p.status}:${p.hops.filter((h) => h.lat != null).length}`,
        )
        .join(";");
      const processes = g.processes.map((process) => process.id).sort().join(",");
      return `${g.name}|${g.traced}/${g.totalDests}|${g.activity.toFixed(1)}|${g.currentConnections}|${processes}|${dests}`;
    })
    .join("||");
  const unattributed = snapshot?.unattributed;
  return `${foc}#${exp}#${filter}#${body}#${unattributed?.connections ?? 0}`;
}

function currentSettings(): SettingsDto {
  return {
    settingsVersion: 2,
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
    privacyAccepted,
  };
}

async function pushSettings() {
  try {
    await invoke("set_settings", { settings: currentSettings() });
  } catch {
    /* preview */
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
  const paths = collectPaths(true);
  const mapPaths = paths.filter((p) => p.status === "done" && p.hops.length > 0);
  const groups = collectAppGroups(paths);
  setFocusedApps([...focused]);
  routeInspector.update(allPaths);
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
      mapPaths.length,
      snapshot.geoBackend,
      snapshot.monitoring?.status,
      snapshot.collection?.status,
      snapshot.collection?.droppedEvents,
      [...focused].join(","),
      el.selDensity.value,
    ].join("|");

    if (headerSig !== lastHeaderSig) {
      lastHeaderSig = headerSig;
      el.statApps.textContent = `${snapshot.appCount}`;
      el.statTraces.textContent = snapshot.tracesEnabled
        ? `Q${t.queued} · R${t.running} · ${t.done} done`
        : "offline";
      if (el.statGeo) {
        const backend = snapshot.geoBackend ?? "api";
        el.statGeo.textContent = backend;
        el.statGeo.classList.toggle("accent", !!snapshot.geoMmdb);
        el.statGeo.title = snapshot.geoMmdb
          ? "Local MaxMind city DB loaded"
          : "Online geo only";
      }
      const attribution = snapshot.attribution;
      const quality = attribution
        ? ` · ${Math.round(attribution.ratio * 100)}% attributed`
        : "";
      const recovered = attribution?.recovered
        ? ` · ${attribution.recovered} recovered`
        : "";
      const telemetry = snapshot.monitoring;
      const traffic = telemetry?.mode === "native" ? " · traffic rates on" : "";
      const trafficError = telemetry?.status === "unavailable"
        ? ` · traffic rates unavailable: ${telemetry.message}`
        : "";
      const collection = snapshot.collection;
      const collectionMode = collection?.mode === "event-assisted"
        ? " · TCP close events on"
        : " · TCP polling";
      const collectionWarning = collection?.status === "degraded"
        ? ` · collector degraded: ${collection.message}`
        : "";
      const accessLimited = collection?.accessLimited
        ? ` · ${collection.accessLimited} protected processes skipped`
        : "";
      const dropped = collection?.droppedEvents
        ? ` · ${collection.droppedEvents} events dropped`
        : "";
      el.status.textContent = `Live · ${snapshot.liveConnections} conns · ${mapPaths.length} paths${quality}${recovered}${collectionMode}${collectionWarning}${accessLimited}${dropped}${traffic}${trafficError}`;
      el.btnClearFocus.hidden = focused.size === 0;
      el.sidebarSub.textContent =
        focused.size === 0
          ? "Select an app to isolate traffic"
          : `Isolating ${[...focused].join(", ")}`;
    }
  }

  let pathCount = 0;
  let hopCount = 0;
  let destCount = 0;
  if (globeReady) {
    const stats = updateAllPaths(mapPaths);
    pathCount = stats.pathCount;
    hopCount = stats.hopCount;
    destCount = stats.destCount;
  }
  el.statPaths.textContent = `${pathCount}`;
  el.statHops.textContent = `${hopCount}`;
  el.globeStatus.textContent =
    pathCount > 0
      ? `${pathCount} paths · ${destCount} destinations · ${hopCount} hops`
      : "Waiting for mapped traceroutes…";

  const sig = sidebarSignature(groups);
  if (forceSidebar || sig !== lastSidebarSig) {
    lastSidebarSig = sig;
    const scrollTop = el.appList.scrollTop;
    renderSidebar(groups);
    el.appList.scrollTop = scrollTop;
  }
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

function statusBadge(status: string): string {
  if (status === "done") return `<span class="badge ok">observed</span>`;
  if (status === "running") return `<span class="badge run">tracing…</span>`;
  if (status === "queued") return `<span class="badge queue">queued</span>`;
  if (status === "failed") return `<span class="badge fail">fail</span>`;
  return `<span class="badge">${escapeHtml(status)}</span>`;
}

function renderSidebar(groups: AppGroup[]) {
  if (groups.length === 0 && !snapshot?.unattributed) {
    el.appList.innerHTML = `<div class="empty">No applications with internet connections yet.</div>`;
    return;
  }

  const applications = groups
    .map((g) => {
      const isOpen = expanded.has(g.name) || focused.has(g.name);
      const isFocused = focused.has(g.name);
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
                .find((a) => a.name === g.name)
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
              const owners = g.processes.filter((process) =>
                destMeta?.processIds?.includes(process.id),
              );
              const ownerLabel = owners.length
                ? ` · ${owners.map((process) => `${process.name} (${process.pid})`).join(", ")}`
                : "";
              const marker = p.reachedTarget ? "★" : "◌";
              const traceState = p.reachedTarget
                ? `${mapped} mapped`
                : p.status === "done"
                  ? "partial"
                  : statusBadge(p.status);
              return `<button type="button" class="dest-row${destMeta?.pathChanged ? " flash" : ""}${routeInspector.selectedRouteId === p.id ? " selected" : ""}" data-route-id="${escapeHtml(p.id)}" data-dest-host="${escapeHtml(p.host)}" title="Inspect route to ${escapeHtml(p.ip)}${escapeHtml(nameSource)}" aria-pressed="${routeInspector.selectedRouteId === p.id}">
                <span class="dest-star${p.reachedTarget ? "" : " partial"}">${marker}</span>
                <span class="dest-main">
                  <span class="dest-host">${escapeHtml(p.host)}${org}${changed}</span>
                  <span class="dest-meta">:${p.port} · ${escapeHtml(p.protocol)} · ${p.reachedTarget ? rtt : `last reply ${rtt}`}${destCity}${escapeHtml(ownerLabel)}</span>
                </span>
                <span class="dest-side">${mapped > 0 ? traceState : statusBadge(p.status)}</span>
              </button>`;
            })
            .join("")
        : "";

      return `<div class="app-card${isFocused ? " focused" : ""}${dim ? " dim" : ""}" data-app="${escapeHtml(g.name)}">
        <button type="button" class="app-row" data-app-toggle="${escapeHtml(g.name)}" aria-expanded="${isOpen}">
          <span class="swatch" style="background:${g.color};box-shadow:0 0 10px ${g.color}"></span>
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
  el.appList.addEventListener("click", (ev) => {
    const routeButton = (ev.target as HTMLElement).closest<HTMLButtonElement>(
      "[data-route-id]",
    );
    if (routeButton?.dataset.routeId) {
      setSelectedPath(routeButton.dataset.routeId);
      routeInspector.showRoute(routeButton.dataset.routeId, routeButton);
      paint(true);
      return;
    }
    const btn = (ev.target as HTMLElement).closest<HTMLButtonElement>(
      "[data-app-toggle]",
    );
    if (!btn) return;
    const name = btn.dataset.appToggle!;
    const multi = (ev as MouseEvent).shiftKey;

    if (multi) {
      if (focused.has(name)) focused.delete(name);
      else focused.add(name);
      expanded.add(name);
    } else {
      if (expanded.has(name) && focused.has(name) && focused.size === 1) {
        expanded.delete(name);
        focused.clear();
      } else {
        expanded.add(name);
        focused.clear();
        focused.add(name);
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
    el.status.textContent = "Reset history and traceroute cache";
    schedulePaint(undefined, true);
  });

  el.btnTraceAll.addEventListener("click", async () => {
    await invoke("force_trace_all");
    el.status.textContent = "Re-tracing all destinations…";
  });

  el.btnRecenter.addEventListener("click", () => {
    recenterOnData();
    paint(true);
    el.status.textContent = "Camera recentered on active paths";
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
        setSelectedPath(routeIds[0]);
        routeInspector.showRoute(routeIds[0]);
      } else {
        setSelectedPath(null);
        routeInspector.showNode(selection);
      }
      paint(true);
      const place = selection.city || selection.hostname || selection.addr || "mapped node";
      el.status.textContent = `Inspecting ${place} · ${routeIds.length} route${routeIds.length === 1 ? "" : "s"}`;
    });
    globeReady = true;
  } catch (e) {
    el.globeStatus.textContent = `globe failed: ${String(e)}`;
  }

  try {
    const settings = await invoke<SettingsDto>("get_settings");
    privacyAccepted = !!settings.privacyAccepted;
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

  mountOnboarding(el.onboardingHost, {
    privacyAccepted,
    onAcceptPrivacy: async () => {
      privacyAccepted = true;
      await pushSettings();
    },
  });

  try {
    snapshot = await invoke<SnapshotDto>("get_snapshot");
    paint(true);
  } catch (e) {
    el.status.textContent = `Waiting for backend… (${String(e)})`;
  }

  try {
    await listen<SnapshotDto>("monitor-update", (event) => {
      schedulePaint(event.payload, false);
    });
    await listen<string>("monitor-error", (event) => {
      el.status.textContent = `Monitor error: ${event.payload}`;
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
