import { createElement, type IconNode } from "lucide";

const ROOT_ATTRS = {
  viewBox: "0 0 24 24",
  "aria-hidden": "true",
} as const;

/** Lucide icon node → inline SVG string (only icons you import are bundled). */
export function lucideIcon(icon: IconNode): string {
  return createElement(icon, ROOT_ATTRS).outerHTML;
}
