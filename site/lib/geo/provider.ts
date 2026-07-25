import { getCached, setCached } from "./cache";
import type { GeoResult } from "./schema";

type ProviderResponse = { results: GeoResult[] };

export class GeoProviderUnavailableError extends Error {}
export class GeoProviderError extends Error {}

function isGeoResult(value: unknown): value is GeoResult {
  if (!value || typeof value !== "object") return false;
  const row = value as Partial<GeoResult>;
  return typeof row.ip === "string" && typeof row.source === "string";
}

function isProviderResponse(value: unknown): value is ProviderResponse {
  return Boolean(
    value &&
      typeof value === "object" &&
      "results" in value &&
      Array.isArray((value as ProviderResponse).results) &&
      (value as ProviderResponse).results.every(isGeoResult),
  );
}

export function isGeoConfigured(): boolean {
  return Boolean(process.env.GEO_PROVIDER_URL && process.env.GEO_PROVIDER_TOKEN);
}

async function fetchFromProvider(ips: string[]): Promise<GeoResult[]> {
  const endpoint = process.env.GEO_PROVIDER_URL;
  const token = process.env.GEO_PROVIDER_TOKEN;
  if (!endpoint || !token) {
    throw new GeoProviderUnavailableError("No geolocation provider is configured.");
  }
  if (ips.length === 0) return [];

  const response = await fetch(endpoint, {
    method: "POST",
    headers: {
      authorization: `Bearer ${token}`,
      "content-type": "application/json",
      "user-agent": "mapmy.network-geo-proxy/1",
    },
    body: JSON.stringify({ ips }),
    signal: AbortSignal.timeout(8_000),
    cache: "no-store",
  });

  if (!response.ok) {
    throw new GeoProviderError(`The geolocation provider returned ${response.status}.`);
  }

  const body: unknown = await response.json();
  if (!isProviderResponse(body)) {
    throw new GeoProviderError("The geolocation provider returned an invalid response.");
  }
  return body.results;
}

/** Resolve IPs via in-process cache, then the private VPS provider for misses. */
export async function locateIps(ips: string[]): Promise<GeoResult[]> {
  if (!isGeoConfigured()) {
    throw new GeoProviderUnavailableError("No geolocation provider is configured.");
  }

  const cached: GeoResult[] = [];
  const missing: string[] = [];
  for (const ip of ips) {
    const hit = getCached(ip);
    if (hit) cached.push(hit);
    else missing.push(ip);
  }

  let fetched: GeoResult[] = [];
  if (missing.length > 0) {
    fetched = await fetchFromProvider(missing);
    for (const result of fetched) {
      const normalized = { ...result, ip: result.ip.trim().toLowerCase() };
      setCached(normalized);
    }
  }

  const byIp = new Map<string, GeoResult>();
  for (const result of [...cached, ...fetched]) {
    byIp.set(result.ip.trim().toLowerCase(), {
      ...result,
      ip: result.ip.trim().toLowerCase(),
    });
  }

  // Preserve request order; synthesize empty rows if upstream omitted an IP.
  return ips.map(
    (ip) =>
      byIp.get(ip) ?? {
        ip,
        city: null,
        country: null,
        latitude: null,
        longitude: null,
        asn: null,
        organization: null,
        source: "geolite",
        confidence: null,
      },
  );
}
