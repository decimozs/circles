import { describe, expect, it } from "vitest";
import { TOAST_MS, TOAST_WITH_ACTION_MS } from "./toast";

describe("how long a message stays", () => {
  it("is short, and a little longer when there is a button to press", () => {
    expect(TOAST_MS).toBe(3000);
    expect(TOAST_WITH_ACTION_MS).toBeGreaterThan(TOAST_MS);
    expect(TOAST_WITH_ACTION_MS).toBeLessThanOrEqual(10_000);
  });
});
