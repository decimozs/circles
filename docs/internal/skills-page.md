# A Skills page: plan

Status: built in phases 1 to 5. Checked by tests and a run against this Mac's folders; the pages are not yet seen in the running app. A **Skills** item in the sidebar, before **Pull requests**, opens a directory of
the skills that exist on this Mac and in the repositories added to Amplify, laid out like the list of pull
requests: a searchable, grouped list, and a pane that opens beside it for the one you pick.

## What a skill is here

A folder holding a `SKILL.md` (and possibly scripts, references and assets). Its front matter has a `name` and a
`description`, and sometimes `allowed-tools`, `argument-hint`, `model` or `disable-model-invocation`. Claude Code
reads the ones in its own folders by itself; Amplify' agents are also told about a project's skills
(`chat/project_skills.rs`), which already lists `.claude/skills`, `.agents/skills` and `.agent/skills` of a
workspace. The page shows what exists and where it comes from. It does not decide which skill a chat uses.

## Where they are found (checked on this Mac)

| Group | Where | On this Mac |
|---|---|---|
| **Yours** | `<config dir>/skills` for each Claude config folder (`~/.claude`, `~/.claude-work`, and the `CLAUDE_CONFIG_DIR` of Settings > Environment; the usage scan already finds these) | `~/.claude-work/skills` is mostly symlinks into `~/.claude/skills`, so each skill is listed once, by its real path, with both folders named |
| **Shared** | `~/.agents/skills` (other agents' shared folder) | about 40 skills |
| **Plugins** | each installed plugin's `skills` folder, from `~/.claude/plugins/installed_plugins.json` (`installPath`) | one plugin installed; it has hooks, no skills |
| **Projects** | `.claude/skills`, `.agents/skills`, `.agent/skills` of each project added to Amplify | Amplify' own `.claude/skills` has 8 (`amplify-*`) |

A project's skills are read from the project's folder. A workspace on another branch may differ; the page says
"from the project folder" and a later phase can read a workspace's own copy.

## The page (modelled on the pull requests page)

- **Route** `/skills`, sidebar item `Skills` (sparkles or book icon) between Composer and Pull requests.
- **List** (full width until one is open):
  - Search over name and description (plain text, `group:`, `project:`).
  - `Sort` (name, recently changed), `Filters` (group, project, has scripts, hidden from the model),
    `All` / this project scope, refresh.
  - Groups `Yours`, `Shared`, `Plugins` and one group per project, each with its count.
  - Row: icon, name, one-line description, a small badge for where it is (`~/.claude`, `amplify`, a plugin
    name), and its age.
- **Pane** beside the list (resizable like the pull request pane, width remembered):
  - **Overview:** name, description in full, where it is, the front matter fields (allowed tools, argument
    hint, whether the model may call it by itself), and what else is in its folder.
  - **SKILL.md:** rendered Markdown, with a switch to the raw text.
  - **Files:** the folder's tree; a file opens as text (Monaco read only, as the pull request Code tab does).
  - Tabs of open skills in the top bar, like the open pull requests, so you can keep a few.
- **Actions, read only first:** copy the path, reveal in Finder, open in the editor, open on GitHub for a
  project skill whose repository is on GitHub, and **Use in chat** (puts `/skill-name` in the message box).
  Creating, editing, installing and copying a skill between groups are a later phase.

## Backend (Rust)

- New module `skills/`: `scan.rs` (find the roots, read each `SKILL.md`, follow symlinks once and key by
  the canonical path), `frontmatter.rs` (parse the YAML front matter), and commands `skills_list(project_id |
  null)` and `skills_detail(path)`; `skills_file(path, rel)` for a file inside a skill.
- **Front matter needs a real parser.** The existing `description_of` reads one line, but our own skills use
  folded descriptions (`description: >-` across lines). A small maintained YAML crate is needed; which one is
  checked for maintenance and advisories before it is added (the repository now audits its dependencies).
- ts-rs types `SkillRow` and `SkillDetail` in `packages/bindings`.
- The list is cached with the shared `cache` table and shown at once, then refreshed (the same
  stale-while-revalidate as the pull request list). A watch on the roots (`notify` is already a dependency)
  refreshes it when a skill is added or edited, debounced.
- Limits: at most 500 skills, a `SKILL.md` over 256 KB shows "too large to show", file text over 1 MB the same,
  a tree of at most 300 entries, no following a symlink out of the skill's own folder for file reading.

## Security

- **Skill text is untrusted**, especially a project's: it is rendered with `react-markdown` and `remark-gfm`
  only, no raw HTML, links opened with `openUrl`, never run. Nothing in a skill is executed by the page.
- The commands read only below the known roots and refuse a path that resolves elsewhere (a `..` or a symlink
  out), as the file commands do for workspaces.
- Project skills can say anything to an agent: the pane marks one as "from a repository" and **Use in chat**
  on it shows what it is first. This is a read-only page; it adds no new way for a skill to run.
- Local only: nothing is sent anywhere.

## Phases (stacked PRs)

1. **Backend list.** `skills/` scan and parser, `skills_list`, fixtures for symlinks, folded descriptions, a
   missing `SKILL.md`, a broken link. No UI.
2. **Sidebar item, route and list.** Rows, groups, search, sort, filters, scope, empty and error states.
3. **Pane.** Overview, SKILL.md and Files, tabs in the top bar, resizable width.
4. **Actions.** Copy path, reveal, open in editor, open on GitHub, Use in chat.
5. **Docs.** A guide page `skills.md`, `shortcuts.md` if a key is assigned, `CHANGELOG.md`.

## Tests

- Rust: front matter (plain, quoted, folded, missing, broken), symlinked duplicates listed once with both
  folders named, a project root, a plugin root, size limits, a path outside the roots refused.
- Frontend (Vitest with the IPC mock): grouping and counts, search syntax, sort, scope, tabs open and close,
  the empty and error states, Use in chat writes the command, Markdown with raw HTML shows as text.
- Not testable here: a real run against your folders (about 50 skills and symlinks); checked by hand.

## What changed while building

- **No YAML crate.** A small flat parser reads the front matter (folded, literal, quoted, inline and block lists),
  so no dependency was added.
- **No cache table.** The scan reads about 50 small files and is quick; TanStack Query keeps the list and reads
  it again on focus.
- **Phases 2 and 3 in one PR** (list and pane). The pane's width is shared with the pull request pane.
- **File text is a plain scrollable block,** not Monaco.
- **Use in chat is Copy /name.** The page has no open chat to put it in, so it copies the command. Reveal and
  Open (SKILL.md in the text editor) are macOS only for now, like the other Finder actions.
- **Defaults used for the open questions:** all four groups, read only, tabs in the top bar, project folder,
  copy for Use in chat.

## Questions for you

1. **Which groups?** Recommended: Yours, Shared (`~/.agents`), Plugins and Projects. Drop Shared if you do not
   want other agents' skills in Amplify.
2. **Read only first?** Recommended: yes. Creating and editing skills is phase 6, if wanted.
3. **Tabs in the top bar,** like pull requests, or one pane that replaces its content?
4. **Project skills from the project folder,** or from a workspace's own branch? Recommended: the project
   folder first.
5. **Use in chat:** insert `/name` in the open chat, or only copy it?

## Process

Jira is not set up, so each PR is a recorded "Ticket: None" exception.
