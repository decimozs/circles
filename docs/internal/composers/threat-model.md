# Composers: threat model

| # | Threat | What stops it | Phase |
|---|---|---|---|
| 1 | The model runs away: spawns agents forever, loops on a tool | the optional `--max-budget-usd`, Claude Code's own rate limits, and the stop-all key | C1, C3 |
| 2 | Cost grows unseen | The run shows cost like any chat. A spend cap per Composer. A daily cap on triggered runs | C2, C6 |
| 3 | Text from a pull request, issue, file name, commit message or another agent tells the model to do something harmful | Tool results from outside text are wrapped and labelled as untrusted data. The system prompt says so. Most important, the app enforces autonomy itself: a tool set to `ask` asks no matter what the model was told. The model cannot change autonomy settings, because there is no tool for it | C1, C2, C7 |
| 4 | The model calls a tool it should not have | Tools set to `off` are not listed by the tool server and are refused if called anyway. Only the Composer's own tools exist | C1 |
| 5 | A path or branch name escapes where it should | Tools take typed fields. Branch and ref names go through the same `not_an_option` and name checks as the rest of the app. A tool never receives a shell string | C1 |
| 6 | Another local process calls the tool server | The Unix socket is mode `0600` in the app data folder, and every request needs a token made at app start. The token is in a `0600` file, never in logs or the database | C2 |
| 7 | Secrets leak through results or the audit log | The audit log stores redacted inputs and short result summaries. Results pass the same redactor as logs. Tool results never include environment values | C1 |
| 8 | A destructive call goes through by accident | Defaults ask. Enabling `allow` on an external or destructive tool shows a warning. Everything is in the audit log | C1, C2 |
| 9 | Two agents change the same files | `start_chat` refuses a workspace another Composer run is using. A run's children each get their own workspace | C3 |
| 10 | An unattended run does something while nobody watches | Only tools set to `allow` run. Everything else waits in the inbox. A global pause stops triggers and denies waiting approvals | C6 |
| 11 | A trigger fires repeatedly for the same event | Each event is recorded (`trigger_seen`) and fires once | C6 |
| 12 | The app dies mid-call | A call that was in flight is never repeated. Its approval is marked expired. The chat resumes with a note | C2 |
| 13 | A shared Composer file in a repository lowers the guard | A file can name tools and limits but never autonomy. Imported Composers get the safe defaults | C5 |

## Not covered

- The token is passed to the tool server through a file that only this user can read. Another process running as the same user can read it. That user can already read the whole app folder.
- The model can still be talked into misusing a tool that is set to `allow`. That is what `allow` means.
- Claude Code itself is trusted to honour `--tools ""`. The app never relies on that for the tiers.
