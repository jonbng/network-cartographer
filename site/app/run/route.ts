import { NextResponse } from "next/server";

const launcher =
  "https://github.com/jonbng/network-cartographer/releases/latest/download/run.sh";

export function GET() {
  return NextResponse.redirect(launcher, 307);
}
