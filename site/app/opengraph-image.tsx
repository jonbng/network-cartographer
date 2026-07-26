import { ImageResponse } from "next/og";
import { SocialCard, socialImageAlt, socialImageSize } from "./social-card";

export const alt = socialImageAlt;
export const size = socialImageSize;
export const contentType = "image/png";

export default function OpenGraphImage() {
  return new ImageResponse(<SocialCard />, size);
}
