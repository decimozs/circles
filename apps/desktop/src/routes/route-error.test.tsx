import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { createMemoryRouter, RouterProvider } from "react-router";
import { describe, expect, it, vi } from "vitest";
import { RouteError } from "./route-error";

function Boom(): never {
  throw new Error("The workspace list broke.");
}

function show(path: string) {
  const router = createMemoryRouter(
    [
      { path: "/", element: <p>Start</p> },
      { path: "/boom", element: <Boom />, errorElement: <RouteError /> },
      { path: "*", element: <p>never</p> },
    ],
    { initialEntries: [path] },
  );
  render(<RouterProvider router={router} />);
  return router;
}

describe("a page that fails", () => {
  it("shows our own screen, not the router's, with the reason and a way out", async () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    const router = show("/boom");
    expect(await screen.findByRole("alert")).toHaveTextContent("Something went wrong");
    expect(screen.getByText("The workspace list broke.")).toBeInTheDocument();
    expect(screen.queryByText(/Hey developer/)).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Go to the start" }));
    expect(router.state.location.pathname).toBe("/");
    expect(await screen.findByText("Start")).toBeInTheDocument();
  });

  it("says a missing page is missing", async () => {
    const router = createMemoryRouter(
      [{ path: "/", element: <p>Start</p>, errorElement: <RouteError /> }],
      { initialEntries: ["/nowhere"] },
    );
    render(<RouterProvider router={router} />);
    expect(await screen.findByRole("heading", { name: "Page not found" })).toBeInTheDocument();
    expect(screen.getByText("There is nothing at this address.")).toBeInTheDocument();
  });
});
