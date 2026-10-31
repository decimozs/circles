# Pull requests

Circles uses the GitHub CLI (`gh`) for pull requests, so it needs `gh` signed in. It never stores a token. Pushing uses your existing git credentials.

## Browsing pull requests

**Pull requests** in the sidebar, under Composer, lists the pull requests of the repositories you added, whether or not a workspace is on their branch.

- **Which repositories:** the project you worked in last, or all projects. Choose with the project button beside the search box.
- **Groups:** *Needs your review* (you are asked to review), *Yours*, and *Others*.
- **Each row:** state (open, draft, merged, closed), number, title, checks, review state, author, branch, size (`+added -removed`) and when it last changed.
- **Search:** plain words match the title, branch, author and repository. `#123` finds a number. `label:bug`, `author:ana`, `repo:backend`, `branch:feat/`, `is:draft`, `is:mine`, `review:approved` and `checks:failing` narrow it. Put quotes around a value with spaces: `label:"good first issue"`.
- **Sort and Filters:** sort by activity, age or size. Filters choose open, merged, closed or all, hide or show drafts, and filter by checks and review.
- **If a repository cannot be asked** (for example `gh` is not signed in for it), its name and the fix are shown above the list, and the other repositories still show.

Choosing a pull request opens it beside the list, in a tab. The tabs are in the top bar. Open several and switch between them. The **+** beside the tabs lets you pick another, and the x closes one. The list narrows to a column while something is open.

### In a tab

- **Header:** the repository and number, the title, who opened it, when it last changed, the base branch and the branch it comes from, how many files it changes and its size, and **Open on GitHub**.
- **Summary:** who reviews (who approved, who asked for changes, who was asked and has not answered), labels, the description as formatted text, the checks (open the list to see each one and its link) and the comments. A branch that cannot be merged because of conflicts says so.
- **Timeline:** the opening, every commit, review and comment, and the merge, newest first. The button at the top reverses the order.

- **Code:** the files the pull request changes, each with its status (added, modified, deleted, renamed), its size and a **Viewed** box. Open a file to see it before and after in a diff editor, side by side or in one column. Two files can be open at once. Marking a file as viewed puts it away and counts it, and Circles remembers that until the branch gets a new commit. A binary file or one that is too large says so; open the pull request on GitHub to see it.

Circles reads the files from your own copy of the repository when it already has the commits, and from GitHub when not. It does not fetch into your repository.

Pictures inside a description or comment are not shown. Circles only loads avatars from GitHub, so open the pull request on GitHub to see them.

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
