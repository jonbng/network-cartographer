import { NextResponse } from "next/server";

const launcher =
  "https://github.com/jonbng/network-cartographer/releases/latest/download/run.ps1";

export function GET() {
  return NextResponse.redirect(launcher, 307);
}
