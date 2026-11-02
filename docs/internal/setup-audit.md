# Setup audit: what macOS may ask, and when

Written 2026-10-11 from the code. The Keychain row was removed afterwards: the Jira login is now kept in a file
(`integration-secrets.json`, mode 0600) instead, so Amplify does not ask for the Keychain at all. It has **not** been run on a clean macOS user account yet: do
that with the release checklist and record the results at the bottom.

| Prompt or need | Triggered by | When | Handled by |
|---|---|---|---|
| "Amplify would like to access files in your Desktop, Documents or Downloads folder" | The first read of a protected folder, such as adding or cloning a repository there | The first time a repository in one of those folders is used | Setup, "Folders where your code lives" reads them on purpose, during setup |
| Removable and network volumes | Reading a repository on one | First use | Same row, and the system prompt |
| Notifications | `notify_via` (waiting approvals, cards, test) | The first notification | Setup, "Notifications" sends a test. Amplify sends it itself, and uses a script (shown as Script Editor) only when it cannot |
| Open at login | The "Open Amplify at login" setting | When switched on | Setup shows the state. macOS lists it under Login Items |
| Full Disk Access | Nothing | Never needed | Setup explains it is not needed, and links to the pane |
| Automation (control Terminal) | The Setup button "Open Terminal" runs `osascript` | When pressed, once | macOS asks "Amplify wants to control Terminal". Allow it, or run `claude` yourself |
| Accessibility, Camera, Microphone, Screen recording, Location, Contacts | Nothing | Never | none |
| Network (outgoing) | `gh`, `git`, Claude Code, Jira | Always allowed on macOS | none |
| Gatekeeper ("cannot be opened") | Opening an app that is not signed | The very first launch | Not signed, no Apple Developer account. README and `docs/troubleshooting.md` say right-click, Open |

## Results of a clean-account run

Not done yet. Record here: macOS version, chip, what prompted, in which order, and what failed.
