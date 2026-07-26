import type { Metadata, Viewport } from "next";
import type { ReactNode } from "react";
import { IBM_Plex_Mono } from "next/font/google";
import { Analytics } from "@vercel/analytics/next";
import "./globals.css";

const siteUrl = "https://mapmy.network";
const title = "Network Cartographer";
const description =
  "See where every app connects with a live, local 3D map of network requests and traceroute paths. One command, no account.";

const mono = IBM_Plex_Mono({
  subsets: ["latin"],
  weight: ["400", "500"],
  variable: "--font-mono",
  display: "swap",
});

export const metadata: Metadata = {
  metadataBase: new URL(siteUrl),
  title,
  description,
  applicationName: title,
  authors: [{ name: "Jonathan Bangert", url: "https://jonathanb.dk" }],
  creator: "Jonathan Bangert",
  publisher: "Jonathan Bangert",
  category: "technology",
  keywords: [
    "network monitor",
    "network visualization",
    "traceroute",
    "network map",
    "developer tool",
    "open source",
  ],
  alternates: {
    canonical: "/",
  },
  openGraph: {
    title,
    description,
    type: "website",
    url: "/",
    siteName: title,
    locale: "en_US",
  },
  twitter: {
    card: "summary_large_image",
    title,
    description,
  },
};

export const viewport: Viewport = {
  themeColor: "#0b0c0c",
  colorScheme: "dark",
};

export default function RootLayout({ children }: Readonly<{ children: ReactNode }>) {
  const structuredData = {
    "@context": "https://schema.org",
    "@graph": [
      {
        "@type": "WebSite",
        "@id": `${siteUrl}/#website`,
        url: siteUrl,
        name: title,
        description,
        inLanguage: "en",
      },
      {
        "@type": "SoftwareApplication",
        "@id": `${siteUrl}/#software`,
        name: title,
        description,
        url: siteUrl,
        image: `${siteUrl}/opengraph-image`,
        applicationCategory: "DeveloperApplication",
        operatingSystem: "macOS, Linux, Windows",
        isAccessibleForFree: true,
        license: "https://opensource.org/license/mit",
        codeRepository: "https://github.com/jonbng/network-cartographer",
        author: {
          "@type": "Person",
          name: "Jonathan Bangert",
          url: "https://jonathanb.dk",
        },
        offers: {
          "@type": "Offer",
          price: "0",
          priceCurrency: "USD",
        },
        featureList: [
          "Live per-application connection monitoring",
          "Interactive 3D network map",
          "Traceroute visualization",
          "Local-first browser interface",
        ],
      },
    ],
  };

  return (
    <html lang="en" className={mono.variable}>
      <head>
        <script
          type="application/ld+json"
          dangerouslySetInnerHTML={{
            __html: JSON.stringify(structuredData).replace(/</g, "\\u003c"),
          }}
        />
      </head>
      <body className={mono.className}>
        {children}
        <Analytics />
      </body>
    </html>
  );
}
