import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { ComposerOrb, orbFor } from "./composer-orb";

describe("orbFor", () => {
  it("gives the same orb for the same seed", () => {
    expect(orbFor("abc")).toEqual(orbFor("abc"));
  });

  it("gives different orbs for different seeds", () => {
    const seen = new Set(["a", "b", "c", "d", "e", "f"].map((s) => JSON.stringify(orbFor(s))));
    expect(seen.size).toBe(6);
  });

  it("uses only the colour tokens, never a raw colour", () => {
    const orb = orbFor("01JABC");
    const colours = [orb.base, ...orb.blobs.map((b) => b.color)].join(" ");
    expect(colours).not.toMatch(/#|rgb|hsl|oklch/);
    expect(colours).toMatch(/var\(--orb-\d+\)/);
  });

  it("never uses the same colour twice in one orb", () => {
    const orb = orbFor("xyz");
    const all = [...orb.base.matchAll(/--orb-\d+/g)].map((m) => m[0]);
    all.push(...orb.blobs.map((b) => b.color.slice(4, -1)));
    expect(new Set(all).size).toBe(all.length);
  });
});

describe("ComposerOrb", () => {
  it("is hidden from screen readers without a label", () => {
    render(<ComposerOrb seed="a" />);
    expect(screen.queryByRole("img")).toBeNull();
    expect(screen.getByTestId("composer-orb")).toHaveAttribute("aria-hidden", "true");
  });

  it("is an image with a label when given one", () => {
    render(<ComposerOrb seed="a" label="Started by Captain" />);
    expect(screen.getByRole("img", { name: "Started by Captain" })).toBeInTheDocument();
  });
});
