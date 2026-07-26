import { NextResponse } from "next/server";
import {
  GeoProviderError,
  GeoProviderUnavailableError,
  locateIps,
} from "@/lib/geo/provider";
import {
  checkRateLimit,
  clientIpFromRequest,
  clientKeyFromRequest,
} from "@/lib/geo/rate-limit";
import { isPublicIp } from "@/lib/geo/schema";

export const runtime = "nodejs";
export const dynamic = "force-dynamic";

const responseHeaders = {
  "cache-control": "private, max-age=0, no-store",
  "x-content-type-options": "nosniff",
};

export async function GET(request: Request) {
  const rate = checkRateLimit(clientKeyFromRequest(request));
  const headers = {
    ...responseHeaders,
    "x-ratelimit-remaining": String(rate.remaining),
    "x-ratelimit-reset": String(Math.ceil(rate.resetAt / 1000)),
  };

  if (!rate.ok) {
    return NextResponse.json(
      { error: "rate_limited", message: "Too many egress lookups. Try again shortly." },
      { status: 429, headers },
    );
  }

  const ip = clientIpFromRequest(request);
  if (!ip || !isPublicIp(ip)) {
    return NextResponse.json(
      {
        error: "egress_unavailable",
        message: "A public network exit could not be determined for this request.",
      },
      { status: 422, headers },
    );
  }

  try {
    const [egress] = await locateIps([ip]);
    return NextResponse.json({ egress }, { headers });
  } catch (error) {
    if (error instanceof GeoProviderUnavailableError) {
      return NextResponse.json(
        { error: "service_unavailable", message: "Hosted geolocation is not configured yet." },
        { status: 503, headers },
      );
    }
    const message =
      error instanceof GeoProviderError
        ? error.message
        : "The geolocation provider could not be reached.";
    return NextResponse.json(
      { error: "provider_error", message },
      { status: 502, headers },
    );
  }
}
