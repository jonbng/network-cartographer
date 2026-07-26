type Bucket = {
  count: number;
  resetAt: number;
};

const WINDOW_MS = 60_000;
const MAX_REQUESTS_PER_WINDOW = 60;

const buckets = new Map<string, Bucket>();

function prune(now: number): void {
  for (const [key, bucket] of buckets) {
    if (bucket.resetAt <= now) buckets.delete(key);
  }
  if (buckets.size > 10_000) {
    buckets.clear();
  }
}

export type RateLimitResult =
  | { ok: true; remaining: number; resetAt: number }
  | { ok: false; remaining: 0; resetAt: number };

export function checkRateLimit(
  clientKey: string,
  maxRequests = MAX_REQUESTS_PER_WINDOW,
  windowMs = WINDOW_MS,
): RateLimitResult {
  const now = Date.now();
  prune(now);
  const existing = buckets.get(clientKey);
  if (!existing || existing.resetAt <= now) {
    const resetAt = now + windowMs;
    buckets.set(clientKey, { count: 1, resetAt });
    return { ok: true, remaining: maxRequests - 1, resetAt };
  }
  if (existing.count >= maxRequests) {
    return { ok: false, remaining: 0, resetAt: existing.resetAt };
  }
  existing.count += 1;
  return {
    ok: true,
    remaining: maxRequests - existing.count,
    resetAt: existing.resetAt,
  };
}

export function clientKeyFromRequest(request: Request): string {
  return clientIpFromRequest(request) ?? "unknown";
}

/**
 * Resolve the public address observed by the hosting edge. Vercel replaces its
 * forwarding header; the fallbacks keep local and alternate deployments useful.
 */
export function clientIpFromRequest(request: Request): string | null {
  for (const header of [
    "x-vercel-forwarded-for",
    "x-real-ip",
    "x-forwarded-for",
  ]) {
    const value = request.headers.get(header);
    if (!value) continue;
    for (const candidate of value.split(",")) {
      const ip = candidate.trim().toLowerCase();
      if (ip) return ip;
    }
  }
  return null;
}
