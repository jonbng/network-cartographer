import type { DemoPath } from "./demo-paths";

type Point = {
  lat: number;
  lng: number;
  size: number;
  color: string;
  label: string;
  isDestination: boolean;
};

type Arc = {
  startLat: number;
  startLng: number;
  endLat: number;
  endLng: number;
  color: string[];
  stroke: number;
  animate: boolean;
};

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type GlobeInstance = any;

export type HeroGlobeHandle = {
  dispose: () => void;
  setAutoRotate: (on: boolean) => void;
};

export async function mountHeroGlobe(
  container: HTMLElement,
  paths: DemoPath[],
  options: { autoRotate: boolean; animateArcs: boolean },
): Promise<HeroGlobeHandle> {
  const Globe = (await import("globe.gl")).default;

  const globe: GlobeInstance = new Globe(container)
    .backgroundColor("rgba(0,0,0,0)")
    .showAtmosphere(true)
    .atmosphereColor("#e0a86a")
    .atmosphereAltitude(0.18)
    .globeImageUrl("/earth-dark.jpg")
    .pointAltitude((d: object) => ((d as Point).isDestination ? 0.02 : 0.01))
    .pointRadius("size")
    .pointColor("color")
    .pointsMerge(false)
    .pointLabel((d: object) => {
      const p = d as Point;
      return `<div style="font-family:ui-monospace,monospace;font-size:11px;padding:2px 0;color:#ebe8e0">${p.label}</div>`;
    })
    .arcColor("color")
    .arcStroke("stroke")
    .arcAltitudeAutoScale(0.32)
    .arcDashLength((d: object) => ((d as Arc).animate ? 0.35 : 1))
    .arcDashGap((d: object) => ((d as Arc).animate ? 0.65 : 0))
    .arcDashAnimateTime((d: object) => ((d as Arc).animate ? 2200 : 0))
    .arcsTransitionDuration(0)
    .pointsTransitionDuration(0)
    .labelsTransitionDuration(0);

  const controls = globe.controls();
  controls.autoRotate = options.autoRotate;
  controls.autoRotateSpeed = 0.2;
  controls.enableDamping = true;
  controls.dampingFactor = 0.08;
  controls.minDistance = 140;
  controls.maxDistance = 520;
  controls.enableZoom = true;
  controls.zoomSpeed = 0.7;

  // Frame the Atlantic / Americas where the demo routes live.
  globe.pointOfView({ lat: 30, lng: -60, altitude: 1.7 }, 0);

  const { points, arcs, labels } = buildGeometry(paths, options.animateArcs);
  globe.pointsData(points);
  globe.arcsData(arcs);
  globe
    .labelsData(labels)
    .labelLat("lat")
    .labelLng("lng")
    .labelText("label")
    .labelSize((d: object) => ((d as Point).isDestination ? 0.55 : 0.4))
    .labelDotRadius(0)
    .labelColor((d: object) =>
      (d as Point).isDestination
        ? "rgba(224, 168, 106, 0.9)"
        : "rgba(235, 232, 224, 0.55)",
    )
    .labelAltitude(0.018)
    .labelResolution(3);

  const resize = () => {
    const { width, height } = container.getBoundingClientRect();
    if (width > 0 && height > 0) {
      globe.width(width);
      globe.height(height);
    }
  };
  resize();
  const ro = new ResizeObserver(resize);
  ro.observe(container);

  container.style.touchAction = "none";

  return {
    dispose: () => {
      ro.disconnect();
      try {
        globe._destructor?.();
      } catch {
        // globe.gl cleanup is best-effort
      }
      container.replaceChildren();
    },
    setAutoRotate: (on: boolean) => {
      controls.autoRotate = on;
    },
  };
}

function buildGeometry(
  paths: DemoPath[],
  animateArcs: boolean,
): {
  points: Point[];
  arcs: Arc[];
  labels: Point[];
} {
  const nodeMap = new Map<string, Point>();
  const arcs: Arc[] = [];

  for (const path of paths) {
    const hops = path.hops;
    if (hops.length === 0) continue;

    hops.forEach((hop, i) => {
      const isDestination = i === hops.length - 1;
      const key = `${hop.lat.toFixed(2)},${hop.lon.toFixed(2)}`;
      const existing = nodeMap.get(key);
      if (existing) {
        if (isDestination) {
          existing.isDestination = true;
          existing.size = Math.max(existing.size, 0.62);
          existing.color = "#e0a86a";
          existing.label = hop.city;
        }
        return;
      }
      nodeMap.set(key, {
        lat: hop.lat,
        lng: hop.lon,
        size: isDestination ? 0.62 : i === 0 ? 0.38 : 0.22,
        color: isDestination ? "#e0a86a" : path.color,
        label: hop.city,
        isDestination,
      });
    });

    for (let i = 0; i < hops.length - 1; i++) {
      const a = hops[i];
      const b = hops[i + 1];
      const isLast = i === hops.length - 2;
      arcs.push({
        startLat: a.lat,
        startLng: a.lon,
        endLat: b.lat,
        endLng: b.lon,
        color: isLast
          ? [path.color, "#e0a86a"]
          : [path.color, lighten(path.color, 0.12)],
        stroke: isLast ? 0.7 : 0.45,
        animate: animateArcs,
      });
    }
  }

  const points = [...nodeMap.values()];
  const labels = pickLabels(points);
  return { points, arcs, labels };
}

function pickLabels(points: Point[]): Point[] {
  const seen = new Set<string>();
  const out: Point[] = [];
  const ranked = [...points].sort(
    (a, b) => Number(b.isDestination) - Number(a.isDestination),
  );
  for (const p of ranked) {
    if (!p.label || seen.has(p.label)) continue;
    seen.add(p.label);
    out.push(p);
    if (out.length >= 10) break;
  }
  return out;
}

function lighten(hex: string, amount: number): string {
  const raw = hex.replace("#", "");
  if (!/^[0-9a-f]{6}$/i.test(raw)) return hex;
  const n = (i: number) =>
    Math.min(255, Math.round(parseInt(raw.slice(i, i + 2), 16) + 255 * amount));
  return `#${[0, 2, 4].map((i) => n(i).toString(16).padStart(2, "0")).join("")}`;
}
