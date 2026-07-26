export type RenderableTrace = {
  status: string;
  hops: readonly unknown[];
};

export function isRenderableTrace(trace: RenderableTrace): boolean {
  return (
    trace.hops.length > 0 &&
    (trace.status === "done" || trace.status === "running" || trace.status === "queued")
  );
}
