import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { backend } from "@/test/backend";
import { ErrorBoundary } from "./error-boundary";

function Boom(): never {
  throw new Error("kaput");
}

describe("ErrorBoundary", () => {
  afterEach(() => vi.restoreAllMocks());

  it("shows the error with a way to copy diagnostics", async () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    backend.on("diagnostics_report", "REPORT");
    const user = userEvent.setup();
    const writeText = vi.fn(async () => {});
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    render(
      <ErrorBoundary>
        <Boom />
      </ErrorBoundary>,
    );
    expect(screen.getByRole("alert")).toHaveTextContent("kaput");
    await user.click(screen.getByRole("button", { name: "Copy diagnostics" }));
    expect(writeText).toHaveBeenCalledWith(expect.stringContaining("REPORT"));
    expect(await screen.findByRole("button", { name: "Copied" })).toBeInTheDocument();
  });

  it("renders children when nothing fails", () => {
    render(
      <ErrorBoundary>
        <p>fine</p>
      </ErrorBoundary>,
    );
    expect(screen.getByText("fine")).toBeInTheDocument();
  });
});
