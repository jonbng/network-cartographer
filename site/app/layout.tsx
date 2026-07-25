import type { Metadata, Viewport } from "next";
import type { ReactNode } from "react";
import "./globals.css";

export const metadata: Metadata = {
  metadataBase: new URL("https://mapmy.network"),
  title: "Map My Network — See where your apps connect",
  description:
    "See which apps are online, where they connect, and how traffic crosses the globe — all from one local command.",
  openGraph: {
    title: "Map My Network",
    description: "Your network, made visible. No account, no installer, one command.",
    type: "website",
    url: "/",
    siteName: "Map My Network",
  },
  twitter: {
    card: "summary_large_image",
    title: "Map My Network",
    description: "Your network, made visible. No account, no installer, one command.",
  },
};

export const viewport: Viewport = {
  themeColor: "#060a0a",
  colorScheme: "dark",
};

export default function RootLayout({ children }: Readonly<{ children: ReactNode }>) {
  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  );
}
