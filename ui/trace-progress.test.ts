import { describe, expect, it } from "vitest";
import { isRenderableTrace } from "./trace-progress";

describe("isRenderableTrace", () => {
  it("renders progressive hops while a trace is running", () => {
    expect(isRenderableTrace({ status: "running", hops: [{ ttl: 1 }] })).toBe(true);
  });

  it("keeps a previous route visible while its retrace is queued", () => {
    expect(isRenderableTrace({ status: "queued", hops: [{ ttl: 1 }] })).toBe(true);
  });

  it("does not draw empty or failed traces", () => {
    expect(isRenderableTrace({ status: "running", hops: [] })).toBe(false);
    expect(isRenderableTrace({ status: "failed", hops: [{ ttl: 1 }] })).toBe(false);
  });
});
