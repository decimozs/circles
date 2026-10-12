# Setup audit: what macOS may ask, and when

Written 2026-10-11 from the code. It has **not** been run on a clean macOS user account yet: do
that with the release checklist and record the results at the bottom.

| Prompt or need | Triggered by | When | Handled by |
|---|---|---|---|
| "Circles would like to access files in your Desktop, Documents or Downloads folder" | The first read of a protected folder, such as adding or cloning a repository there | The first time a repository in one of those folders is used | Setup, "Folders where your code lives" reads them on purpose, during setup |
| Removable and network volumes | Reading a repository on one | First use | Same row, and the system prompt |
| Notifications | `notify_via` (waiting approvals, cards, test) | The first notification | Setup, "Notifications" sends a test. Circles sends it itself, and uses a script (shown as Script Editor) only when it cannot |
| Keychain ("wants to use your confidential information") | Reading or saving a Jira token or sign-in | Connecting Jira, and each Jira call after an unsigned build changed | Setup explains it. The state of a connection is kept in a plain file so showing it never asks |
| Open at login | The "Open Circles at login" setting | When switched on | Setup shows the state. macOS lists it under Login Items |
| Full Disk Access | Nothing | Never needed | Setup explains it is not needed, and links to the pane |
| Automation (control Terminal) | The Setup button "Open Terminal" runs `osascript` | When pressed, once | macOS asks "Circles wants to control Terminal". Allow it, or run `claude` yourself |
| Accessibility, Camera, Microphone, Screen recording, Location, Contacts | Nothing | Never | none |
| Network (outgoing) | `gh`, `git`, Claude Code, Jira | Always allowed on macOS | none |
| Gatekeeper ("cannot be opened") | Opening an app that is not signed | The very first launch | Not signed, no Apple Developer account. README and `docs/troubleshooting.md` say right-click, Open |

## Results of a clean-account run

Not done yet. Record here: macOS version, chip, what prompted, in which order, and what failed.
