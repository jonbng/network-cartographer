import { NextResponse } from "next/server";
import {
  checkRateLimit,
  clientKeyFromRequest,
} from "@/lib/geo/rate-limit";
import {
  getRunCount,
  incrementRunCount,
  isRunCounterConfigured,
  RunCounterUnavailableError,
} from "@/lib/runs/upstash";

export const runtime = "nodejs";
export const dynamic = "force-dynamic";

const responseHeaders = {
  "cache-control": "private, max-age=0, no-store",
  "x-content-type-options": "nosniff",
};

export async function POST(request: Request) {
  const clientKey = `runs:${clientKeyFromRequest(request)}`;
  const rate = checkRateLimit(clientKey, 5, 60_000);
  if (!rate.ok) {
    return NextResponse.json(
      { error: "rate_limited", message: "Too many startup reports." },
      {
        status: 429,
        headers: {
          ...responseHeaders,
          "x-ratelimit-reset": String(Math.ceil(rate.resetAt / 1000)),
        },
      },
    );
  }

  try {
    const count = await incrementRunCount();
    return NextResponse.json({ count }, { headers: responseHeaders });
  } catch (error) {
    const unavailable = error instanceof RunCounterUnavailableError;
    console.error("Run counter increment failed", error);
    return NextResponse.json(
      {
        error: unavailable ? "not_configured" : "counter_unavailable",
        message: unavailable
          ? "The run counter is not configured."
          : "The run counter is temporarily unavailable.",
      },
      { status: unavailable ? 503 : 502, headers: responseHeaders },
    );
  }
}

export async function GET() {
  if (!isRunCounterConfigured()) {
    return NextResponse.json(
      { configured: false, count: null },
      { headers: responseHeaders },
    );
  }

  try {
    const count = await getRunCount();
    return NextResponse.json(
      { configured: true, count },
      { headers: responseHeaders },
    );
  } catch (error) {
    console.error("Run counter read failed", error);
    return NextResponse.json(
      { error: "counter_unavailable", message: "The run counter is unavailable." },
      { status: 502, headers: responseHeaders },
    );
  }
}
