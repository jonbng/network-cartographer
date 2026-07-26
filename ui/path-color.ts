const PALETTE = [
  "#e0a86a",
  "#8fb4a2",
  "#d5c07a",
  "#c98c76",
  "#9caaa2",
  "#b69ac5",
  "#d88273",
  "#82aeb1",
  "#c4a56d",
  "#8dae7f",
  "#87a1bf",
  "#bd8e9e",
  "#cf9364",
  "#789e91",
] as const;

export function colorForKey(key: string): string {
  let hash = 0;
  for (let i = 0; i < key.length; i++) {
    hash = (hash * 31 + key.charCodeAt(i)) >>> 0;
  }
  return PALETTE[hash % PALETTE.length];
}
