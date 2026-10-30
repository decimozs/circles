# Repositories: adding them, and how the app follows their habits

How a repository gets into Circles, what the right pane shows for a workspace, and how Circles learns a repository's own way of working. This describes the app as it is now. The phase docs describe what each phase planned, and some of it changed later.

## Adding a project

Add project has three tabs: a local folder, GitHub, and quick start.

### GitHub

- The picker lists the repositories of the account `gh` is signed in as, then those of each organization that account belongs to, up to 100 per owner, newest push first. With more than one owner the list is grouped under an owner header.
- It says which account it uses ("Signed in as ..."). `gh` has one active account, and several can be signed in. If the active one owns no repositories and belongs to no organization, the list says "No repositories found for `<account>`". Switch with `gh auth switch --user <login>`.
- The search box matches the owner, the name and the description across every owner. A repository outside the first 100 of its owner is cloned by typing `owner/name` in the box above the list.
- An organization that cannot be listed, for example one that needs SSO, is named in a note under the list, and the rest still show.
- Cloning goes to `~/circles/repos/<name>`.

### Adding a removed project again

Removing a project takes it out of Circles and keeps its clone. Adding the same GitHub repository again reuses that folder when it holds the same repository (same `origin`) and no project uses it. A folder of another repository is never taken over: the clone goes to `<name>-2`, `<name>-3` and so on. Nothing is deleted.

## The right pane of a workspace

- **Tabs:** All files, Changes and Checks. The Review tab and the todos were removed. Line comments from the diff and file viewers still work. A saved layout that was on the Review tab opens All files.
- **Checks:** git status, the pull request, checks, deployments and review threads. A merge is held back by failing checks, conflicts and requested changes, and can be done anyway with an explicit override.
- **Graph:** at the bottom of the pane, the branch drawn against its base. The expand button at the right of its header opens it in a large dialog over the chat, like a file from the tree. Escape closes it.

## A workspace that no longer exists

If the page of a workspace is open when the workspace is deleted, the app goes to Home. Home offers to add a project when there is none. Other errors are still shown on the page.

## Following a repository's own way of working

Claude Code loads `CLAUDE.md` and `.claude/skills` by itself in every workspace. Circles adds a note about `.agents/skills`, `.agent/skills` and `AGENTS.md`, which it does not load. So an agent in a chat follows what the repository writes down.

Circles' own drafts (the pull request title and description, and the grouping of commits) used to rely on the diff and the settings alone. They now also read what the repository writes down:

| What | Where it is looked for |
|---|---|
| Pull request template | `.github/pull_request_template.md`, `pull_request_template.md` at the root, `docs/pull_request_template.md`, or a `.github/PULL_REQUEST_TEMPLATE/` folder (`default.md`, else the first by name). Any letter case. |
| Conventions | `AGENTS.md`, `CLAUDE.md`, `CONTRIBUTING.md`, `.github/CONTRIBUTING.md`, `docs/CONTRIBUTING.md`. A link to the same file is read once. |

- The PR draft fills in the template (headings and order kept, placeholder comments removed) and follows what the files say about titles and descriptions. Commit grouping follows what they say about commit messages. The template is read up to 6,000 characters, each file up to 4,000, all of them together up to about 10,000.
- These come on top of the instructions in the settings (`instructions.general`, `.pr`, `.commits`).

### Learn

The repository settings page has a **Learn** row. It has Claude read the template, the convention files, the last 40 commit subjects and the 30 most recent branch names, and propose three settings:

| Setting | What it holds |
|---|---|
| `git.branch_pattern` | How branches are named, with `{type}`, `{slug}` and `{ticket}`. A proposal with no placeholder is dropped. |
| `instructions.pr` | How to write the title and description of a pull request. |
| `instructions.commits` | How commit messages should look. |

- A dialog shows each proposal beside the value it would replace. Only the ones kept are saved, to "Only me" (this Mac) or "Shared" (`.circles/settings.toml`), whichever is chosen on the page. Nothing is saved before that.
- It runs once, with no tools, away from the repository and without the person's own settings and `CLAUDE.md`. It uses the person's Claude subscription and sends the text to Anthropic, only when the button is pressed.
- A repository with no template, no convention files, no commits and no branches gives a message that there is nothing to learn from.
