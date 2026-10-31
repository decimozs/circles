import { screen } from "@testing-library/react";
import type { UserEvent } from "@testing-library/user-event";

/** Opens the run panel of a Composer's page and goes to what the right pane's tabs became: the
 * Fleet graph and Approvals open in a dialog, and the repositories, integrations and the Settings
 * button are in the panel itself, so for "Details" the panel is all there is to open. */
export async function openRunTab(user: UserEvent, tab: string) {
  const toggle = await screen.findByRole("button", { name: "Run details" });
  if (toggle.getAttribute("aria-pressed") !== "true") await user.click(toggle);
  if (tab === "Details") return screen.findByRole("dialog", { name: "This run" });
  await user.click(await screen.findByRole("button", { name: new RegExp(`^${tab}`) }));
  return screen.findByRole("tab", { name: new RegExp(tab) });
}
