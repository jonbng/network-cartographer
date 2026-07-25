import type { GeoResult } from "./schema";

type CacheEntry = {
  result: GeoResult;
  expiresAt: number;
};

const HIT_TTL_MS = 24 * 60 * 60 * 1000;
const MISS_TTL_MS = 60 * 60 * 1000;
const MAX_ENTRIES = 20_000;

const memory = new Map<string, CacheEntry>();

function isHit(result: GeoResult): boolean {
  return result.city != null && result.latitude != null && result.longitude != null;
}

function prune(now: number): void {
  for (const [key, entry] of memory) {
    if (entry.expiresAt <= now) memory.delete(key);
  }
  if (memory.size <= MAX_ENTRIES) return;
  const overflow = memory.size - MAX_ENTRIES;
  let removed = 0;
  for (const key of memory.keys()) {
    memory.delete(key);
    removed += 1;
    if (removed >= overflow) break;
  }
}

export function getCached(ip: string): GeoResult | undefined {
  const now = Date.now();
  const entry = memory.get(ip);
  if (!entry) return undefined;
  if (entry.expiresAt <= now) {
    memory.delete(ip);
    return undefined;
  }
  return entry.result;
}

export function setCached(result: GeoResult): void {
  const now = Date.now();
  prune(now);
  memory.set(result.ip, {
    result,
    expiresAt: now + (isHit(result) ? HIT_TTL_MS : MISS_TTL_MS),
  });
}

export function cacheStats(): { size: number } {
  return { size: memory.size };
}
