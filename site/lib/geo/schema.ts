import { isIP } from "node:net";

export const MAX_BATCH_SIZE = 40;

export type GeoResult = {
  ip: string;
  city: string | null;
  country: string | null;
  latitude: number | null;
  longitude: number | null;
  asn: number | null;
  organization: string | null;
  source: string;
  confidence: "low" | "medium" | "high" | null;
};

type ParsedGeoRequest =
  | { ok: true; ips: string[] }
  | { ok: false; message: string; status: 400 | 413 | 422 };

const blockedIpv4Ranges: ReadonlyArray<readonly [number, number]> = [
  [ipv4ToInteger("0.0.0.0"), 8],
  [ipv4ToInteger("10.0.0.0"), 8],
  [ipv4ToInteger("100.64.0.0"), 10],
  [ipv4ToInteger("127.0.0.0"), 8],
  [ipv4ToInteger("169.254.0.0"), 16],
  [ipv4ToInteger("172.16.0.0"), 12],
  [ipv4ToInteger("192.0.0.0"), 24],
  [ipv4ToInteger("192.0.2.0"), 24],
  [ipv4ToInteger("192.168.0.0"), 16],
  [ipv4ToInteger("198.18.0.0"), 15],
  [ipv4ToInteger("198.51.100.0"), 24],
  [ipv4ToInteger("203.0.113.0"), 24],
  [ipv4ToInteger("224.0.0.0"), 4],
  [ipv4ToInteger("240.0.0.0"), 4],
];

function ipv4ToInteger(ip: string): number {
  return ip
    .split(".")
    .reduce((value, octet) => (value << 8) + Number(octet), 0) >>> 0;
}

function isInIpv4Range(ip: number, network: number, prefix: number): boolean {
  const mask = prefix === 0 ? 0 : (0xffffffff << (32 - prefix)) >>> 0;
  return (ip & mask) >>> 0 === (network & mask) >>> 0;
}

function isPublicIpv4(ip: string): boolean {
  const value = ipv4ToInteger(ip);
  return !blockedIpv4Ranges.some(([network, prefix]) =>
    isInIpv4Range(value, network, prefix),
  );
}

function expandIpv6(ip: string): number[] | null {
  const withoutZone = ip.split("%", 1)[0].toLowerCase();
  const [head = "", tail = "", ...extra] = withoutZone.split("::");
  if (extra.length > 0) return null;

  const parseSide = (side: string): number[] => {
    if (!side) return [];
    const groups = side.split(":");
    const last = groups.at(-1);
    if (last?.includes(".")) {
      const mapped = ipv4ToInteger(last);
      groups.splice(
        -1,
        1,
        ((mapped >>> 16) & 0xffff).toString(16),
        (mapped & 0xffff).toString(16),
      );
    }
    return groups.map((group) => Number.parseInt(group, 16));
  };

  const left = parseSide(head);
  const right = parseSide(tail);
  const missing = 8 - left.length - right.length;
  if (missing < 0 || (!withoutZone.includes("::") && missing !== 0)) return null;
  return [...left, ...Array.from({ length: missing }, () => 0), ...right];
}

function hasIpv6Prefix(groups: number[], prefix: number[], bits: number): boolean {
  const fullGroups = Math.floor(bits / 16);
  const remainingBits = bits % 16;
  for (let index = 0; index < fullGroups; index += 1) {
    if (groups[index] !== prefix[index]) return false;
  }
  if (remainingBits === 0) return true;
  const mask = (0xffff << (16 - remainingBits)) & 0xffff;
  return (groups[fullGroups] & mask) === (prefix[fullGroups] & mask);
}

function isPublicIpv6(ip: string): boolean {
  const groups = expandIpv6(ip);
  if (!groups) return false;

  const isIpv4Mapped = hasIpv6Prefix(groups, [0, 0, 0, 0, 0, 0xffff], 96);
  if (isIpv4Mapped) {
    const mapped = `${groups[6] >>> 8}.${groups[6] & 255}.${groups[7] >>> 8}.${groups[7] & 255}`;
    return isPublicIpv4(mapped);
  }

  const blocked: ReadonlyArray<readonly [number[], number]> = [
    [[0, 0, 0, 0, 0, 0, 0, 0], 128],
    [[0, 0, 0, 0, 0, 0, 0, 1], 128],
    [[0x100, 0, 0, 0], 64],
    [[0x2001, 0x0db8], 32],
    [[0xfc00], 7],
    [[0xfe80], 10],
    [[0xff00], 8],
  ];
  return !blocked.some(([prefix, bits]) => hasIpv6Prefix(groups, prefix, bits));
}

export function isPublicIp(ip: string): boolean {
  const version = isIP(ip);
  if (version === 4) return isPublicIpv4(ip);
  if (version === 6) return isPublicIpv6(ip);
  return false;
}

export function parseGeoRequest(value: unknown): ParsedGeoRequest {
  if (!value || typeof value !== "object" || !("ips" in value)) {
    return { ok: false, status: 400, message: "Expected an object with an ips array." };
  }

  const { ips } = value as { ips?: unknown };
  if (!Array.isArray(ips) || ips.some((ip) => typeof ip !== "string")) {
    return { ok: false, status: 400, message: "ips must be an array of IP address strings." };
  }
  if (ips.length === 0) {
    return { ok: false, status: 400, message: "At least one IP address is required." };
  }
  if (ips.length > MAX_BATCH_SIZE) {
    return {
      ok: false,
      status: 413,
      message: `A maximum of ${MAX_BATCH_SIZE} addresses is allowed per request.`,
    };
  }

  const normalized = [...new Set(ips.map((ip) => ip.trim().toLowerCase()))];
  const invalid = normalized.filter((ip) => !isPublicIp(ip));
  if (invalid.length > 0) {
    return {
      ok: false,
      status: 422,
      message: "Only public, globally routable IP addresses are accepted.",
    };
  }

  return { ok: true, ips: normalized };
}
