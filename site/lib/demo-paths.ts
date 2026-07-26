export type DemoHop = {
  ttl: number;
  lat: number;
  lon: number;
  city: string;
  rttMs: number;
};

export type DemoPath = {
  id: string;
  app: string;
  host: string;
  ip: string;
  port: number;
  color: string;
  rttMs: number;
  hops: DemoHop[];
};

/** Static routes adapted from experiments/cli-globe mock data. */
export const DEMO_PATHS: DemoPath[] = [
  {
    id: "firefox|cloudflare|443",
    app: "Firefox",
    host: "cdn.cloudflare.com",
    ip: "104.16.132.229",
    port: 443,
    color: "#e0a86a",
    rttMs: 32,
    hops: [
      { ttl: 1, lat: 18.47, lon: -66.11, city: "San Juan", rttMs: 2 },
      { ttl: 5, lat: 25.76, lon: -80.19, city: "Miami", rttMs: 12 },
      { ttl: 9, lat: 40.71, lon: -74.01, city: "New York", rttMs: 32 },
    ],
  },
  {
    id: "firefox|mozilla|443",
    app: "Firefox",
    host: "services.mozilla.com",
    ip: "34.120.208.123",
    port: 443,
    color: "#e0a86a",
    rttMs: 68,
    hops: [
      { ttl: 1, lat: 18.47, lon: -66.11, city: "San Juan", rttMs: 2 },
      { ttl: 5, lat: 25.76, lon: -80.19, city: "Miami", rttMs: 13 },
      { ttl: 11, lat: 37.77, lon: -122.42, city: "San Francisco", rttMs: 68 },
    ],
  },
  {
    id: "spotify|audio|443",
    app: "Spotify",
    host: "audio-fa.scdn.co",
    ip: "35.186.224.25",
    port: 443,
    color: "#8fbf9f",
    rttMs: 91,
    hops: [
      { ttl: 1, lat: 18.47, lon: -66.11, city: "San Juan", rttMs: 2 },
      { ttl: 6, lat: 40.71, lon: -74.01, city: "New York", rttMs: 34 },
      { ttl: 12, lat: 53.35, lon: -6.26, city: "Dublin", rttMs: 91 },
    ],
  },
  {
    id: "code|github|443",
    app: "Code",
    host: "api.github.com",
    ip: "140.82.114.6",
    port: 443,
    color: "#7a9eb8",
    rttMs: 47,
    hops: [
      { ttl: 1, lat: 18.47, lon: -66.11, city: "San Juan", rttMs: 2 },
      { ttl: 5, lat: 25.76, lon: -80.19, city: "Miami", rttMs: 12 },
      { ttl: 10, lat: 39.96, lon: -83.0, city: "Columbus", rttMs: 47 },
    ],
  },
  {
    id: "slack|wss|443",
    app: "Slack",
    host: "wss-primary.slack.com",
    ip: "34.120.54.55",
    port: 443,
    color: "#c4a0b0",
    rttMs: 54,
    hops: [
      { ttl: 1, lat: 18.47, lon: -66.11, city: "San Juan", rttMs: 2 },
      { ttl: 4, lat: 25.76, lon: -80.19, city: "Miami", rttMs: 14 },
      { ttl: 9, lat: 33.75, lon: -84.39, city: "Atlanta", rttMs: 28 },
      { ttl: 14, lat: 37.42, lon: -122.08, city: "Mountain View", rttMs: 54 },
    ],
  },
];
