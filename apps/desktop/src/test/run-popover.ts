import { screen } from "@testing-library/react";
import type { UserEvent } from "@testing-library/user-event";

/** Opens the run popover of a Composer's page and goes to one of its dialogs: the Fleet,
 * Approvals, Steps or Details that used to be tabs of the right pane. */
export async function openRunTab(user: UserEvent, tab: string) {
  const toggle = await screen.findByRole("button", { name: "Run details" });
  if (toggle.getAttribute("aria-pressed") !== "true") await user.click(toggle);
  await user.click(await screen.findByRole("button", { name: new RegExp(`^${tab}`) }));
  return screen.findByRole("tab", { name: new RegExp(tab) });
}
