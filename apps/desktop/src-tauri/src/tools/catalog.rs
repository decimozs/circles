//! Every tool a Composer can have, with its inputs and tier. The names are what the model calls.

use serde_json::{json, Value};

use super::{Tier, ToolDef};

fn schema(properties: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": properties, "required": required })
}

fn text(description: &str) -> Value {
    json!({ "type": "string", "minLength": 1, "maxLength": 20000, "description": description })
}

fn id(description: &str) -> Value {
    json!({ "type": "string", "minLength": 1, "maxLength": 200, "description": description })
}

fn flag(description: &str) -> Value {
    json!({ "type": "boolean", "description": description })
}

/// All tools, in the order they are listed to the model.
pub fn all() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "list_projects",
            description: "List the projects in Circles: id, name, folder, base branch and number of live workspaces.",
            tier: Tier::Read,
            schema: schema(json!({}), &[]),
            repeat_safe: true,
            untrusted: false,
        },
        ToolDef {
            name: "list_pull_requests",
            description: "List the open pull requests of a project's GitHub repository: number, title, branch, author and when it was last updated.",
            tier: Tier::Read,
            schema: schema(json!({ "project_id": id("The project.") }), &["project_id"]),
            repeat_safe: true,
            untrusted: true,
        },
        ToolDef {
            name: "list_workspaces",
            description: "List workspaces (each a separate copy of a project on its own branch), newest first. Optionally only one project's.",
            tier: Tier::Read,
            schema: schema(
                json!({
                    "project_id": id("Only workspaces of this project."),
                    "include_archived": flag("Also list archived workspaces."),
                }),
                &[],
            ),
            repeat_safe: true,
            untrusted: false,
        },
        ToolDef {
            name: "get_workspace",
            description: "Details of one workspace: branch, base branch, state, pull request, and its chats with their status.",
            tier: Tier::Read,
            schema: schema(json!({ "workspace_id": id("The workspace.") }), &["workspace_id"]),
            repeat_safe: true,
            untrusted: false,
        },
        ToolDef {
            name: "get_chat_tail",
            description: "The most recent messages of a chat, oldest first, and whether the chat is working, idle or waiting.",
            tier: Tier::Read,
            schema: schema(
                json!({
                    "chat_id": id("The chat."),
                    "count": { "type": "integer", "minimum": 1, "maximum": 50, "description": "How many messages. Default 10." },
                }),
                &["chat_id"],
            ),
            repeat_safe: true,
            untrusted: true,
        },
        ToolDef {
            name: "get_changes",
            description: "The files changed in a workspace, with lines added and removed. `uncommitted` is only what is not committed yet. `all` is everything compared with the base branch.",
            tier: Tier::Read,
            schema: schema(
                json!({
                    "workspace_id": id("The workspace."),
                    "scope": { "type": "string", "enum": ["all", "uncommitted"], "description": "Default `all`." },
                }),
                &["workspace_id"],
            ),
            repeat_safe: true,
            untrusted: true,
        },
        ToolDef {
            name: "get_pr_status",
            description: "Where a workspace's branch stands: commits ahead and behind, pull request, checks, and what blocks merging.",
            tier: Tier::Read,
            schema: schema(json!({ "workspace_id": id("The workspace.") }), &["workspace_id"]),
            repeat_safe: true,
            untrusted: true,
        },
        ToolDef {
            name: "wait_for_chats",
            description: "Wait until the given chats have stopped working (idle, waiting for a permission, or stopped), or the time runs out. Use this instead of asking again and again.",
            tier: Tier::Read,
            schema: schema(
                json!({
                    "chat_ids": { "type": "array", "maxItems": 10, "description": "Chats to wait for." },
                    "timeout_seconds": { "type": "integer", "minimum": 1, "maximum": 600, "description": "Default 120." },
                }),
                &["chat_ids"],
            ),
            repeat_safe: true,
            untrusted: false,
        },
        ToolDef {
            name: "create_workspace",
            description: "Create a workspace in a project: a separate copy of the repository. By default on a new branch. Give `pull_request` to check out a pull request's branch, or `branch` to check out an existing branch. Returns the workspace id.",
            tier: Tier::Write,
            schema: schema(
                json!({
                    "project_id": id("The project."),
                    "base_branch": id("For a new branch: the branch to start from. Default: the project's base branch."),
                    "pull_request": { "type": "integer", "minimum": 1, "description": "Check out this pull request's branch." },
                    "branch": id("Check out this existing branch."),
                    "branch_name": id("Name for the new branch, in the conventional form `type/short-description` such as feat/add-dark-mode-toggle. Give it whenever you know the task. Without one the branch gets a random name."),
                }),
                &["project_id"],
            ),
            repeat_safe: false,
            untrusted: false,
        },
        ToolDef {
            name: "start_chat",
            description: "Start a coding agent in a workspace by giving it a task. Returns the chat id. The agent works on its own, so write the task as a complete brief. A workspace whose pull request was merged is refused: create a new workspace for follow-up work, or set `reuse_merged` when the work is about files that exist only in that workspace.",
            tier: Tier::Write,
            schema: schema(
                json!({
                    "workspace_id": id("The workspace."),
                    "task": text("What the agent should do."),
                    "model": id("Model alias, for example sonnet or opus. Default: the usual one."),
                    "reuse_merged": flag("Start in a workspace whose pull request was merged. Only when the work needs files that exist only there."),
                }),
                &["workspace_id", "task"],
            ),
            repeat_safe: false,
            untrusted: false,
        },
        ToolDef {
            name: "rename_branch",
            description: "Rename a workspace's branch to a conventional name, `type/short-description` with a type of feat, fix, chore, docs, refactor, test, perf, build, ci, style or revert, for example feat/add-dark-mode-toggle. Only possible before the branch is pushed or has a pull request.",
            tier: Tier::Write,
            schema: schema(
                json!({
                    "workspace_id": id("The workspace."),
                    "name": id("The new branch name."),
                }),
                &["workspace_id", "name"],
            ),
            repeat_safe: false,
            untrusted: false,
        },
        ToolDef {
            name: "send_message",
            description: "Send a message to an existing chat. While the agent is working, this steers it.",
            tier: Tier::Write,
            schema: schema(
                json!({ "chat_id": id("The chat."), "text": text("The message.") }),
                &["chat_id", "text"],
            ),
            repeat_safe: false,
            untrusted: false,
        },
        ToolDef {
            name: "interrupt_chat",
            description: "Stop what a chat's agent is doing right now. The chat stays usable.",
            tier: Tier::Write,
            schema: schema(json!({ "chat_id": id("The chat.") }), &["chat_id"]),
            repeat_safe: true,
            untrusted: false,
        },
        ToolDef {
            name: "add_local_project",
            description: "Add a repository that is already on this Mac as a project. It needs at least one commit.",
            tier: Tier::Write,
            schema: schema(json!({ "path": text("Absolute path of the repository or a folder inside it.") }), &["path"]),
            repeat_safe: false,
            untrusted: false,
        },
        ToolDef {
            name: "clone_github_project",
            description: "Clone a GitHub repository and add it as a project.",
            tier: Tier::Write,
            schema: schema(json!({ "repository": id("owner/name or a GitHub URL.") }), &["repository"]),
            repeat_safe: false,
            untrusted: false,
        },
        ToolDef {
            name: "quick_start_project",
            description: "Create a brand new repository with a first commit and add it as a project.",
            tier: Tier::Write,
            schema: schema(json!({ "name": id("Name of the new project.") }), &["name"]),
            repeat_safe: false,
            untrusted: false,
        },
        ToolDef {
            name: "create_pr",
            description: "Push the workspace's branch and open a pull request on GitHub. The branch must have commits the base branch does not have.",
            tier: Tier::External,
            schema: schema(
                json!({
                    "workspace_id": id("The workspace."),
                    "title": id("Pull request title."),
                    "body": json!({ "type": "string", "maxLength": 20000, "description": "Description, in Markdown." }),
                    "draft": flag("Open it as a draft."),
                }),
                &["workspace_id", "title"],
            ),
            repeat_safe: false,
            untrusted: false,
        },
        ToolDef {
            name: "merge_pr",
            description: "Merge the workspace's pull request with the project's merge method. Failing checks and conflicts block it.",
            tier: Tier::External,
            schema: schema(
                json!({
                    "workspace_id": id("The workspace."),
                    "override_blockers": flag("Merge although something blocks it. Only works when the person allowed it for this Composer."),
                }),
                &["workspace_id"],
            ),
            repeat_safe: false,
            untrusted: false,
        },
        ToolDef {
            name: "archive_workspace",
            description: "Archive a workspace: stop its agents and remove its folder. The branch is kept. Refuses when there are uncommitted changes unless told to commit them.",
            tier: Tier::Destructive,
            schema: schema(
                json!({
                    "workspace_id": id("The workspace."),
                    "commit_dirty": flag("Commit uncommitted changes first."),
                }),
                &["workspace_id"],
            ),
            repeat_safe: false,
            untrusted: false,
        },
        ToolDef {
            name: "delete_workspace",
            description: "Delete an archived or errored workspace record. The branch is kept unless told to delete it.",
            tier: Tier::Destructive,
            schema: schema(
                json!({
                    "workspace_id": id("The workspace."),
                    "delete_branch": flag("Also delete the branch."),
                }),
                &["workspace_id"],
            ),
            repeat_safe: false,
            untrusted: false,
        },
        ToolDef {
            name: "remove_project",
            description: "Remove a project from Circles. Its repository on disk is not touched. It must have no live workspaces.",
            tier: Tier::Destructive,
            schema: schema(json!({ "project_id": id("The project.") }), &["project_id"]),
            repeat_safe: false,
            untrusted: false,
        },
    ]
}

/// A tool of the Composers' catalogue, or one of an outside service such as Jira.
pub fn get(name: &str) -> Option<ToolDef> {
    all()
        .into_iter()
        .chain(crate::integrations::jira::tools::defs())
        .chain(std::iter::once(crate::genui::tool_def()))
        .find(|d| d.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn names_are_unique_and_every_schema_is_an_object() {
        let tools = all();
        let names: HashSet<_> = tools.iter().map(|t| t.name).collect();
        assert_eq!(names.len(), tools.len());
        for t in &tools {
            assert_eq!(t.schema["type"], "object", "{}", t.name);
            assert!(!t.description.is_empty());
            // Everything required is described.
            for name in t.schema["required"].as_array().unwrap() {
                assert!(
                    t.schema["properties"].get(name.as_str().unwrap()).is_some(),
                    "{}: {name}",
                    t.name
                );
            }
        }
    }

    #[test]
    fn the_catalogue_matches_the_spec() {
        let tier = |n: &str| get(n).unwrap().tier;
        assert_eq!(tier("list_projects"), Tier::Read);
        assert_eq!(tier("create_workspace"), Tier::Write);
        assert_eq!(tier("create_pr"), Tier::External);
        assert_eq!(tier("merge_pr"), Tier::External);
        assert_eq!(tier("delete_workspace"), Tier::Destructive);
        assert_eq!(all().len(), 21);
    }

    #[test]
    fn reads_are_safe_to_repeat_and_nothing_that_creates_is() {
        for t in all() {
            if t.tier == Tier::Read {
                assert!(t.repeat_safe, "{}", t.name);
            }
        }
        assert!(!get("create_workspace").unwrap().repeat_safe);
        assert!(!get("start_chat").unwrap().repeat_safe);
    }

    #[test]
    fn tools_that_return_outside_text_are_labelled() {
        for name in ["get_chat_tail", "get_changes", "get_pr_status"] {
            assert!(get(name).unwrap().untrusted, "{name}");
        }
    }
}
