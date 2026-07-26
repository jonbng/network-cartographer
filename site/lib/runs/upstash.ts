const RUN_COUNT_KEY = "network-cartographer:runs:total";

type UpstashResponse = {
  result?: unknown;
  error?: string;
};

export class RunCounterUnavailableError extends Error {}
export class RunCounterError extends Error {}

function credentials(): { url: string; token: string } {
  const url = process.env.UPSTASH_REDIS_REST_URL?.replace(/\/$/, "");
  const token = process.env.UPSTASH_REDIS_REST_TOKEN;
  if (!url || !token) {
    throw new RunCounterUnavailableError("Upstash Redis is not configured.");
  }
  return { url, token };
}

async function command(parts: string[]): Promise<unknown> {
  const { url, token } = credentials();
  const response = await fetch(url, {
    method: "POST",
    headers: {
      authorization: `Bearer ${token}`,
      "content-type": "application/json",
    },
    body: JSON.stringify(parts),
    cache: "no-store",
    signal: AbortSignal.timeout(3_000),
  });

  let body: UpstashResponse;
  try {
    body = (await response.json()) as UpstashResponse;
  } catch {
    throw new RunCounterError("Upstash returned an invalid response.");
  }

  if (!response.ok || body.error) {
    throw new RunCounterError(body.error ?? `Upstash returned ${response.status}.`);
  }
  return body.result;
}

function parseCount(value: unknown): number {
  const count = typeof value === "number" ? value : Number(value ?? 0);
  if (!Number.isSafeInteger(count) || count < 0) {
    throw new RunCounterError("Upstash returned an invalid run count.");
  }
  return count;
}

export function isRunCounterConfigured(): boolean {
  return Boolean(
    process.env.UPSTASH_REDIS_REST_URL &&
      process.env.UPSTASH_REDIS_REST_TOKEN,
  );
}

export async function incrementRunCount(): Promise<number> {
  return parseCount(await command(["INCR", RUN_COUNT_KEY]));
}

export async function getRunCount(): Promise<number> {
  return parseCount(await command(["GET", RUN_COUNT_KEY]));
}
