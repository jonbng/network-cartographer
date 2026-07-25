import { NextResponse } from "next/server";

export function GET() {
  return NextResponse.redirect(
    "https://github.com/jonbng/network-cartographer",
    307,
  );
}
