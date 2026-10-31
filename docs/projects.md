# Projects

A project is a git repository you added to Circles. Circles never changes the original folder: agents work in workspaces made from it.

## Adding a project

**Add project** (Cmd+Option+A) has three tabs.

### Local folder

Choose a repository that is already on your Mac, or paste its path. Any folder inside the repository works. A repository with no commits yet, a folder that is not a git repository, and a folder that is already a git worktree are refused with the reason.

### GitHub

- The picker lists the repositories of the account `gh` is signed in as, then those of each organization that account belongs to, up to 100 per owner, newest push first. With more than one owner the list is grouped under an owner header.
- The list opens at once with what was found last time and is asked from GitHub again in the background when it is ten minutes old, so a repository created a moment ago may show up a moment later. Changing the active account clears it.
- It says which account it uses ("Signed in as ..."). `gh` has one active account even when several are signed in. If the active account owns no repositories and belongs to no organization, the list says "No repositories found for `<account>`". Switch with `gh auth switch --user <login>`.
- The search box matches the owner, name and description across every owner. A repository outside the first 100 of its owner is cloned by typing `owner/name` (or a URL) in the box above the list.
- An organization that cannot be listed, for example one that needs SSO, is named in a note under the list, and the rest still show.
- The clone goes to `~/circles/repos/<name>`.

### Quick start

Creates a new repository with an initial commit in `~/circles/repos/<name>`.

## Removing and adding again

Removing a project takes it out of Circles and keeps the repository folder, with or without its workspaces. Adding the same GitHub repository again reuses that folder when it holds the same repository (same `origin`) and no project uses it. A folder of another repository is never taken over: the clone goes to `<name>-2`, `<name>-3` and so on. Nothing is deleted.

## Repository settings

Each project has its own page under Settings, with:

- **Base branch:** new workspaces branch from it and pull requests target it. Circles reads `origin/HEAD`, then falls back to `main` and `master`.
- **Scripts:** setup, run and archive, and whether the run script can start in several workspaces at once. See [Workspaces](workspaces.md).
- **Files to copy:** ignored files, such as `.env`, copied into every new workspace.
- **Merge method:** squash, merge commit or rebase.
- **Instructions:** text added to the prompts Circles sends in this repository. There is one for everything, and ones for reviews, pull requests, fixing errors, conflicts, branch names and commits.

The **Branch naming rule** (Settings > Git, and in a project's shared or local settings file) sets how a project's branches are named, for example `{type}/{ticket}-{slug}`. `{type}` is feat, fix, chore and so on, `{slug}` is lowercase words joined by dashes, and `{ticket}` is an issue key such as `PROJ-123`. Left empty, it is `{type}/{slug}`. Composers follow it. **Learn** (below) can fill it in for a project.

Changes are saved either to **Only me** (this Mac) or to **Shared**, which writes `.circles/settings.toml` in the repository so everyone who clones it gets the same settings. See [Settings](settings.md).

## How Circles follows a repository's habits

Claude Code loads `CLAUDE.md` and `.claude/skills` by itself in every workspace. Circles also tells the agent about `.agents/skills`, `.agent/skills` and `AGENTS.md`, which Claude Code does not load. So an agent in a chat follows what the repository writes down, whoever started the chat.

Circles' own drafts of a pull request and its grouping of commits read the repository too:

| What | Where it is looked for |
|---|---|
| Pull request template | `.github/pull_request_template.md`, `pull_request_template.md` at the root, `docs/pull_request_template.md`, or a `.github/PULL_REQUEST_TEMPLATE/` folder (`default.md`, else the first by name). Any letter case. |
| Conventions | `AGENTS.md`, `CLAUDE.md`, `CONTRIBUTING.md`, `.github/CONTRIBUTING.md`, `docs/CONTRIBUTING.md`. A link to the same file is read once. |

The pull request draft fills in the template (headings and order kept, placeholder comments removed) and follows what the files say about titles and descriptions. Commit grouping follows what they say about commit messages. The template is read up to 6,000 characters, each file up to 4,000, and all of them together up to about 10,000. This comes on top of the instructions in the settings.

### Learn

The repository settings page has a **Learn** button. It has Claude read the template, the convention files, the last 40 commit subjects and the 30 most recent branch names, and propose three settings:

| Setting | What it holds |
|---|---|
| Branch naming rule (`git.branch_pattern`) | A rule with `{type}`, `{slug}` and `{ticket}`. A proposal with no placeholder is dropped. |
| Pull request instructions (`instructions.pr`) | How to write the title and description. |
| Commit instructions (`instructions.commits`) | How commit messages should look. |

A dialog shows each proposal next to the value it would replace. Only the ones you keep are saved, to **Only me** or **Shared**, whichever is chosen on the page. Nothing is saved before that. Learn runs once with no tools, away from the repository and without your own Claude settings. It uses your Claude subscription and sends that text to Anthropic, only when you press the button. A repository with no template, no convention files, no commits and no branches gives a message that there is nothing to learn from.
