# Pull requests

Circles uses the GitHub CLI (`gh`) for pull requests, so it needs `gh` signed in. It never stores a token. Pushing uses your existing git credentials.

## The next step

The top bar of a workspace shows one button for the next thing worth doing, driven by the state of the branch:

| State | Button |
|---|---|
| Commits and no pull request | **Create PR** (Cmd+Shift+P) |
| A reviewer asked for changes | **Respond to review** |
| Checks are failing | **Fix failing checks** |
| The branch conflicts with its base | **Resolve conflicts** |
| Everything is fine | **Merge**, with a "Ready to merge" label |
| Merged | **Archive** |

The pull request number shows as a chip (`#123`) that opens it on GitHub. **Fix failing checks** and **Respond to review** start an agent turn with the failing logs or the unresolved threads as context. **Resolve conflicts** asks the agent to rebase or merge, following the project's conflict instructions.

## Creating one

**Create PR** opens a dialog with a title and a description. **Draft with Claude** writes both from the commits and the changes, with no tools and no chat. It reads the repository's pull request template and convention files and follows them: it fills in the template and keeps its headings, and follows what the files say about titles. See [Projects](projects.md). Preview the description as Markdown, edit it, and create. Lines that credit a tool, such as "Generated with" or a `Co-Authored-By: Claude` trailer, are removed from the draft. Circles pushes the branch, creates the pull request with `gh pr create` and remembers its number.

**Commit with Claude** groups your uncommitted changes into atomic commits: each is one logical change with a short message, and every file goes in exactly one commit. It follows the commit messages the repository's files describe. Claude only proposes the grouping, and Circles runs the git commands. You can also commit everything at once with a message you write.

## The Checks tab

It shows, from top to bottom:

- **Git:** the branch, how far it is ahead and behind its base, the working tree, and whether it was pushed.
- **Pull request:** its state, title and a link.
- **Checks:** the CI and status checks, with a link to each. They refresh while a pull request is open.
- **Deployments** and **review threads** from GitHub. A thread can be resolved from here and it resolves on GitHub too.

GitHub being unreachable is a normal state, not an error: the git part still shows.

## Merging

**Merge** uses the project's merge method (squash, merge commit or rebase, set in the repository settings) and then offers to archive the workspace. It is held back, with the reason, when checks are failing or pending, when the branch conflicts with its base, when a reviewer asked for changes, or when the pull request is a draft. Except for a draft, you can still merge by choosing **Merge anyway** in the dialog.
