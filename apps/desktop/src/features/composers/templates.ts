import type { ComposerInput } from "@circles/bindings/ComposerInput";

/** A starting point for a Composer. Autonomy is left to the safe defaults. */
export type Template = {
  id: string;
  name: string;
  description: string;
  input: Partial<ComposerInput>;
};

export const TEMPLATES: Template[] = [
  {
    id: "triage",
    name: "Triage and split into tasks",
    description: "Reads a goal, splits it into independent tasks, and starts one agent for each.",
    input: {
      name: "Task splitter",
      instructions:
        "When I give you a goal for a project, look at the project first. Split the goal into small tasks that do not touch the same files. Create one workspace for each task and start an agent in it with a complete brief. Wait for them, read what they changed, and report what each one did and what is left. Do not open pull requests unless I ask.",
      limits: {
        max_steps: 100,
        max_children_at_once: 3,
        max_children_total: 8,
        max_minutes: 120,
        max_budget_usd: null,
      },
    },
  },
  {
    id: "refactor",
    name: "Parallel refactor",
    description: "Applies one change across a project by giving each area to its own agent.",
    input: {
      name: "Refactorer",
      instructions:
        "When I describe a refactor, find the separate areas it touches and give each area to its own agent in its own workspace. Tell every agent exactly what to change and what not to touch. When they finish, check each workspace's changes, and report anything that looks wrong before I look. Do not merge anything.",
      limits: {
        max_steps: 150,
        max_children_at_once: 4,
        max_children_total: 12,
        max_minutes: 180,
        max_budget_usd: null,
      },
    },
  },
  {
    id: "review",
    name: "Review all open pull requests",
    description: "Checks out each open pull request and has an agent review it, then summarises.",
    input: {
      name: "PR reviewer",
      instructions:
        "List the open pull requests of the project I name. For each one, create a workspace on its branch and ask an agent to review it: correctness, tests, and anything risky. Collect the findings and give me one short summary per pull request. Never merge, comment or push.",
      autonomy: { merge_pr: "off", create_pr: "off" },
      limits: {
        max_steps: 150,
        max_children_at_once: 3,
        max_children_total: 15,
        max_minutes: 120,
        max_budget_usd: null,
      },
    },
  },
  {
    id: "fix-checks",
    name: "Fix failing checks across branches",
    description: "Finds branches whose checks fail and has an agent fix each one.",
    input: {
      name: "Check fixer",
      instructions:
        "Look at the workspaces of the project I name. For each one whose checks are failing (use get_pr_status), tell its agent to read the failing logs and fix them. Wait for them and check the result. Report which branches are green now and which still fail. Do not merge.",
      limits: {
        max_steps: 150,
        max_children_at_once: 3,
        max_children_total: 10,
        max_minutes: 120,
        max_budget_usd: null,
      },
    },
  },
];
