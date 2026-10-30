# Workspaces

A workspace is a copy of a project on its own branch, made with `git worktree`. Agents work in it, so your original folder and your other workspaces are never touched. A project can have as many workspaces as you like, and agents in different workspaces work at the same time.

## Creating one

Press **Create workspace** (Cmd+N). Pick the project, describe the work in the box ("What do you want to work on?"), choose a model and effort, attach files if you want, and press Enter. The `···` menu lets you change the base branch or start from an existing branch or pull request.

Circles then:

1. fetches `origin/<base>`,
2. makes a branch and a worktree at `~/circles/workspaces/<project>/<name>`, where `<name>` is a city name unique within the project,
3. copies the files named in **Files to copy** (and in a `.worktreeinclude` file), so ignored files such as `.env` are there,
4. runs the project's setup script, and
5. starts a chat with your message.

Circles keeps private notes for a workspace in `.context/`, which it adds to the worktree's local exclude, so it never touches the repository's `.gitignore`.

The branch name follows the project's [branch naming rule](projects.md) when it has one. You can rename the branch from the workspace menu.

## States

A workspace is `creating`, `setup_running`, `ready`, `working`, `idle`, `blocked` (an agent waits for you), `archived` or `errored`. The sidebar shows the state, and a small icon shows how the branch stands against its base. If the app was closed in the middle of something, Circles checks every workspace against `git worktree list` when it starts. A workspace whose folder is gone shows an error: open it and press **Restore**, and Circles rebuilds the folder from the branch.

## Archive, restore and delete

**Archive** runs the project's archive script, warns if there are uncommitted changes, removes the worktree and **keeps the branch**. **Restore** recreates the worktree from the branch. Deleting an archived workspace removes its record, not its branch.

If you delete a workspace that is open, Circles goes to Home, which offers to add a project when there is none.

## The right pane

Next to the chat, the right pane has three tabs:

- **All files:** a lazy file tree with search and a switch to show or hide ignored files. Opening a file shows it in a large dialog over the chat, read-only unless you choose to edit. The context menu copies the path, reveals the file in Finder, opens it in an editor or attaches it to the chat.
- **Changes:** the changed files with `+` and `-` counts, grouped by folder if you like, for all changes against the base or only uncommitted ones. It updates as files change. Opening a file shows its diff (Cmd+Shift+D) side by side or in one column, with arrows to the neighbouring files.
- **Checks:** where the branch stands, the pull request and its checks. See [Pull requests](pull-requests.md).

Click the gutter of a diff or file to **comment on a line or a range**. Open comments are sent to the agent as structured context, and you can resolve or reopen them.

At the bottom of the pane the **Graph** shows the branch drawn against its base. The expand button at the right of its header opens it in a large dialog over the chat. Escape closes it.

## Terminal and scripts

The terminal dock has tabs for **Setup**, **Run** and **Terminal**, with `+` for more terminals (Cmd+T). A terminal opens in the worktree with your login shell's environment. Cmd+Shift+M makes the terminal fill the center.

Scripts come from the project's settings:

| Script | When it runs |
|---|---|
| Setup | Once in a new workspace, after the files are copied. |
| Run | When you press **Run** (Cmd+R). Stop it with Cmd+Shift+R. Several named run scripts show up in the Run menu. |
| Archive | Before a workspace is archived. |

Scripts get these variables: `CIRCLES_WORKSPACE_NAME`, `CIRCLES_WORKSPACE_PATH`, `CIRCLES_ROOT_PATH`, `CIRCLES_DEFAULT_BRANCH` and `CIRCLES_PORT`, plus any you set under `[environment_variables]`. Each workspace gets its own block of 10 ports starting at 55000, so two workspaces never fight over a port. Use `$CIRCLES_PORT` in a run script. The run mode `nonconcurrent` stops the run scripts of the project's other workspaces before starting one.

Stopping a script sends SIGTERM to its process group and SIGKILL after 5 seconds. When you quit, Circles stops everything it started and asks first if something is still running.
