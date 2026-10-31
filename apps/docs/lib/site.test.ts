import { describe, expect, it } from "vitest";
import { siteUrl } from "./site";

describe("siteUrl", () => {
  it("prefers the address that was set, with or without https://", () => {
    expect(siteUrl({ NEXT_PUBLIC_SITE_URL: "https://docs.example.com" }).href).toBe(
      "https://docs.example.com/",
    );
    expect(siteUrl({ NEXT_PUBLIC_SITE_URL: "docs.example.com" }).href).toBe(
      "https://docs.example.com/",
    );
  });

  it("falls back to the production address Vercel gives, then to localhost", () => {
    expect(siteUrl({ VERCEL_PROJECT_PRODUCTION_URL: "circles-docs.vercel.app" }).href).toBe(
      "https://circles-docs.vercel.app/",
    );
    expect(siteUrl({}).href).toBe("http://localhost:3000/");
    expect(siteUrl({ NEXT_PUBLIC_SITE_URL: "  " }).href).toBe("http://localhost:3000/");
  });
});
