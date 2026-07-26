import type { GlobeHop, GlobePath, HopSelection, NetworkOrigin } from "./globe";

type InspectorOptions = {
  onClose: () => void;
  onSelectRoute: (routeId: string) => void;
};

type InspectorState =
  | { kind: "closed" }
  | { kind: "route"; routeId: string; lastRoute: GlobePath | null }
  | { kind: "node"; selection: HopSelection }
  | { kind: "origin"; origin: NetworkOrigin };

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

  update(paths: GlobePath[], origin?: NetworkOrigin | null): void {
    this.paths = new Map(paths.map((path) => [path.id, path]));
    if (this.state.kind === "route") {
      const latest = this.paths.get(this.state.routeId) ?? null;
      if (latest) this.state.lastRoute = latest;
    }
    if (this.state.kind === "origin" && origin) {
      this.state.origin = origin;
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

  showOrigin(origin: NetworkOrigin, trigger?: HTMLElement | null): void {
    if (trigger) this.returnFocus = trigger;
    this.state = { kind: "origin", origin };
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
    if (this.state.kind === "origin") {
      this.host.innerHTML = renderOrigin(this.state.origin);
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

function renderOrigin(origin: NetworkOrigin): string {
  const exit = origin.exit;
  if (!exit) {
    return shell(
      "Network exit unavailable",
      `<div class="inspector-empty">The public exit could not be located yet. Route monitoring continues normally.</div>`,
      "Network origin",
    );
  }
  const place = exit.city
    ? `${exit.city}${exit.country ? `, ${exit.country}` : ""}`
    : "Location unavailable";
  const assessment = assessmentLabel(origin.assessment);
  const evidence = origin.evidence.length
    ? origin.evidence
        .map(
          (item) => `<li class="origin-evidence ${item.strength}">
            <span></span><div><strong>${escapeHtml(evidenceKind(item.kind))}</strong><p>${escapeHtml(item.label)}</p></div>
          </li>`,
        )
        .join("")
    : `<li class="origin-evidence supporting"><span></span><div><strong>No local signal</strong><p>No explicit proxy or default tunnel interface was found.</p></div></li>`;
  const source = exit.source === "hosted-egress"
    ? "Observed public egress"
    : "Traceroute consensus fallback";
  return shell(
    "Primary network exit",
    `<div class="inspector-body">
      <section class="origin-hero">
        <span class="origin-beacon" aria-hidden="true"><i></i></span>
        <div><span class="inspector-label">${escapeHtml(source)}</span><h3>${escapeHtml(place)}</h3>
        ${exit.ip ? `<code>${escapeHtml(exit.ip)}</code>` : ""}</div>
      </section>
      <section class="route-metrics origin-metrics">
        <div><span>Assessment</span><strong class="${assessment.tone}">${escapeHtml(assessment.label)}</strong></div>
        <div><span>Location confidence</span><strong>${escapeHtml(confidenceLabel(exit.confidence) || "unscored")}</strong></div>
        <div><span>Network</span><strong>${escapeHtml(exit.organization || "—")}</strong></div>
        <div><span>ASN</span><strong>${exit.asn ? `AS${exit.asn}` : "—"}</strong></div>
      </section>
      <section class="origin-evidence-section">
        <div class="section-heading"><span>Routing evidence</span><b>${origin.evidence.length}</b></div>
        <ul class="origin-evidence-list">${evidence}</ul>
      </section>
      <aside class="accuracy-note origin-note">
        <strong>How to read this</strong>
        <p>This is where the public internet sees this connection, not your physical location. Individual applications may use different routes. “No evidence” does not prove a direct connection.</p>
      </aside>
    </div>`,
    "Network origin",
  );
}

function assessmentLabel(assessment: NetworkOrigin["assessment"]): { label: string; tone: string } {
  if (assessment === "proxy_and_tunnel") return { label: "Proxy + tunnel signals", tone: "partial" };
  if (assessment === "proxy_configured") return { label: "Proxy configured", tone: "partial" };
  if (assessment === "tunnel_likely") return { label: "VPN / tunnel likely", tone: "partial" };
  if (assessment === "no_evidence") return { label: "No VPN / proxy evidence", tone: "confirmed" };
  return { label: "Inspection unavailable", tone: "" };
}

function evidenceKind(kind: NetworkOrigin["evidence"][number]["kind"]): string {
  if (kind === "default_interface") return "Default route";
  if (kind === "system_proxy") return "System proxy";
  return "Environment proxy";
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
  const tracePending = path.status === "running" || path.status === "queued";
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
      ${renderDomainEvidence(path)}
      ${inactive ? `<div class="inactive-notice">No longer active · showing the last observed route</div>` : ""}
    </section>
    ${tracePending ? `<div class="trace-incomplete" role="status">
      <span class="trace-progress-pulse" aria-hidden="true"></span>
      <div><strong>${path.status === "running" ? "Traceroute still running" : "Waiting to start traceroute"}</strong><span>This route is incomplete. Hops and locations may change as replies arrive.</span></div>
    </div>` : ""}
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

function renderDomainEvidence(path: GlobePath): string {
  if (!path.domainSource || path.domainSource === "ip") return "";
  const confidence = path.domainConfidence ?? "none";
  const alternatives = path.domainAlternativesCount
    ? ` · ${path.domainAlternativesCount} other candidate${path.domainAlternativesCount === 1 ? "" : "s"}`
    : "";
  const label = confidence === "low" ? "best guess" : confidence;
  return `<div class="location-evidence destination-evidence"><span class="confidence ${escapeHtml(confidence)}">${escapeHtml(label)}</span><span>${escapeHtml(path.domainSource)}${escapeHtml(alternatives)}</span></div>`;
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
  const firstPublicTtl = path.hops
    .slice()
    .sort((a, b) => a.ttl - b.ttl)
    .find(
      (hop) =>
        hop.addr != null &&
        hop.lat != null &&
        hop.lon != null &&
        hop.geoNote !== "private/local",
    )?.ttl;
  return `<ol class="hop-timeline">${path.hops
    .slice()
    .sort((a, b) => a.ttl - b.ttl)
    .map((hop) => {
      const networkKey = networkName(hop);
      const showNetwork = hasAsn && networkKey !== previousNetwork;
      previousNetwork = networkKey;
      return `${showNetwork ? `<li class="network-boundary"><span>${escapeHtml(networkKey)}</span></li>` : ""}${renderHop(path, hop, hop.ttl === firstPublicTtl)}`;
    })
    .join("")}</ol>`;
}

function renderHop(path: GlobePath, hop: GlobeHop, isFirstPublic: boolean): string {
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
      <div class="hop-place">${isFirstPublic ? `<em class="public-entry">First visible public hop</em>` : ""}${isTarget ? `<em>Final destination</em>` : ""}<span>${escapeHtml(location)}</span></div>
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
  if (source === "hosted" || source === "geolite") return "Network Cartographer hosted geo";
  if (source === "geoip" || source === "ipwho") return "legacy online GeoIP estimate";
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
