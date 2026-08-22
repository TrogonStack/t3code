# Message composer

Messages can contain up to 120,000 characters. If a draft is longer, T3 Code keeps it in the
composer and shows how many characters need to be removed. Shorten the draft or split it into
multiple messages, then send again in the same thread.

On desktop, press `Cmd+Enter` on macOS or `Ctrl+Enter` on Windows and Linux from a new thread to
start it in the background. T3 Code opens another new thread and shows an **Open** action for the
thread that started. The new thread keeps the selected workspace mode and base branch. If **New
worktree** is selected, each background thread creates its own worktree.

## Background work

When a turn ends while work is still running, a banner sits above the composer and names that
work: the agents still going, or the watch loops and background shells waiting on something.

Open **Details** on the banner for the full list, with each item's latest progress line and when
it last reported. A watch loop that is waiting and one that has stopped making progress look the
same in the banner, and the progress line is what tells them apart.

**Stop** ends every item in that list at once and interrupts the session. Items cannot be stopped
one at a time.
