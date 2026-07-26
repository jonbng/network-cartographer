"use client";

import { useEffect, useRef, useState } from "react";
import { DEMO_PATHS } from "@/lib/demo-paths";
import type { HeroGlobeHandle } from "@/lib/hero-globe-engine";

export function HeroGlobe() {
  const stageRef = useRef<HTMLDivElement>(null);
  const handleRef = useRef<HeroGlobeHandle | null>(null);
  const [status, setStatus] = useState<"loading" | "ready" | "error">("loading");

  useEffect(() => {
    const container = stageRef.current;
    if (!container) return;

    let cancelled = false;
    const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

    void (async () => {
      try {
        const { mountHeroGlobe } = await import("@/lib/hero-globe-engine");
        if (cancelled || !stageRef.current) return;
        // Keep the camera on the demo hemisphere; no auto-spin.
        const handle = await mountHeroGlobe(stageRef.current, DEMO_PATHS, {
          autoRotate: false,
          animateArcs: !reduceMotion,
        });
        if (cancelled) {
          handle.dispose();
          return;
        }
        handleRef.current = handle;
        setStatus("ready");
      } catch {
        if (!cancelled) setStatus("error");
      }
    })();

    return () => {
      cancelled = true;
      handleRef.current?.dispose();
      handleRef.current = null;
    };
  }, []);

  const apps = [...new Map(DEMO_PATHS.map((path) => [path.app, path])).values()];

  return (
    <figure className="globe-block">
      <div className="globe-shell">
        <aside className="preview-sidebar" aria-label="Sample active applications">
          <div className="preview-brand">
            <span>Network Cartographer</span>
            <strong>Apps and destinations</strong>
          </div>

          <div className="preview-sidebar-heading">
            <strong><span>{apps.length}</span> Active applications</strong>
          </div>

          <div className="preview-apps">
            {apps.map((app, appIndex) => {
              const routes = DEMO_PATHS.filter((path) => path.app === app.app);
              return (
                <div className={`preview-app${appIndex === 0 ? " focused" : ""}`} key={app.app}>
                  <div className="preview-app-row">
                    <span className="preview-app-dot" style={{ background: app.color }} />
                    <span className="preview-app-copy">
                      <strong>{app.app}</strong>
                      <small>{routes[0]?.host}</small>
                    </span>
                    <span className="preview-route-count">{routes.length}</span>
                  </div>
                </div>
              );
            })}
          </div>
        </aside>

        <div className="globe-frame">
          <div className="preview-map-grid" aria-hidden="true" />
          <header className="preview-map-header">
            <div>
              <span>Interactive preview</span>
              <strong>Network map</strong>
              <small>Sample data · {apps.length} apps · {DEMO_PATHS.length} routes</small>
            </div>
          </header>

          <span className="globe-demo-badge">Product preview · Sample data</span>
          {status !== "ready" && (
            <p className="globe-fallback" aria-live="polite">
              {status === "error"
                ? "Globe preview unavailable in this browser."
                : "Loading globe…"}
            </p>
          )}
          <div
            className="globe-stage"
            ref={stageRef}
            aria-label="Demo network routes on a globe"
          />
        </div>
      </div>
      <figcaption className="globe-caption">
        Preview only — the actual tool runs locally with live data from your machine. Drag to orbit,
        scroll to zoom.
      </figcaption>
    </figure>
  );
}
