export const INTRO_LOCK_MS = 2_000;

export type IntroStatusInput = {
  appCount: number;
  destCount: number;
  tracesEnabled: boolean;
  queued: number;
  running: number;
  done: number;
  failed: number;
  mappedRoutes: number;
};

export type IntroStatus = {
  title: string;
  detail: string;
  emptyTitle: string;
  emptyDetail: string;
};

export function introStatus(input: IntroStatusInput | null): IntroStatus {
  if (!input) {
    return status(
      "Starting the local monitor",
      "Connecting to the collector…",
      "Starting the local monitor",
      "The first connection snapshot will appear shortly.",
    );
  }

  if (input.mappedRoutes > 0) {
    const routes = `${input.mappedRoutes} ${input.mappedRoutes === 1 ? "route" : "routes"}`;
    return status(
      `${routes} ready`,
      input.running + input.queued > 0 ? "More routes are still being measured." : "The live map is ready to explore.",
      "No routes match this view",
      "Clear the search or app focus to show the mapped routes.",
    );
  }

  if (input.appCount === 0 || input.destCount === 0) {
    return status(
      "Waiting for internet activity",
      "Open a site hosted farther away to give the map a useful route.",
      "Waiting for internet activity",
      "For a good demo, open a website hosted in another region or continent.",
    );
  }

  if (!input.tracesEnabled) {
    return status(
      "Connections found",
      "Route mapping is turned off in Advanced options.",
      "Route mapping is off",
      "Enable Traceroutes in Advanced options to draw paths on the globe.",
    );
  }

  const pending = input.running + input.queued;
  if (pending > 0) {
    const routes = `${pending} ${pending === 1 ? "route" : "routes"}`;
    return status(
      `Mapping ${routes}`,
      "Traceroutes take a few seconds and appear hop by hop.",
      `Mapping ${routes}`,
      "The first path will appear as soon as responding hops are located.",
    );
  }

  if (input.failed > 0 && input.done === 0) {
    return status(
      "Connections found",
      "The first traceroutes did not return a usable path.",
      "No route replies yet",
      "Connections remain available in the sidebar even when routers do not answer traceroute.",
    );
  }

  return status(
    "Preparing route measurements",
    `${input.destCount} ${input.destCount === 1 ? "destination" : "destinations"} found.`,
    "Preparing route measurements",
    "Destinations are ready; traceroutes will begin shortly.",
  );
}

function status(title: string, detail: string, emptyTitle: string, emptyDetail: string): IntroStatus {
  return { title, detail, emptyTitle, emptyDetail };
}
