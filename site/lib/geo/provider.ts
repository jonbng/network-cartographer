import type { GeoResult } from "./schema";

type ProviderResponse = { results: GeoResult[] };

export class GeoProviderUnavailableError extends Error {}
export class GeoProviderError extends Error {}

function isProviderResponse(value: unknown): value is ProviderResponse {
  return Boolean(
    value &&
      typeof value === "object" &&
      "results" in value &&
      Array.isArray((value as ProviderResponse).results),
  );
}

export async function locateIps(ips: string[]): Promise<GeoResult[]> {
  const endpoint = process.env.GEO_PROVIDER_URL;
  const token = process.env.GEO_PROVIDER_TOKEN;
  if (!endpoint || !token) {
    throw new GeoProviderUnavailableError("No geolocation provider is configured.");
  }

  const response = await fetch(endpoint, {
    method: "POST",
    headers: {
      authorization: `Bearer ${token}`,
      "content-type": "application/json",
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
