export function mergeVisibleSettings<T extends object>(base: T, visible: Partial<T>): T {
  return { ...base, ...visible };
}
