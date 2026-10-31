import { screen } from "@testing-library/react";
import type { UserEvent } from "@testing-library/user-event";

/** Opens the run panel of a Composer's page. The approvals, repositories, integrations and the
 * Settings button are in the panel itself; for "Fleet" the fleet graph dialog is opened too. */
export async function openRunTab(user: UserEvent, tab: "Fleet" | "Approvals" | "Details") {
  const toggle = await screen.findByRole("button", { name: "Run details" });
  if (toggle.getAttribute("aria-pressed") !== "true") await user.click(toggle);
  if (tab !== "Fleet") return screen.findByRole("dialog", { name: "This run" });
  await user.click(await screen.findByRole("button", { name: /^Fleet graph/ }));
  return screen.findByRole("dialog", { name: /fleet$/ });
}
