"use client";

import { useEffect, useRef, useState } from "react";
import { DEMO_APPS, DEMO_PATHS } from "@/lib/demo-paths";
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
      <figcaption className="globe-caption">
        demo · {DEMO_APPS.join(" · ").toLowerCase()}
      </figcaption>
    </figure>
  );
}
