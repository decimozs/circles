# Usage

The **Usage** button at the bottom of the sidebar (the chart icon, next to the keyboard shortcuts and settings buttons) shows how much Claude Code has been used on this Mac, and what that would cost.

## Where the numbers come from

Circles reads Claude Code's own history: the files it keeps under `~/.claude` and, if you use more than one, any other `~/.claude*` folder, such as `~/.claude-work`. If you set `CLAUDE_CONFIG_DIR` in Settings > Environment, that folder is read too.

- It covers every Claude Code conversation on this Mac, not only chats started in Circles.
- Only token counts, model names and dates are read. The text of your conversations is not stored or sent anywhere.
- Circles keeps what it read in its own database, so the page opens at once with the last numbers and says how old they are (**Updated 3 min ago**). A few seconds after Circles starts, and whenever you open the page or press refresh, it reads only what is new in the background and the numbers update on their own.
- The very first time reads the whole history, which can take several seconds on a long one. The page shows how far it is and the numbers so far.
- Claude Code deletes old conversations after a while (30 days unless you changed it). Circles keeps the usage numbers it already read, so a 90 day range still covers 90 days. Only numbers, dates and model names are kept, never the text.

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

- **Start again:** **Rebuild history**, at the bottom of the page, forgets everything kept and reads Claude Code's history again. Usage of conversations Claude Code has already deleted cannot come back, so it asks first.
- **Nothing found:** the page lists the folders it looked in. Claude Code keeps no history until it has run once.
- **Some lines skipped:** Claude Code's history format is not fixed, so a line Circles cannot read is skipped and counted at the bottom of the page.
- **A price looks off:** prices are list prices at the time of writing and change. Treat the dollar figure as a guide.
