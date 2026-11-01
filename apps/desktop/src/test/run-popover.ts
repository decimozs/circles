import { screen } from "@testing-library/react";
import type { UserEvent } from "@testing-library/user-event";
import { session, workspace } from "./fixtures";

/** Opens the run panel of a Composer's page. The approvals, repositories, integrations and the
 * Settings button are in the panel itself; for "Fleet" the fleet graph dialog is opened too. */
export async function openRunTab(user: UserEvent, tab: "Fleet" | "Approvals" | "Details") {
  const toggle = await screen.findByRole("button", { name: "Run details" });
  if (toggle.getAttribute("aria-pressed") !== "true") await user.click(toggle);
  if (tab !== "Fleet") return screen.findByRole("dialog", { name: "This run" });
  await user.click(await screen.findByRole("button", { name: /^Fleet graph/ }));
  return screen.findByRole("dialog", { name: /fleet$/ });
}

/** A workspace of a run with an agent that waits for a permission. A run that has one offers the fleet
 * graph; with nothing going, the panel hides it. */
export const runningFleetItem = () => ({
  child: {
    id: "f9",
    composer_id: "c1",
    session_id: "r1",
    workspace_id: "w9",
    chat_id: "k9",
    created_at: 1,
  },
  workspace: workspace({
    id: "w9",
    city: "Rome",
    title: "Keep busy",
    branch: "me/busy",
    state: "working",
  }),
  chat: session({ id: "k9", workspace_id: "w9", title: "Waiting job", status: "blocked" }),
});
