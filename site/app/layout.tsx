import type { Metadata, Viewport } from "next";
import type { ReactNode } from "react";
import { IBM_Plex_Mono } from "next/font/google";
import "./globals.css";

const mono = IBM_Plex_Mono({
  subsets: ["latin"],
  weight: ["400", "500"],
  variable: "--font-mono",
  display: "swap",
});

export const metadata: Metadata = {
  metadataBase: new URL("https://mapmy.network"),
  title: "Network Cartographer",
  description:
    "Local CLI that maps per-app TCP connections and traceroute paths on a loopback globe UI. One command, no account.",
  openGraph: {
    title: "Network Cartographer",
    description:
      "Local CLI: per-app TCP connections, traceroutes, loopback globe. No account. One command.",
    type: "website",
    url: "/",
    siteName: "Network Cartographer",
  },
  twitter: {
    card: "summary",
    title: "Network Cartographer",
    description:
      "Local CLI: per-app TCP connections, traceroutes, loopback globe. No account. One command.",
  },
};

export const viewport: Viewport = {
  themeColor: "#0b0c0c",
  colorScheme: "dark",
};

export default function RootLayout({ children }: Readonly<{ children: ReactNode }>) {
  return (
    <html lang="en" className={mono.variable}>
      <body className={mono.className}>{children}</body>
    </html>
  );
}
