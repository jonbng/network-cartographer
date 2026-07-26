import type { MetadataRoute } from "next";

export default function manifest(): MetadataRoute.Manifest {
  return {
    name: "Network Cartographer",
    short_name: "NetCart",
    description:
      "See where every app connects with a live, local 3D map of network requests and traceroute paths.",
    start_url: "/",
    display: "standalone",
    background_color: "#0b0c0c",
    theme_color: "#0b0c0c",
    icons: [
      {
        src: "/icon.svg",
        sizes: "any",
        type: "image/svg+xml",
      },
    ],
    categories: ["developer tools", "utilities"],
  };
}
