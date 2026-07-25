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
        const handle = await mountHeroGlobe(stageRef.current, DEMO_PATHS, {
          autoRotate: !reduceMotion,
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

    const motionQuery = window.matchMedia("(prefers-reduced-motion: reduce)");
    const onMotionChange = () => {
      handleRef.current?.setAutoRotate(!motionQuery.matches);
    };
    motionQuery.addEventListener("change", onMotionChange);

    return () => {
      cancelled = true;
      motionQuery.removeEventListener("change", onMotionChange);
      handleRef.current?.dispose();
      handleRef.current = null;
    };
  }, []);

  return (
    <figure className="globe-block">
      <div className="globe-shell">
        <div className="globe-frame">
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

        <aside className="globe-legend" aria-label="Demo routes">
          <p className="globe-legend-title">demo routes</p>
          <ul>
            {DEMO_PATHS.map((path) => {
              const last = path.hops[path.hops.length - 1];
              return (
                <li key={path.id}>
                  <span className="swatch" style={{ background: path.color }} />
                  <span className="legend-app">{path.app}</span>
                  <span className="legend-host">{path.host}</span>
                  <span className="legend-meta">
                    {last?.city ?? "—"} · {path.rttMs.toFixed(0)}ms
                  </span>
                </li>
              );
            })}
          </ul>
        </aside>
      </div>
      <figcaption className="globe-caption">
        Sample data — not your machine. Drag to orbit, scroll to zoom.
      </figcaption>
    </figure>
  );
}
