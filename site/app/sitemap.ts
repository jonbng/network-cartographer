import type { MetadataRoute } from "next";

export default function sitemap(): MetadataRoute.Sitemap {
  return [
    {
      url: "https://mapmy.network",
      changeFrequency: "monthly",
      priority: 1,
    },
  ];
}
