import { NextResponse } from "next/server";

export function GET() {
  const response = NextResponse.redirect(
    "https://github.com/jonbng/network-cartographer",
    307,
  );
  response.headers.set("cache-control", "public, max-age=3600");
  return response;
}
