# Settings

Open Settings with Cmd+, . Search them with Cmd+F inside the Settings page. Each setting has a title, a description and a control. A setting that you have changed from its default has a reset button.

## Where settings are kept

Settings are TOML files in three layers. A later layer wins over an earlier one:

| Layer | File | For |
|---|---|---|
| User | `~/Library/Application Support/circles/settings.toml` | Everything on this Mac. |
| Project (shared) | `<repository>/.circles/settings.toml` | Settings that belong to a repository. Commit it to share them. |
| Local project | `~/Library/Application Support/circles/projects/<id>/settings.toml` | Settings for one repository on this Mac only. |

The repository settings page has a switch, **Only me** or **Shared**, that chooses where a change is written. The Settings page has a button that opens any of the files. Settings files are watched, so an edit outside the app shows up in the page.

## The pages

**Personal**

| Page | What it holds |
|---|---|
| General | The send key (Enter or Cmd+Enter), what a message sent while the agent works does (steer or queue), notifications and the completion sound, full disk access, and whether long pasted text becomes an attachment. |
| Accounts | A card for the GitHub account the GitHub CLI uses, with a **Switch account** menu when several are signed in, and a card for the Claude account Claude Code is signed in with, with its plan. Each says what to run when it is not signed in. Circles never holds either login. |
| Appearance | Theme (light, dark or system), font size and density. |
| Default models | The model and effort a new chat starts with, and whether new chats start in plan mode. |
| Git | The branch prefix, the branch naming rule, the merge strategy, and the commit author git will use. |

**Agents and environment**

| Page | What it holds |
|---|---|
| Agents | The `claude` to use, a check of its version, the default permission mode, extra arguments for every Claude Code start, the model for side questions and whether they may read files, and whether chats may ask with cards. |
| Smart titles | Naming new chats, suggesting the next message and an example for a new Composer's first message. They use Claude Haiku through Claude Code, which spends a little of your Claude usage. |
| Environment | Environment variables set for every agent and terminal, and for the calls Circles makes to Claude Code itself (titles, drafts, Improve, Learn and the Claude card). Values are hidden and never logged. |
| Composers | Pause and stop, the daily cap of runs started on their own, notifications, the menu bar, open at login, keep awake, and cards. |
| Integrations | The Jira connection. See [Jira](jira.md). |

**Repositories:** one page for each project. See [Projects](projects.md).

**More**

| Page | What it holds |
|---|---|
| Experimental | Switches for features that are not finished. |
| Advanced | Data folders, log level, how long checkpoints are kept, **Clear cached data** (the copies of pull requests kept so the page opens at once), export logs, copy diagnostics, the first-run setup screen, and **Reset app data**, which deletes chats, workspace records and terminal history and keeps your settings and repositories. |

## A repository's settings file

```toml
files_to_copy = [".env", ".env.local"]

[scripts]
setup = "pnpm install"
run = "pnpm dev --port $CIRCLES_PORT"
archive = "./scripts/cleanup.sh"
run_mode = "concurrent"   # or "nonconcurrent"

[git]
merge_method = "squash"   # or "merge" or "rebase"
branch_pattern = "{type}/{ticket}-{slug}"

[instructions]
general = "Follow the style guide in CONTRIBUTING.md."
pr = "Start the title with the ticket key."
commits = "Use conventional commits."
```

`[scripts.run]` can instead hold several named run scripts, each with a `command`, an optional `default = true` and an `icon`. Everything in the file is optional, and an empty file changes nothing.

## Using another Claude account

Claude Code keeps its login in a config folder, `~/.claude` by default. If you have signed in to another account in another folder, for example `~/.claude-work`, add a variable in Settings > Environment:

| Name | Value |
|---|---|
| `CLAUDE_CONFIG_DIR` | `/Users/<you>/.claude-work` |

New chats, Composer runs, titles, drafts, Improve, Learn and the Claude card in Settings > Accounts then work as that account. A chat that began under the other account cannot be resumed by Claude Code, which only finds conversations in the folder it runs with. Circles says so, and your next message continues in a new session that is told what was said earlier. To use it for one repository only, put the variable in that repository's settings file under `[environment_variables]`, as **Only me**: a shared file would commit your folder path. Terminals inside Circles get the variable too, so `claude` there uses it as well.
