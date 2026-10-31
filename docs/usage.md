# Usage

The **Usage** button at the bottom of the sidebar (the chart icon, next to the keyboard shortcuts and settings buttons) shows how much Claude Code has been used on this Mac, and what that would cost.

## Where the numbers come from

Circles reads Claude Code's own history: the files it keeps under `~/.claude` and, if you use more than one, any other `~/.claude*` folder, such as `~/.claude-work`. If you set `CLAUDE_CONFIG_DIR` in Settings > Environment, that folder is read too.

- It covers every Claude Code conversation on this Mac, not only chats started in Circles.
- Only token counts, model names and dates are read. The text of your conversations is not stored or sent anywhere.
- The first time takes a few seconds on a long history. After that only what has changed is read.

## Cost is an estimate

Cost is what the tokens would cost at the API's list prices. A Claude subscription is not billed per token, so on a plan this is not what you pay. It shows how much work the plan covered.

If Circles has no price for a model, its tokens are counted and its cost is left out, and the page says which model.

## The page

- **Range:** Past 24h, 7 days, 30 days or 90 days. Circles remembers your choice.
- **Environment:** all Claude folders together, or one of them.
- **Cost:** the estimated total and the number of sessions, a daily chart (point at it to see a day), totals, cost by type (input, cache read, cache write, output) and a breakdown by model or by day.
- **Tokens:** the same page in tokens instead of dollars.
- **Limits:** the plan limits Claude Code last reported, for the 5-hour and 7-day windows. Claude Code reports them while it works, so this stays empty until you have sent a message in a chat.

**Cached input** is input Claude read from its cache, which costs much less than new input. **Cache savings** is what those reads would have cost as new input, less what they cost.

## If it looks wrong

- **Nothing found:** the page lists the folders it looked in. Claude Code keeps no history until it has run once.
- **Some lines skipped:** Claude Code's history format is not fixed, so a line Circles cannot read is skipped and counted at the bottom of the page.
- **A price looks off:** prices are list prices at the time of writing and change. Treat the dollar figure as a guide.
