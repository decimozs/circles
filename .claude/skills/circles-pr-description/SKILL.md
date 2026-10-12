---
name: circles-pr-description
description: >-
  Write the body of a Circles pull request from the repository's PR template: Description (ticket
  link first when there is one), Summary, Changes, Evidence, Acceptance Criteria and Stack. Use when
  drafting or checking a PR description, when the user runs /circles-pr-description, or from
  circles-create-pr. Evidence is exact command output, never a claim.
---

# circles-pr-description

Fill `.github/pull_request_template.md` exactly. Keep its headings and order. No emoji, and no Claude
or Anthropic attribution anywhere in the body.

## Sections

1. **Description.**
   - With a ticket: the first line is `**Ticket:** https://<site>.atlassian.net/browse/<KEY-n>`.
     If the link cannot be built, stop and ask. Without one write `None`.
   - Then 1 to 3 sentences on what was wrong or wanted and why. Name the symptom the person saw,
     then the cause. Not a list of files.
2. **Summary.** 2 to 4 bullets a reviewer can read in ten seconds.
3. **Changes.** Grouped by area (`apps/desktop/src-tauri`, `apps/desktop/src`, `packages/*`, `.github`,
   `docs`). Each bullet names the file or module and what changed. It must match
   `git diff --stat origin/main...HEAD`. Anything outside the PR's intent is listed under
   "Outside the intent" with a reason, or removed from the PR.
4. **Evidence.** The exact commands that were run, each with its shortest decisive result (pass
   counts, the failing line that is now green). Run them now if they were not run on this HEAD. A
   claim without output is not evidence. Add screenshots or recordings for UI changes.
5. **Acceptance Criteria.** Only when there is a ticket: copy its table and set Result to `Pass`,
   `Fail` or `Pending` with evidence per row. A PR with a `Fail` is opened as a draft and says so.
   Without a ticket, delete the section.
6. **Stack.** Position (level n of m), the PRs below, and the merge note. `circles-create-pr` fills
   this from the live stack; never guess it.

## Notes

- Mention a flaky or unrelated test only when it affected this run, and say it was not changed.
- Say what was **not** checked (for example "not run in the running app"). Do not hide gaps.
- Existing behaviour that changes for users goes in Summary, in plain words.
- Re-read the body once with `gh pr view --json body -q .body` after opening: the template comments
  must be gone and no attribution present.

## Steps

1. `git log origin/main..HEAD --oneline`, `git diff --stat origin/main...HEAD`, the ticket if any.
2. Draft against the template. Print the whole body in chat.
3. Hand it to `circles-create-pr`, or write it to a file for `gh pr create --body-file`.
