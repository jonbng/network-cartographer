import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  clearGlobe,
  colorForKey,
  initGlobe,
  recenterOnData,
  setDensity,
  setFocusedApps,
  setHopClickHandler,
  setLabelsVisible,
  updateAllPaths,
  type GlobePath,
} from "./globe";
import { mountOnboarding } from "./onboarding";

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
  asn?: number | null;
  org?: string | null;
  pathChanged?: boolean;
  trace: TraceDto;
};

type AppDto = {
  id?: string;
  name: string;
  path?: string | null;
  pids: number[];
  destCount: number;
  hits: number;
  hitsPerSec?: number;
  activity?: number;
  destinations: DestDto[];
};

type SettingsDto = {
  externalOnly: boolean;
  includeUdp: boolean;
  tracesEnabled: boolean;
  pollIntervalMs: number;
  geoLocalOnly?: boolean;
  showLowConfidence?: boolean;
  confidenceMin?: number;
  globeDensity?: string;
  captureSni?: boolean;
  historyEnabled?: boolean;
  privacyAccepted?: boolean;
};

type SnapshotDto = {
  apps: AppDto[];
  appCount: number;
  destCount: number;
  liveConnections: number;
  missingPid: number;
  externalOnly: boolean;
  includeUdp: boolean;
  tracesEnabled: boolean;
  traceStats: { queued: number; running: number; done: number; failed: number };
  geoBackend?: string;
  geoMmdb?: boolean;
  geoAsnMmdb?: boolean;
  elevated?: boolean;
  elevationHint?: string | null;
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
  togTraces: document.getElementById("tog-traces") as HTMLInputElement,
  togLabels: document.getElementById("tog-labels") as HTMLInputElement,
  togLocalGeo: document.getElementById("tog-local-geo") as HTMLInputElement,
  togHistory: document.getElementById("tog-history") as HTMLInputElement,
  selDensity: document.getElementById("sel-density") as HTMLSelectElement,
  btnReset: document.getElementById("btn-reset")!,
  btnTraceAll: document.getElementById("btn-trace-all")!,
  btnRecenter: document.getElementById("btn-recenter")!,
  banner: document.getElementById("banner")!,
  toast: document.getElementById("toast")!,
  onboardingHost: document.getElementById("onboarding-host")!,
  appVersion: document.getElementById("app-version")!,
  aboutModal: document.getElementById("about-modal")!,
  aboutVersion: document.getElementById("about-version")!,
  btnAbout: document.getElementById("btn-about") as HTMLButtonElement,
  btnAboutClose: document.getElementById("btn-about-close") as HTMLButtonElement,
};

let appVersion = "0.1.0";

function destName(d: DestDto): string {
  return d.displayHost || d.sni || d.host || d.ip;
}

function matchesFilter(app: AppDto, dest: DestDto): boolean {
  const q = filter.trim().toLowerCase();
  if (!q) return true;
  if (app.name.toLowerCase().includes(q)) return true;
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

function collectPaths(): GlobePath[] {
  if (!snapshot) return [];
  const paths: GlobePath[] = [];
  for (const app of snapshot.apps) {
    for (const dest of app.destinations) {
      if (!matchesFilter(app, dest)) continue;
      const hasHops = dest.trace.hops.length > 0;
      if (dest.trace.status === "queued" || dest.trace.status === "running") {
        paths.push({
          id: `${app.name}|${dest.ip}|${dest.port}`,
          app: app.name,
          host: destName(dest),
          ip: dest.ip,
          port: dest.port,
          protocol: dest.protocol,
          hits: dest.hits,
          color: colorForKey(app.name),
          hops: [],
          status: dest.trace.status,
          rttMs: null,
        });
        continue;
      }
      if (dest.trace.status !== "done" || !hasHops) continue;
      paths.push({
        id: `${app.name}|${dest.ip}|${dest.port}`,
        app: app.name,
        host: destName(dest),
        ip: dest.ip,
        port: dest.port,
        protocol: dest.protocol,
        hits: dest.hits,
        color: colorForKey(app.name),
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
        })),
        status: "done",
        rttMs: finalRtt(dest.trace.hops),
      });
    }
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
    };
    for (const dest of app.destinations) {
      if (!matchesFilter(app, dest)) continue;
      group.totalDests += 1;
      const id = `${app.name}|${dest.ip}|${dest.port}`;
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
      return `${g.name}|${g.traced}/${g.totalDests}|${g.activity.toFixed(1)}|${dests}`;
    })
    .join("||");
  return `${foc}#${exp}#${filter}#${body}`;
}

function currentSettings(): SettingsDto {
  return {
    externalOnly: el.togExternal.checked,
    // Retained in the settings schema for compatibility; remote UDP peers are
    // not exposed by the current cross-platform socket collector.
    includeUdp: false,
    tracesEnabled: el.togTraces.checked,
    pollIntervalMs: 1000,
    geoLocalOnly: el.togLocalGeo.checked,
    showLowConfidence: true,
    confidenceMin: 0.45,
    globeDensity: el.selDensity.value,
    captureSni: false,
    historyEnabled: el.togHistory.checked,
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

  const paths = collectPaths();
  const mapPaths = paths.filter((p) => p.status === "done" && p.hops.length > 0);
  const groups = collectAppGroups(paths);
  setFocusedApps([...focused]);

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
      snapshot.elevated ? 1 : 0,
      [...focused].join(","),
      el.selDensity.value,
    ].join("|");

    if (headerSig !== lastHeaderSig) {
      lastHeaderSig = headerSig;
      el.statApps.textContent = `${snapshot.appCount} apps`;
      el.statTraces.textContent = snapshot.tracesEnabled
        ? `tr q${t.queued} r${t.running} ✓${t.done}`
        : "tr off";
      if (el.statGeo) {
        const backend = snapshot.geoBackend ?? "api";
        el.statGeo.textContent = `geo ${backend}`;
        el.statGeo.classList.toggle("accent", !!snapshot.geoMmdb);
        el.statGeo.title = snapshot.geoMmdb
          ? "Local MaxMind city DB loaded"
          : "Online geo only";
      }
      if (el.banner) {
        if (snapshot.elevationHint && !snapshot.elevated) {
          el.banner.hidden = false;
          el.banner.textContent = snapshot.elevationHint;
        } else {
          el.banner.hidden = true;
        }
      }
      const missing =
        snapshot.missingPid > 0 ? ` · ${snapshot.missingPid} without pid` : "";
      el.status.textContent = `Live · ${snapshot.liveConnections} conns · ${mapPaths.length} paths${missing}`;
      el.btnClearFocus.hidden = focused.size === 0;
      el.sidebarSub.textContent =
        focused.size === 0
          ? "Click focus · Shift+click multi-select"
          : `Focused: ${[...focused].join(", ")}`;
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
  el.statPaths.textContent = `${pathCount} paths`;
  el.statHops.textContent = `${hopCount} hops · ${destCount} ★`;
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
  if (status === "done") return `<span class="badge ok">traced</span>`;
  if (status === "running") return `<span class="badge run">tracing…</span>`;
  if (status === "queued") return `<span class="badge queue">queued</span>`;
  if (status === "failed") return `<span class="badge fail">fail</span>`;
  return `<span class="badge">${escapeHtml(status)}</span>`;
}

function renderSidebar(groups: AppGroup[]) {
  if (groups.length === 0) {
    el.appList.innerHTML = `<div class="empty">No applications with internet connections yet.</div>`;
    return;
  }

  el.appList.innerHTML = groups
    .map((g) => {
      const isOpen = expanded.has(g.name) || focused.has(g.name);
      const isFocused = focused.has(g.name);
      const dim = focused.size > 0 && !isFocused;
      const act =
        g.activity > 0.05
          ? `<span class="act"> · ${g.activity.toFixed(1)}/s</span>`
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
              return `<div class="dest-row${destMeta?.pathChanged ? " flash" : ""}" data-dest-host="${escapeHtml(p.host)}" title="${escapeHtml(p.ip)}">
                <span class="dest-star">★</span>
                <span class="dest-main">
                  <span class="dest-host">${escapeHtml(p.host)}${org}${changed}</span>
                  <span class="dest-meta">:${p.port} · ${escapeHtml(p.protocol)} · ${rtt}${destCity}</span>
                </span>
                <span class="dest-side">${mapped > 0 ? `${mapped} hops` : statusBadge(p.status)}</span>
              </div>`;
            })
            .join("")
        : "";

      return `<div class="app-card${isFocused ? " focused" : ""}${dim ? " dim" : ""}" data-app="${escapeHtml(g.name)}">
        <button type="button" class="app-row" data-app-toggle="${escapeHtml(g.name)}">
          <span class="swatch" style="background:${g.color};box-shadow:0 0 10px ${g.color}"></span>
          <span class="app-main">
            <span class="app-name">${escapeHtml(g.name)}</span>
            <span class="app-meta">${g.traced}/${g.totalDests} dests · ${g.mappedHops} hops${act}</span>
          </span>
          <span class="chev">${isOpen ? "▾" : "▸"}</span>
        </button>
        ${isOpen ? `<div class="dest-list">${destRows || `<div class="empty sm">No destinations</div>`}</div>` : ""}
      </div>`;
    })
    .join("");
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
    el.togTraces,
    el.togLocalGeo,
    el.togHistory,
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
    if (ev.key === "Escape" && !el.aboutModal.hidden) {
      el.aboutModal.hidden = true;
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
    setHopClickHandler((apps, hosts) => {
      focused.clear();
      for (const a of apps) {
        focused.add(a);
        expanded.add(a);
      }
      paint(true);
      // scroll first matching app into view
      const card = el.appList.querySelector(
        `[data-app="${CSS.escape(apps[0] ?? "")}"]`,
      );
      card?.scrollIntoView({ block: "nearest", behavior: "smooth" });
      if (hosts[0]) {
        el.status.textContent = `Selected hop · ${apps.join(", ")} · ${hosts[0]}`;
      }
    });
    globeReady = true;
  } catch (e) {
    el.globeStatus.textContent = `globe failed: ${String(e)}`;
  }

  try {
    const settings = await invoke<SettingsDto>("get_settings");
    privacyAccepted = !!settings.privacyAccepted;
    el.togExternal.checked = settings.externalOnly;
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
