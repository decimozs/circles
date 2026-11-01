---
title: "Skills"
description: "A directory of the skills on this Mac and in your projects, to read and find. Nothing in it runs."
---

A skill is a folder with a `SKILL.md` that tells an agent how to do one kind of task. **Skills** in the sidebar, above Pull requests, lists the ones that exist on this Mac and in the repositories you added. It only reads: nothing here changes a skill or runs one.

## Where Circles looks

| Group | Where |
|---|---|
| **Yours** | The `skills` folder of each Claude folder: `~/.claude`, `~/.claude-work` and the one set as `CLAUDE_CONFIG_DIR` in Settings > Environment. |
| **Shared** | `~/.agents/skills`, which other agents use as well. |
| **Plugins** | The `skills` of each plugin you installed in Claude Code. |
| **One group per project** | `.claude/skills`, `.agents/skills` and `.agent/skills` in the project's folder. |

A skill that two folders link to (a symbolic link) is listed once, by its real folder, and its row says **+1** with the other folder in its tooltip. A folder that starts with a dot, or has no `SKILL.md`, is not a skill.

## The list

- **Each row:** name, where it is from, what it says it is for, and when it last changed. **Manual only** means its front matter tells the model not to use it by itself.
- **Search:** plain words match the name and description. `group:project`, `project:circles` and `source:~/.claude` narrow it.
- **Sort and Filters:** sort by name or by when it changed. Filters pick a group, whether the model may use the skill, and whether it has scripts or other files.
- **The project button** shows the skills of one project. Yours, Shared and Plugins stay.
- **Refresh** reads the folders again. They are read again when you come back to the window, so a skill added in an editor shows up.
- If nothing is found, the page lists the folders it looked in.

## Opening a skill

Choosing one opens it beside the list, in a tab in the top bar. The **+** beside the tabs goes back to the list, and the x closes one. The line between them can be dragged, and the width is shared with the pull request pane.

- **Overview:** what it is for, where it is (with a copy button), whether the model may use it by itself, its argument hint and model if it names them, the tools it may use without asking, and how many files it has.
- **SKILL.md:** the text as formatted Markdown, without the front matter. A switch shows the raw text. HTML in a skill is shown as text and never run.
- **Files:** the other files of the skill. Choose one to read it. Large files and files that are not text are not shown, and a link that leads out of the skill's folder is not followed.
- **Copy /name** copies the command that names the skill, to paste in a chat. **Reveal** shows the folder in Finder, **Open** opens `SKILL.md` in your text editor, and **GitHub** (for a project's skill on GitHub) opens its folder on the project's default branch.

A project's skills come from the repository, so they can say anything to an agent. The pane says so: read one before you use it.

Skills are read from the project's folder, not from each workspace's own branch, so a skill that exists only on another branch is not listed. Circles sends nothing anywhere.
