import type { GlobeHop, GlobePath, HopSelection } from "./globe";

type InspectorOptions = {
  onClose: () => void;
  onSelectRoute: (routeId: string) => void;
};

type InspectorState =
  | { kind: "closed" }
  | { kind: "route"; routeId: string; lastRoute: GlobePath | null }
  | { kind: "node"; selection: HopSelection };

export class RouteInspector {
  private state: InspectorState = { kind: "closed" };
  private paths = new Map<string, GlobePath>();
  private returnFocus: HTMLElement | null = null;

  constructor(
    private readonly host: HTMLElement,
    private readonly options: InspectorOptions,
  ) {
    host.addEventListener("click", (event) => {
      const close = (event.target as HTMLElement).closest("[data-inspector-close]");
      if (close) {
        this.close();
        return;
      }
      const route = (event.target as HTMLElement).closest<HTMLElement>(
        "[data-inspector-route]",
      );
      if (route?.dataset.inspectorRoute) {
        this.showRoute(route.dataset.inspectorRoute);
        this.options.onSelectRoute(route.dataset.inspectorRoute);
      }
    });
  }

  get selectedRouteId(): string | null {
    return this.state.kind === "route" ? this.state.routeId : null;
  }

  update(paths: GlobePath[]): void {
    this.paths = new Map(paths.map((path) => [path.id, path]));
    if (this.state.kind === "route") {
      const latest = this.paths.get(this.state.routeId) ?? null;
      if (latest) this.state.lastRoute = latest;
    }
    this.render();
  }

  showRoute(routeId: string, trigger?: HTMLElement | null): void {
    if (trigger) this.returnFocus = trigger;
    const latest = this.paths.get(routeId) ?? null;
    this.state = { kind: "route", routeId, lastRoute: latest };
    this.open();
  }

  showNode(selection: HopSelection, trigger?: HTMLElement | null): void {
    if (trigger) this.returnFocus = trigger;
    this.state = { kind: "node", selection };
    this.open();
  }

  close(restoreFocus = true): void {
    if (this.state.kind === "closed") return;
    this.state = { kind: "closed" };
    this.host.hidden = true;
    document.getElementById("app")?.classList.remove("inspector-open");
    this.options.onClose();
    if (restoreFocus) this.returnFocus?.focus({ preventScroll: true });
    this.returnFocus = null;
  }

  private open(): void {
    this.host.hidden = false;
    document.getElementById("app")?.classList.add("inspector-open");
    this.render();
  }

  private render(): void {
    if (this.state.kind === "closed") return;
    if (this.state.kind === "node") {
      this.host.innerHTML = renderNodeChoices(this.state.selection, this.paths);
      return;
    }

    const route = this.paths.get(this.state.routeId) ?? this.state.lastRoute;
    if (!route) {
      this.host.innerHTML = shell(
        "Route unavailable",
        `<div class="inspector-empty">This route is no longer present in the live snapshot.</div>`,
      );
      return;
    }
    this.host.innerHTML = renderRoute(route, !this.paths.has(this.state.routeId));
  }
}

function shell(title: string, body: string, kicker = "Route intelligence"): string {
  return `<div class="inspector-head">
    <div>
      <span class="eyebrow">${escapeHtml(kicker)}</span>
      <h2 id="inspector-title">${escapeHtml(title)}</h2>
    </div>
    <button class="inspector-close" type="button" data-inspector-close aria-label="Close route inspector">
      <svg viewBox="0 0 24 24" fill="none" aria-hidden="true"><path d="m6 6 12 12M18 6 6 18"></path></svg>
    </button>
  </div>${body}`;
}

function renderNodeChoices(
  selection: HopSelection,
  paths: Map<string, GlobePath>,
): string {
  const place = selection.city
    ? `${selection.city}${selection.country ? `, ${selection.country}` : ""}`
    : selection.hostname || selection.addr || "Mapped network node";
  const choices = selection.routes
    .filter((choice) => paths.has(choice.pathId))
    .map((choice) => {
      const path = paths.get(choice.pathId)!;
      return `<button type="button" class="route-choice" data-inspector-route="${escapeHtml(path.id)}">
        <span class="swatch" style="background:${path.color};box-shadow:0 0 9px ${path.color}"></span>
        <span class="route-choice-main">
          <strong>${escapeHtml(path.app)}</strong>
          <small>${escapeHtml(path.host)} · hop ${choice.ttl}${choice.rttMs != null ? ` · ${Math.round(choice.rttMs)}ms` : ""}</small>
        </span>
        <span class="route-choice-arrow">→</span>
      </button>`;
    })
    .join("");
  const evidence = renderLocationEvidence(
    selection.geoSource,
    selection.geoConfidence,
    selection.geoNote,
  );
  const network = selection.org
    ? `<div class="node-network">${escapeHtml(selection.org)}${selection.asn ? ` · AS${selection.asn}` : ""}</div>`
    : "";

  return shell(
    place,
    `<div class="inspector-body">
      <section class="node-summary">
        <span class="inspector-label">Shared node</span>
        ${selection.hostname ? `<strong>${escapeHtml(selection.hostname)}</strong>` : ""}
        ${selection.addr ? `<code>${escapeHtml(selection.addr)}</code>` : ""}
        ${network}${evidence}
      </section>
      <section>
        <div class="section-heading"><span>Routes through this node</span><b>${selection.routes.length}</b></div>
        <div class="route-choices">${choices || `<div class="inspector-empty">No active routes remain.</div>`}</div>
      </section>
      ${accuracyNote()}
    </div>`,
    "Node intelligence",
  );
}

function renderRoute(path: GlobePath, inactive: boolean): string {
  const answered = path.hops.filter((hop) => hop.addr != null).length;
  const located = path.hops.filter((hop) => hop.lat != null && hop.lon != null).length;
  const lastReply = [...path.hops].reverse().find((hop) => hop.rttMs != null)?.rttMs ?? null;
  const rtt = path.reachedTarget ? path.targetRttMs : lastReply;
  const stateClass = path.reachedTarget ? "confirmed" : path.status === "failed" ? "failed" : "partial";
  const stateText = path.reachedTarget
    ? "Target reached"
    : path.status === "running" || path.status === "queued"
      ? path.status
      : path.status === "failed"
        ? "Trace failed"
        : "Partial route";
  const timeline = renderTimeline(path);
  const body = `<div class="inspector-body">
    <section class="route-identity">
      <div class="route-app"><i style="background:${path.color};box-shadow:0 0 10px ${path.color}"></i>${escapeHtml(path.app)}</div>
      <h3>${escapeHtml(path.host)}</h3>
      <code>${escapeHtml(path.ip)}:${path.port} · ${escapeHtml(path.protocol)}</code>
      ${inactive ? `<div class="inactive-notice">No longer active · showing the last observed route</div>` : ""}
    </section>
    <section class="route-metrics">
      <div><span>Trace</span><strong class="${stateClass}">${escapeHtml(stateText)}</strong></div>
      <div><span>${path.reachedTarget ? "End-to-end RTT" : "Last reply RTT"}</span><strong>${rtt != null ? `${Math.round(rtt)}ms` : "—"}</strong></div>
      <div><span>Responses</span><strong>${answered}/${path.hops.length || "—"}</strong></div>
      <div><span>Mapped</span><strong>${located}/${answered || "—"}</strong></div>
    </section>
    <section class="route-timeline-section">
      <div class="section-heading"><span>Observed route</span><b>${path.hops.length} TTL${path.hops.length === 1 ? "" : "s"}</b></div>
      ${timeline}
    </section>
    ${path.error ? `<div class="trace-error">${escapeHtml(path.error)}</div>` : ""}
    ${accuracyNote()}
  </div>`;
  return shell(path.reachedTarget ? "Confirmed route" : "Route evidence", body);
}

function renderTimeline(path: GlobePath): string {
  if (path.hops.length === 0) {
    const message = path.status === "queued" || path.status === "running"
      ? `Traceroute is ${path.status}…`
      : "No hop responses were recorded.";
    return `<div class="inspector-empty">${escapeHtml(message)}</div>`;
  }

  let previousNetwork = "__start__";
  const hasAsn = path.hops.some((hop) => hop.asn != null);
  return `<ol class="hop-timeline">${path.hops
    .slice()
    .sort((a, b) => a.ttl - b.ttl)
    .map((hop) => {
      const networkKey = networkName(hop);
      const showNetwork = hasAsn && networkKey !== previousNetwork;
      previousNetwork = networkKey;
      return `${showNetwork ? `<li class="network-boundary"><span>${escapeHtml(networkKey)}</span></li>` : ""}${renderHop(path, hop)}`;
    })
    .join("")}</ol>`;
}

function renderHop(path: GlobePath, hop: GlobeHop): string {
  const isTarget = path.reachedTarget && hop.addr === path.ip;
  const timedOut = hop.addr == null;
  const location = hop.city
    ? `${hop.city}${hop.country ? `, ${hop.country}` : ""}`
    : hop.geoNote === "private/local"
      ? "Private/local network"
      : timedOut
        ? "No response"
        : "Location unavailable";
  const primary = hop.hostname || hop.addr || "Request timed out";
  const secondary = hop.hostname && hop.addr ? `<code>${escapeHtml(hop.addr)}</code>` : "";
  const evidence = renderLocationEvidence(
    hop.geoSource ?? null,
    hop.geoConfidence ?? null,
    hop.geoNote ?? null,
  );
  const classes = [timedOut ? "timeout" : "", isTarget ? "target" : ""].filter(Boolean).join(" ");
  return `<li class="hop-row ${classes}">
    <div class="hop-marker"><span>${hop.ttl}</span></div>
    <div class="hop-copy">
      <div class="hop-primary"><strong>${escapeHtml(primary)}</strong><b>${hop.rttMs != null ? `${Math.round(hop.rttMs)}ms` : "—"}</b></div>
      ${secondary}
      <div class="hop-place">${isTarget ? `<em>Final destination</em>` : ""}<span>${escapeHtml(location)}</span></div>
      ${evidence}
    </div>
  </li>`;
}

function networkName(hop: GlobeHop): string {
  if (hop.org) return `${hop.org}${hop.asn ? ` · AS${hop.asn}` : ""}`;
  if (hop.geoNote === "private/local") return "Local/private network";
  return "Network not identified";
}

function renderLocationEvidence(
  source: string | null,
  score: number | null,
  note: string | null,
): string {
  if (!source && !note) return "";
  const confidence = confidenceLabel(score);
  return `<div class="location-evidence">
    ${source ? `<span class="confidence ${confidence}">${confidence || "unscored"}</span><span>${escapeHtml(sourceLabel(source))}</span>` : ""}
    ${note && note !== "private/local" ? `<small>${escapeHtml(note)}</small>` : ""}
  </div>`;
}

function confidenceLabel(score: number | null): string {
  if (score == null) return "";
  if (score >= 0.75) return "high";
  if (score >= 0.55) return "medium";
  return "low";
}

function sourceLabel(source: string): string {
  if (source === "mmdb") return "local MaxMind city data";
  if (source === "geoip" || source === "ipwho") return "online GeoIP estimate";
  if (source.startsWith("rdns")) return "reverse-DNS location hint";
  if (source.startsWith("inferred")) return "route and latency inference";
  return source;
}

function accuracyNote(): string {
  return `<aside class="accuracy-note">
    <strong>How to read this</strong>
    <p>Traceroute observes responding network hops. Locations are estimates, and globe arcs show topology—not physical cable routes. Confidence labels are heuristic, not probabilities.</p>
  </aside>`;
}

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}
