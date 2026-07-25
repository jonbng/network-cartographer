import { NextResponse } from "next/server";
import { cacheStats } from "@/lib/geo/cache";
import {
  GeoProviderError,
  GeoProviderUnavailableError,
  isGeoConfigured,
  locateIps,
} from "@/lib/geo/provider";
import { checkRateLimit, clientKeyFromRequest } from "@/lib/geo/rate-limit";
import { parseGeoRequest } from "@/lib/geo/schema";

export const runtime = "nodejs";
export const dynamic = "force-dynamic";

const responseHeaders = {
  "cache-control": "private, max-age=0, no-store",
  "x-content-type-options": "nosniff",
};

export async function POST(request: Request) {
  const rate = checkRateLimit(clientKeyFromRequest(request));
  const rateHeaders = {
    ...responseHeaders,
    "x-ratelimit-remaining": String(rate.remaining),
    "x-ratelimit-reset": String(Math.ceil(rate.resetAt / 1000)),
  };

  if (!rate.ok) {
    return NextResponse.json(
      {
        error: "rate_limited",
        message: "Too many geolocation requests. Try again shortly.",
      },
      { status: 429, headers: rateHeaders },
    );
  }

  let body: unknown;
  try {
    body = await request.json();
  } catch {
    return NextResponse.json(
      { error: "invalid_json", message: "The request body must be valid JSON." },
      { status: 400, headers: rateHeaders },
    );
  }

  const parsed = parseGeoRequest(body);
  if (!parsed.ok) {
    return NextResponse.json(
      { error: "invalid_request", message: parsed.message },
      { status: parsed.status, headers: rateHeaders },
    );
  }

  try {
    const results = await locateIps(parsed.ips);
    return NextResponse.json(
      { results },
      {
        headers: {
          ...rateHeaders,
          "cache-control": "private, max-age=3600",
        },
      },
    );
  } catch (error) {
    if (error instanceof GeoProviderUnavailableError) {
      return NextResponse.json(
        {
          error: "service_unavailable",
          message: "Hosted geolocation is not configured yet.",
        },
        { status: 503, headers: rateHeaders },
      );
    }

    const message =
      error instanceof GeoProviderError
        ? error.message
        : "The geolocation provider could not be reached.";
    return NextResponse.json(
      { error: "provider_error", message },
      { status: 502, headers: rateHeaders },
    );
  }
}

export function GET() {
  return NextResponse.json(
    {
      service: "Map My Network Geo API",
      status: isGeoConfigured() ? "configured" : "unconfigured",
      privacy:
        "Only public IP address batches are accepted. IPs are used for geolocation lookup and short-lived caching; connection and process metadata are never accepted.",
      retention:
        "Lookup results may be cached in memory for up to 24 hours. Request bodies are not persisted.",
      cache: cacheStats(),
    },
    { headers: { "cache-control": "no-store" } },
  );
}
