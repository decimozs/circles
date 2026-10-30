# Troubleshooting

**macOS will not open Circles ("cannot be opened", "unidentified developer" or "is damaged")**
The app is not signed by an Apple Developer account yet. The first time: right-click the app in
Applications, choose Open, then Open again. Or open System Settings, Privacy & Security, scroll to
the message about Circles and choose Open Anyway. If macOS says the app is damaged, the download
was marked as quarantined: run `xattr -dr com.apple.quarantine /Applications/Circles.app`.

**A notification says it is from Script Editor**
Circles could not send it itself and used a script. Allow notifications in System Settings,
Notifications, and use the installed app rather than a development build.

**macOS asks again and again for a Keychain password**
A development build is a new program every time it is built, so macOS asks again. The installed
app asks once: choose Always Allow.

**The title model is missing (building from source)**
Run `pnpm model:fetch`, then build again. The release script stops when it is not there.

**"Claude Code was not found"**
Install it from https://claude.com/claude-code, run `claude` once in a terminal to sign in, then
use Settings > Agents > Check. If it lives somewhere unusual, set its path there.

**"The GitHub CLI is not signed in"**
Run `gh auth login` in a terminal. Install the CLI first with `brew install gh` if needed.

**git is missing**
Run `xcode-select --install`.

**A workspace shows an error after a crash or restart**
Open it and press Restore. Circles repairs a missing worktree from the branch.

**A script or agent cannot find a tool that works in my terminal**
Circles reads the environment from your login shell at startup. Settings > Environment shows
what is set, and the diagnostics report lists which tools were found.

**Reset**
Settings > Advanced > Reset app data deletes chats and workspace records but keeps your settings
and repositories.

**Reporting a bug**
Settings > Advanced > Copy diagnostics, or Export logs. Both remove secrets first. Logs live in
`~/Library/Application Support/circles/logs`.
