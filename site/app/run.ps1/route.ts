import { NextResponse } from "next/server";

const launcher =
  "https://github.com/jonbng/network-cartographer/releases/latest/download/run.ps1";

export function GET() {
  const response = NextResponse.redirect(launcher, 307);
  response.headers.set("cache-control", "public, max-age=300");
  return response;
}
