import { NextResponse } from "next/server";
import {
  GeoProviderError,
  GeoProviderUnavailableError,
  locateIps,
} from "@/lib/geo/provider";
import { parseGeoRequest } from "@/lib/geo/schema";

export const runtime = "nodejs";
export const dynamic = "force-dynamic";

const responseHeaders = {
  "cache-control": "private, max-age=0, no-store",
  "x-content-type-options": "nosniff",
};

export async function POST(request: Request) {
  let body: unknown;
  try {
    body = await request.json();
  } catch {
    return NextResponse.json(
      { error: "invalid_json", message: "The request body must be valid JSON." },
      { status: 400, headers: responseHeaders },
    );
  }

  const parsed = parseGeoRequest(body);
  if (!parsed.ok) {
    return NextResponse.json(
      { error: "invalid_request", message: parsed.message },
      { status: parsed.status, headers: responseHeaders },
    );
  }

  try {
    const results = await locateIps(parsed.ips);
    return NextResponse.json(
      { results },
      { headers: { ...responseHeaders, "cache-control": "private, max-age=3600" } },
    );
  } catch (error) {
    if (error instanceof GeoProviderUnavailableError) {
      return NextResponse.json(
        {
          error: "service_unavailable",
          message: "Hosted geolocation is not configured yet.",
        },
        { status: 503, headers: responseHeaders },
      );
    }

    const message =
      error instanceof GeoProviderError
        ? error.message
        : "The geolocation provider could not be reached.";
    return NextResponse.json(
      { error: "provider_error", message },
      { status: 502, headers: responseHeaders },
    );
  }
}

export function GET() {
  return NextResponse.json(
    {
      service: "Map My Network Geo API",
      status: process.env.GEO_PROVIDER_URL ? "configured" : "unconfigured",
      privacy: "Only public IP address batches are accepted by this endpoint.",
    },
    { headers: { "cache-control": "no-store" } },
  );
}
