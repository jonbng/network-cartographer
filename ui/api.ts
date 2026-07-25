type ApiEvent<T> = { payload: T };
type EventHandler<T> = (event: ApiEvent<T>) => void;

const routes: Record<string, { method: string; path: string }> = {
  get_snapshot: { method: "GET", path: "/api/snapshot" },
  refresh_now: { method: "POST", path: "/api/refresh" },
  get_settings: { method: "GET", path: "/api/settings" },
  set_settings: { method: "PUT", path: "/api/settings" },
  reset_monitor: { method: "POST", path: "/api/reset" },
  force_trace_all: { method: "POST", path: "/api/trace-all" },
};

export async function invoke<T = void>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  const route = routes[command];
  if (!route) throw new Error(`Unknown API command: ${command}`);

  const response = await fetch(route.path, {
    method: route.method,
    headers:
      route.method === "GET"
        ? undefined
        : {
            "Content-Type": "application/json",
            "X-Network-Cartographer": "1",
          },
    body:
      route.method === "GET" || route.method === "POST"
        ? undefined
        : JSON.stringify(args?.settings ?? args ?? {}),
  });
  if (!response.ok) {
    const detail = (await response.text()).trim();
    throw new Error(detail || `${response.status} ${response.statusText}`);
  }
  const text = await response.text();
  return text ? (JSON.parse(text) as T) : (undefined as T);
}

export async function getVersion(): Promise<string> {
  const response = await fetch("/api/version");
  if (!response.ok) throw new Error("Version endpoint unavailable");
  const data = (await response.json()) as { version: string };
  return data.version;
}

export async function listen<T>(
  eventName: string,
  handler: EventHandler<T>,
): Promise<() => void> {
  const source = eventSource();
  const listener = (event: MessageEvent<string>) => {
    handler({ payload: JSON.parse(event.data) as T });
  };
  source.addEventListener(eventName, listener as EventListener);
  return () => source.removeEventListener(eventName, listener as EventListener);
}

let source: EventSource | null = null;

function eventSource(): EventSource {
  if (!source || source.readyState === EventSource.CLOSED) {
    source = new EventSource("/api/events");
  }
  return source;
}
