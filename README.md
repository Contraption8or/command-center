# Command Center

A local-first personal dashboard for Windows. One window shows your tasks, upcoming events, habits and goals, projects and quick notes, and stays out of the way while you edit. Written in Rust with [Dioxus](https://dioxuslabs.com) (desktop) and SQLite. Your data never leaves your machine.

> **Status:** all modules except **Spending** are built. Spending is on hold: its card shows this month's and last month's totals from sample data, and its full view is a placeholder (see [Roadmap](#roadmap)).

---

## Contents

- [Setup](#setup)
- [First run](#first-run)
- [Overview and window](#overview-and-window)
- [Modules](#modules): [Tasks](#tasks) · [Upcoming events](#upcoming-events) · [Habits & goals](#habits--goals) · [Projects](#projects) · [Quick notes](#quick-notes)
- [Command palette and quick add](#command-palette-and-quick-add)
- [Keyboard shortcuts](#keyboard-shortcuts)
- [Data, backups and sample data](#data-backups-and-sample-data)
- [Troubleshooting](#troubleshooting)
- [Architecture](#architecture)
- [Adding a widget](#adding-a-widget)
- [Roadmap and known limitations](#roadmap-and-known-limitations)

---

## Setup

### Download

Grab `command-center.exe` from the [latest release](https://github.com/Contraption8or/command-center/releases/latest) and run it; there's no installer.

- **Windows SmartScreen** may warn about an unrecognised app the first time, because the exe isn't code-signed. Click *More info → Run anyway*.
- **Updates:** the app checks this repository's releases when it starts. When a newer version exists, a banner offers **What's new** and **Download**; replace the old exe with the new one. Turn the check off, or run it by hand, in **Settings → Data → Updates**.
- **What the check sends:** one anonymous request to the GitHub API. Nothing about you or your data is sent.

### Build from source

Requirements:

- Windows 10/11 with the **WebView2 runtime**, which ships with Windows 11 and current Windows 10 builds.
- **Rust** stable (1.83 or newer), MSVC toolchain: <https://rustup.rs>.

Build and run:

```bash
cargo run --release -p command-center
```

- **Build time:** the first build takes a few minutes; later builds are incremental.
- **The program:** `target\release\command-center.exe` is a single self-contained file (about 7 MB) that you can copy anywhere.
- **Pin it:** right-click the running app's taskbar icon → *Pin to taskbar*.
- **Start with Windows:** press `Win+R`, type `shell:startup`, and put a shortcut to the exe in that folder. This also keeps the global quick-add hotkey available all day.

For development:

```bash
cargo run -p command-center        # debug build, keeps a console window for logs
cargo test --workspace             # unit tests (all core logic is tested without a UI)
```

## First run

- **Sample data:** the app starts with realistic sample data (Minecraft-channel projects, tasks, events, habits and so on), dated around today. When you're ready, go to **Settings → Data → Sample data → Clear**. Only sample rows are removed; anything you've added stays.
- **Global hotkey:** the default is `Ctrl+Alt+Space`. If another program already uses it, Settings → Shortcuts shows a warning; type another combination there and press **Apply**. See [Changing the global hotkey](#changing-the-global-hotkey).
- **Theme and clock:** dark theme and a 12-hour clock are the defaults. Change either in **Settings → Appearance**.

## Overview and window

- **Overview** (`Ctrl+0`): a grid with one card per module. Click ↗ on a card, its icon in the side rail, or `Ctrl+1…6` to open the module's full view. `Esc` goes back.
- **Customize the dashboard** by clicking the ✎ pencil in the top bar of the overview (or `Ctrl+K` → *Customize dashboard*). The cards stay live while you arrange them, so you see exactly what you'll get:
  - **Move:** drag a card anywhere. The others reflow around it as you go.
  - **Resize:** drag a card's right edge, bottom edge or corner. It snaps to grid slots, from 3 to 12 columns wide and 1 to 4 rows tall; the corner of the card header shows its current size, e.g. `4 × 2`.
  - **Hide** a card with ×. Hidden cards appear as **+ Name** buttons in the top bar; click one to bring it back.
  - **Reset** restores the defaults, and **Done** (or `Esc`) leaves edit mode. Changes save as you make them.
  - **Settings → Dashboard** has the same controls as a list (preset widths and heights, ↑ ↓ to reorder).
  - The side rail and the `Ctrl+number` shortcuts keep their fixed order, so muscle memory doesn't change when you rearrange cards.
  - Below 1150 px of window width, cards fall back to half or full width so they stay readable.
- **Fills any window size:** cards share the full width and height of the window and scroll inside when content is longer. Text never grows with the window: on wide windows the gaps and padding open up instead, and lists show more items. To make everything bigger or smaller, set **Settings → Appearance → Interface size** (90–150%, default 100%).
- **The window remembers** its size, position and maximized state between sessions. If it was last on a monitor that's no longer connected, it reopens centered on your main screen.
- **Motion:** views, overview cards, calendar views, side panels and overlays animate in briefly (about a quarter second, opacity and position only). With Windows' *Animation effects* turned off, the app's animations are off too.
- **One copy at a time:** opening Command Center while it's already running brings the open window to the front instead of starting a second copy.
- **Saving:** every change is written to disk the moment you make it, and closing the app folds everything into the main database file. If a save ever fails, an error toast says so.
- **The title bar matches the theme.** On Windows 11 its colour, text and border follow dark or light mode, so it blends into the app while keeping native snapping, resizing and the window buttons. On Windows 10 only the dark or light frame changes.
- **No loading screens:** everything is read from the local database instantly.

## Modules

### Tasks

- **Card:** today's list with an add field. Check items off in place; click a title to open it in the full view.
- **Full view** (`Ctrl+1`): today's list on the left and upcoming days on the right, grouped by day.
- **Add field:** accepts the [quick-add tokens](#quick-add-syntax) `!fri`, `@tomorrow` and `#project`. A hint shows how your text was read, and focus stays in the field so you can add several tasks in a row.
- **Detail editor:** click a task to edit its title, notes, due date, which day's list it's on, and its project, or to delete it. Changes save as you type.
- **Step tasks:** add `x3` (2–99) to a task, e.g. `Edit ep 8 x3`, to split it into 3 steps. The check circle becomes a progress ring: each click checks off one step (`1/3 · 33%`), right-click (or `Shift+Space`) undoes one, and the last step completes the task. Change the number of steps in the detail editor. Projects that track progress from tasks count partial steps.
- **Reorder:** drag a task by its handle, or select a row and press `Ctrl+↑` / `Ctrl+↓`.
- **Carry-over:** at startup and whenever the date changes, unfinished tasks from earlier days move to the top of today's list. The app checks the date every 30 seconds and when the window gets focus.
  - A ↻ chip shows how many days a task has slipped, and turns amber at 3 or more.
  - Moving a task to another day yourself resets that count.
- **Chips:** due dates show red when overdue and amber when due today.
- **Done tasks** sink to the bottom. In the full view they're hidden behind **Show done**.
- **Delete:** a toast offers **Undo** (or `Ctrl+Z`) for about 7 seconds.

### Upcoming events

- **Card:** the next 7 days as a compact agenda, skipping empty days.
- **Full view** (`Ctrl+2`) has three views (your choice is remembered). Use ‹ **Today** › to move between weeks or months.
  - **Agenda:** the next **7 or 14 days**, listing every day so you can see the gaps. Hover a day and press **+** to add an event on it. An event happening now has a green bar; finished ones are dimmed.
  - **Week:** a Monday–Sunday time grid with an all-day row and a red line at the current time. It opens scrolled to the morning, or to just before now.
    - **Drag on empty space** to create an event over that time range, in 15-minute steps. A single click makes a one-hour event. The new event opens in the editor with its title selected.
    - **Drag an event** to move it, including to another day. **Drag its bottom edge** to change the end time. Click it to open it.
    - Overlapping events sit side by side.
  - **Month:** six weeks with up to three events per day plus "+N more".
    - **Double-click** a day to add an event at 9:00.
    - Click a day number or "+N more" to open that week.
- **Editor:** title, date, all-day (with an optional "until" date for multi-day events), start/end time, location and notes. Everything saves as you change it.
- **Times:**
  - Moving the start keeps the duration.
  - An end time before the start runs past midnight, so a 10 PM–1 AM stream works.
- **Color:** pick one of eight colors (or Default, the theme accent) in the editor or from the right-click menu. Week blocks, month entries and the agenda dot all use it.
- **Delete:** in the editor or from the right-click menu, with Undo.
- **Quick add:** use the `e` prefix, for example `e Dentist @fri 9:15-10`. See [Quick-add syntax](#quick-add-syntax).

#### Timezones

The panel to the right of the calendar converts times for the people you work with.

- **What it shows:** with an event selected, each zone shows that event's time, the date, and a "+1 day" / "-1 day" tag when the date differs from yours. With nothing selected, it shows the current time in each zone.
- **Each zone's label** shows its abbreviation and its offset from you, e.g. `GMT · +6h`. Both follow daylight saving automatically.
- **Plan in someone else's time:** with a manual event selected, edit the time in any zone's row. Setting London to 8:00 PM moves the event to 8 PM London time, converted to your time, and keeps its length.
- **Zones:** the defaults are Los Angeles, New York, London, Berlin and Sydney. Click ⚙ to remove zones or add your own, with a name ("Alex") and an IANA zone ("Europe/London"); the zone field suggests names as you type.

#### Copy a time for Discord

**Right-click any time** to copy it for Discord. This works on events in the week and month grids, agenda rows, the editor's date line, and any row in the timezone panel. Right-clicking an event also offers its **colors** and **Delete event** (manual events only) above the copy options.

| Option | Copies | Discord shows (in each reader's own time zone) |
|---|---|---|
| Weekday, date and time | `<t:…:F>` | Thursday, November 12, 2026 12:45 PM |
| Date and time | `<t:…:f>` | November 12, 2026 12:45 PM |
| Time | `<t:…:t>` | 12:45 PM |
| Date | `<t:…:D>` | November 12, 2026 |
| Relative | `<t:…:R>` | in 2 days |

Paste the tag into a Discord message: everyone who reads it sees the time converted to their own time zone. **Copy as text for every zone** instead copies one plain line per zone (yours plus every saved zone), for places that don't understand Discord tags.

**Google Calendar (prepared, not connected yet).** The groundwork is in place:

- The `events` table already stores `source`, `external_id` and `calendar_id`.
- `crates/core/src/calendar.rs` defines an `EventSource` trait plus a tested `sync()`. Within the synced date range it adds new events, updates changed ones in place, and removes events deleted at the source.
- Synced events show a source chip and open read-only.

Connecting Google means implementing `EventSource` for the Calendar API (OAuth sign-in, token stored locally) and adding a "Sync now" button and a timer.

### Habits & goals

- **Card:** each habit shows a check button for today, its last 7 days as dots, and its current streak (`9d` = days, `3w` = weeks). Open goals and their progress bars are listed below.
- **Full view** (`Ctrl+3`): habits on the left, goals on the right.

**Habits** are **daily** or **N times a week** (1–7). Weeks start on Monday.

- **Check in:** click the round button. A weekly habit's button shows this week's count (`1/3`) and turns your color once the target is met.
- **Heatmap:** the last 26 weeks, one column per week, Monday at the top. Click any past day to add or remove a check-in, which is handy for catching up.
- **Stats:** current streak, best streak, and completion rate. The rate covers the last 30 days for daily habits and the last 4 full weeks for weekly ones.
- **Streak rules:** a streak isn't broken until its period is over.
  - A daily streak stays alive until midnight even if you haven't checked in yet today.
  - The current week only counts toward a weekly streak once its target is met.
- **Edit (⚙):** name, daily or weekly, times per week, and color.
  - **Archive** hides the habit but keeps its history.
  - **Delete** removes it (with Undo).

**Goals** move a number from a start value to a target, optionally by a deadline.

- **Progress** is measured from start to target, so goals that go down (debt, weight) work too.
- **Pace** compares your progress with a straight line from the day you created the goal to its deadline:
  - **Ahead**, **On track** (within ±10 percentage points) or **Behind**
  - **Done** once you reach the target, **Overdue** after the deadline
- **Footer:** deadline, days left, and how much you need **per week** to finish on time.
- **Update:** type a new value in the box and press Enter.
- **Edit (⚙):** title, start, current, target, unit, deadline and linked project. Delete has Undo.

### Projects

- **Card:** active, planned and paused projects, each with status, next action and progress bar.
- **Board** (`Ctrl+4`): projects grouped by status (Active, Planned, Paused, Idea, Done), with filter tabs at the top. Done projects are collapsed until you click **Show**.
- **Project cards** show:
  - the description and the next action (→)
  - progress
  - done/total linked tasks and the number of linked notes
  - "auto" when progress comes from tasks
- **Detail panel:** click a card to edit its name, status, next action, description and color. Everything saves as you type.
- **Progress** is set per project:
  - **Manual:** drag the bar to set it, in steps of 5%. Marking a manual project **Done** fills it to 100%.
  - **From tasks:** done ÷ total of the linked tasks, updated live as you tick tasks off anywhere.
- **Linked tasks** are listed in the panel.
  - A task added there is linked automatically; so is any task added with `#project` in quick add or a task field.
  - Check them off or edit them in place. They can't be dragged here, because manual order belongs to each day's list.
- **Linked notes** are listed with how long ago each was edited. Click one to open it in Notes; **+** creates a new note linked to the project.
- **Delete** keeps the project's tasks and notes, just unlinked. **Undo** restores the project and re-links them.

### Quick notes

- **Scratchpad card:** your notes list (pinned first, then most recently edited) next to an editor. Pick a note and type; it saves on every keystroke.
- **Full view** (`Ctrl+6`): a searchable list next to an editor with **Write / Split / Preview** modes (your choice is remembered).
- **Markdown:** headings, bold/italic, lists, `- [ ]` checklists, tables, code, quotes and links.
  - Click a checkbox in the preview to tick it; the source text updates to match.
  - Links open in your default browser. Only `http`, `https` and `mailto` links are followed.
  - Raw HTML in a note is shown as text and never run.
- **Pin** keeps a note at the top. The **project** dropdown links the note to a project, and it then appears in that project's panel.
- **New note:** `Ctrl+N` from anywhere, the **+** button, or quick add with `n …`. A new note left empty is discarded when you switch away.
- **Title:** a note's title is its first line, with Markdown symbols stripped.
- **Search:** `Ctrl+F` in the notes view searches note text and project names.
- **Delete:** with Undo.

### Claude assistant

Chat with Claude about your plans (`Ctrl+7`, the ✦ icon in the rail, or `Ctrl+K` → *Ask Claude*). It can see your tasks, events, projects, habits, goals and note titles. **It never changes anything itself:** it proposes changes, and you apply them.

**Setup**

1. Create an API key at console.anthropic.com. API usage is billed by Anthropic to that account, separately from any Claude subscription.
2. Paste the key into the assistant view, or into **Settings → Claude assistant**. It's stored in **Windows Credential Manager** under "CommandCenter", never in the database or backups. If no key is stored, the `ANTHROPIC_API_KEY` environment variable is used.
3. Optional: choose the model in Settings. **Claude Opus 5.5** is the default; **Claude Sonnet 5.5** costs half as much. A new model applies to new chats.

**Using it**

- Ask anything ("plan my day", "what's slipping?", "fit editing time around my deadlines"), or tap a starter.
- Replies stream in as Markdown.
- When Claude suggests changes, they appear as a card. Each line reads plainly (e.g. *Task "Grade ep 9": move to Thu Nov 12*) with Claude's reason underneath. Click **Apply** or **Skip** on each line, or **Apply all** / **Skip all**.
- Applied changes take effect immediately everywhere in the app, and Claude is told what you applied and skipped in your next message.
- Claude can propose these changes:
  - **Tasks:** add (with due date, project and steps), move, rename, re-date, complete, delete
  - **Events:** add, move, delete
  - **Projects:** change status or next action
  - **Notes:** add
  - It can also read a note's full text when it needs to.
- **Stop** cancels a reply in progress. **Retry** resends after an error. **New chat** starts fresh.
- The header shows tokens used this chat and an estimated cost at list prices.
- Chats are not saved: closing the app or starting a new chat clears them.
- The ✦ card on the overview (hidden by default; show it from the dashboard's edit mode or Settings → Dashboard) has a quick "Ask Claude" box.

**What gets sent, and the cost**

- **What's sent:** each message includes a compact text snapshot of your data: today's and upcoming tasks, the next 3 weeks of events, open projects, habits, goals and the 25 most recent note titles. Note bodies are sent only when Claude reads one.
- **Size:** the snapshot is roughly 1,000 tokens with the sample data. It's re-sent only when your data has changed since the last message.
- **Caching:** requests use prompt caching, so the earlier conversation is billed at a tenth of the input price on later turns.
- **Typical cost:** a typical planning exchange on Opus 5.5 costs a few cents.
- **Settings:** Claude Opus 5.5 at effort `medium`, streaming, with Anthropic's server-side refusal fallback on.

**Errors:** a rejected key, an empty balance, rate limits and overload each show a plain message in the chat. Temporary failures are retried automatically (twice) before you see them.

## Command palette and quick add

- **Command palette** (`Ctrl+K`):
  - Fuzzy search over every place and action.
  - **Content search:** matching tasks (title or project), notes (text), projects (name or next action), upcoming events (title or location), and habits and goals (name) are listed under their module. Choosing one opens it.
  - If nothing matches, **Add "…"** sends the text to quick add.
- **Quick add** (`Ctrl+J` in the app, or the global hotkey from any app): one line becomes a task, note or event, with a live preview of how it was read.
- **Global hotkey:** works while Premiere, OBS or a game has focus. It brings the window up with quick add open; `Enter` saves, `Esc` cancels, and the window minimizes again so you land back where you were.

### Quick-add syntax

```text
Color-grade ep 7 !fri #survival     task on today's list, due Friday, linked to "Hardcore Survival S3"
Record door build @tomorrow         task on tomorrow's list
Renew domain !+5                    task due five days from now
Edit ep 8 x3                        task with 3 steps (each check-off = 33%)
n Ideas: world-shrinks challenge    note (also "note …", "n:", "note:")
e Dentist @fri 9:15-10              event Friday 9:15–10:00 (also "event …")
e Thumbnail review 3-4pm            event today 3–4 PM
e Rent due @2026-12-01              all-day event (no time given)
```

| Token | Meaning |
|---|---|
| `!date` | Task due date |
| `@date` | The day a task appears on (default: today), or an event's date |
| `x3` | Split a task into 3 steps (2–99) |
| `#name` | Link a task to a project. Matches the start of the name or any word in it, ignoring case and punctuation (`#ppcurves` → "PP Curves panel") |

- **Dates:** `today`, `tomorrow`/`tmr`, a weekday (`fri`, `friday`; today counts if it matches), `+N` days, or `YYYY-MM-DD`.
- **Event times:** `9:15`, `14:30`, `8pm`, `12:30am`, or ranges like `9-10:30` and `3-4pm`.
- **Left in the title:**
  - A bare number such as the `9` in "ep 9", which is never read as a time.
  - Unrecognised `!words`.

## Keyboard shortcuts

| Shortcut | Action |
|---|---|
| `Ctrl+K` | Command palette (press again to close) |
| `Ctrl+J` | Quick add |
| `Ctrl+Alt+Space` *(default, configurable)* | **Global** quick add, from any app |
| `Ctrl+0` | Overview |
| `Ctrl+1` … `Ctrl+7` | Tasks · Upcoming · Habits & goals · Projects · Spending · Notes · Claude |
| `Ctrl+N` | New note |
| `Ctrl+F` | Search notes (in the notes view) |
| `Ctrl+,` | Settings |
| `Ctrl+Shift+L` | Toggle light / dark |
| `Ctrl+Z` | Undo the last delete while its toast is showing |
| Right-click a time | Copy it as a Discord timestamp (or as text for every zone) |
| Right-click an event | Recolor or delete it, or copy its time |
| `Esc` | Leave a text field → close an overlay → leave dashboard edit mode → back to the overview |
| `↑` `↓` `Enter` | Move and run in the palette |

**Task lists** (select a row by clicking its empty space or with `Tab`):

| Key | Action |
|---|---|
| `↑` `↓` | Previous / next task |
| `Space` | Mark done / not done |
| `Shift+Space` | Undo one step (step tasks) |
| `Enter` | Open / close the detail editor |
| `T` | Move to tomorrow |
| `Delete` | Delete (undoable) |
| `Ctrl+↑` `Ctrl+↓` | Move the task up / down |

Only the global hotkey works when Command Center isn't focused, so the other shortcuts never clash with Premiere's `Ctrl+K` (Razor) or any other app.

### Changing the global hotkey

1. Go to **Settings → Shortcuts**.
2. Type a combination such as `Ctrl+Shift+Alt+Q` and press **Apply**. It takes effect immediately.

- **Modifiers:** `Ctrl`, `Alt`, `Shift`, `Super`.
- **Keys:** letters, digits, `F1`–`F24`, `Space`, and so on.
- **Conflicts:** if another program already owns the combination, a warning appears. Choose one Premiere doesn't use, because a global hotkey takes that keystroke away from every other app.

## Data, backups and sample data

| What | Where |
|---|---|
| Database | `%APPDATA%\CommandCenter\data\command-center.db` |
| Automatic and safety backups | `%APPDATA%\CommandCenter\data\backups\` |
| WebView cache | `%APPDATA%\CommandCenter\data\webview\` |

- **Open the folder:** Settings → Data → **Open folder**.
- **Moving your data:** to keep it somewhere else (a synced drive, for example), set the environment variable `COMMAND_CENTER_DATA` to a folder before starting the app.
- **Format:** the database is a normal SQLite file that any SQLite browser can open.
  - Money is stored as integer cents.
  - Dates are `YYYY-MM-DD`.
  - Timestamps are local `YYYY-MM-DDTHH:MM:SS`.

### Backups

- **Automatic:** once a day (at startup or when the date changes) the app writes `backups\auto-YYYY-MM-DD.ccbackup` and keeps the newest **7**. Settings → Data shows the latest one and opens the folder.
- **Export:** `Ctrl+K` → *Export backup…* or Settings → Data writes a complete, consistent snapshot to a `.ccbackup` file you choose. The file is itself a SQLite database.
- **Restore:** *Restore from backup…* replaces all current data.
  - A safety copy of your current data is written to `backups\pre-restore-<timestamp>.ccbackup` first.
  - Backups from older app versions are upgraded automatically.
- **Pruning:** only `auto-` files are ever removed. Manual exports and safety copies are kept.

Automatic backups live next to the database, on the same disk. For protection against a failed drive, export to (or sync the backups folder to) another disk or a cloud folder now and then.

### Sample data

Every sample row is marked `seed = 1`.

- **Clear** deletes only those rows. Tasks and notes you added keep working; if one was linked to a sample project, it's simply unlinked.
- **Reload** clears the sample rows and inserts a fresh set dated around today.
- Once cleared, sample data never comes back unless you press Reload.

## Troubleshooting

- **Global hotkey shows "already used by another app":** another program has registered that combination. Pick a different one in Settings → Shortcuts.
- **The global hotkey does nothing after closing the window:** closing quits the app, which also stops the hotkey. Minimize the window instead, or start the app with Windows (see [Setup](#setup)).
- **A blank or white window on start:** the WebView2 runtime is missing or broken. Install the Evergreen runtime from Microsoft's WebView2 page.
- **"Could not open the database":** the data folder isn't writable, or the file is locked by another copy of the app that's already running. Close the other copy, or check the folder's permissions.
- **"Database schema … is newer than this app":** the database was opened by a newer build. Update the app; it never downgrades data.

## Architecture

```text
command-center/
  Cargo.toml                    workspace
  crates/
    core/                       cc-core: no UI, fully unit-tested
      src/db.rs                 connection, data dir, settings, migrations runner, LIKE escaping
      src/migrations/*.sql      schema, one file per version (never edit a shipped one)
      src/model.rs              quick-add apply, project lookup, overview summary
      src/model/tasks.rs        task lists, carry-over, reorder, search, undo
      src/model/notes.rs        notes, search, checkbox toggling
      src/model/projects.rs     projects, task-based progress, delete/undo with re-linking
      src/model/events.rs       events, day ranges, multi-day and overnight handling
      src/model/habits.rs       habits, check-ins, daily/weekly streaks and rates
      src/model/goals.rs        goals, percent, pace, needed-per-week
      src/calendar.rs           EventSource trait + sync() for external calendars
      src/tz.rs                 time zone conversion, Discord <t:…> timestamps
      src/update.rs             GitHub Releases lookup and version comparison
      src/ai/mod.rs             assistant: system prompt, tools, data snapshot, applying approved changes
      src/ai/api.rs             Messages API over raw HTTP: request, SSE parsing, errors, retries
      src/markdown.rs           Markdown → safe HTML, titles, snippets
      src/quick_add.rs          quick-add parser (tasks, notes, events, dates, times)
      src/seed.rs               sample data, clear and reload
      src/backup.rs             export, restore, daily automatic backups
    app/                        command-center: the Dioxus desktop app
      assets/style.css          the only stylesheet; theme tokens at the top
      src/main.rs               database bootstrap, window config
      src/window_state.rs       window size/position/maximized persistence, themed title bar, app icon
      src/updates.rs            update check against GitHub Releases, banner
      src/layout.rs             overview layout (order, size, visibility)
      src/store.rs              Store: DB handle + change signal
      src/state.rs              Ui signals, View, Command, Action, toasts with undo
      src/shell/                rail, top bar, grid, dashboard edit mode, palette, quick add, settings, keys, layout editor, time menu
      assets/calendar.css       week/month grid, timezone panel and time-menu styles
      assets/assistant.css      Claude chat, proposal cards
      src/widgets/              one folder per module + REGISTRY
```

**Why there are no loading spinners:** the `Store` reads SQLite synchronously during render. Local queries take microseconds, so there's nothing to wait for. Each read subscribes the component to a revision signal, and each write bumps it, so every view showing data re-renders after a change.

## Adding a widget

Example: a *Video pipeline* widget (script → record → edit → published).

1. **Schema (if it stores data).** Add `crates/core/src/migrations/002_pipeline.sql`:
   ```sql
   CREATE TABLE pipeline_items (
       id INTEGER PRIMARY KEY, title TEXT NOT NULL,
       stage TEXT NOT NULL DEFAULT 'script', project_id INTEGER REFERENCES projects(id) ON DELETE SET NULL,
       position INTEGER NOT NULL DEFAULT 0, created_at TEXT NOT NULL, seed INTEGER NOT NULL DEFAULT 0
   );
   ```
   Append it to `MIGRATIONS` in `crates/core/src/db.rs`. Existing databases are upgraded on the next launch, and automatic backups mean you can always roll back.
2. **Logic.** Put queries in `crates/core/src/model/pipeline.rs`, declare it in `model.rs`, and add tests.
3. **Sample data (optional).** Insert rows with `seed = 1` in `seed.rs` and add the table to `SEED_TABLES`, children first.
4. **UI.** Create `crates/app/src/widgets/pipeline.rs` (or a `pipeline/` folder for bigger widgets):
   ```rust
   use super::WidgetDef;
   use crate::{icons, state::{Action, Command, View}, store::use_store};
   use dioxus::prelude::*;

   pub static DEF: WidgetDef = WidgetDef {
       id: "pipeline",            // stable forever: used in saved settings and layout
       title: "Pipeline",
       icon: icons::PROJECTS,     // or add a new path in icons.rs
       cols: 4, rows: 2,          // default footprint on the 12-column grid
       card, full, commands,
       search: None,              // or Some(fn(&Db, &str) -> Vec<Hit>) for palette content search
       open_item: None,           // or Some(fn(i64)): select that item before the full view opens
       on_message: None,          // or Some(fn(&Store, Ui, &str)): page messages "pipeline:…"
   };

   fn card() -> Element {
       let store = use_store();
       let n = store.read(|db| db.count("SELECT count(*) FROM pipeline_items WHERE stage != 'published'"));
       rsx! { "{n} in progress" }
   }
   fn full() -> Element { rsx! { div { class: "full", "…" } } }
   fn commands() -> Vec<Command> {
       vec![Command::new("Go to", icons::PROJECTS, "Pipeline", Action::Open(View::Widget("pipeline")))]
   }
   ```
5. **Register it.** In `crates/app/src/widgets/mod.rs`, add `pub mod pipeline;` and `&pipeline::DEF` to `REGISTRY`.
   - Its position in the list sets its place in the side rail and its `Ctrl+<number>` shortcut.
   - It's appended to everyone's dashboard automatically, and can be moved, resized or hidden from the dashboard's edit mode or Settings → Dashboard.

How widget code runs:

- `card` and `full` are plain functions that may use hooks; the shell runs each inside its own keyed component.
- Use `store.read(..)` while rendering and `store.write(..)` in event handlers.
- Shared building blocks: `ProgressBar`, `Stat`, `money()`, `ago()`, `tasks::{TaskList, AddTask}`, and toasts with undo (`ui.notify_undo`).
- Palette entries can run any function through `Action::Run(fn(&Store, Ui))`.

## Roadmap and known limitations

1. ~~App shell, layout, data layer, palette, quick add, backup, sample data~~
2. ~~Tasks: list, check off, drag to reorder, due dates, carry-over at midnight~~
3. ~~Quick notes: Markdown editor with autosave, pin, link to project~~
4. ~~Projects: cards, status, next action, progress (manual or from tasks)~~
5. ~~Upcoming events: 7–14 day agenda, manual entry, `EventSource` trait for Google Calendar later~~
6. Spending: manual entry, bank CSV import with column mapping and rules, month comparison, category budgets. *On hold.* The tables (`expenses`, `expense_categories`, `import_profiles`, `category_rules`) and sample data already exist.
7. ~~Habits & goals: daily/weekly habits, streaks, heatmap; goals with target and deadline~~
8. ~~Polish: saved window state, dashboard customization, daily automatic backups, app icon, README~~

Ideas for later: Google Calendar sync, a system-tray mode (so closing the window keeps the hotkey alive), YouTube channel stats, an upload schedule, and a video pipeline widget.

Known limitations:

- **Explorer icon:** the exe file itself shows the default program icon in Explorer. The window and taskbar use the app's own icon.
- **Not tested by hand during development:**
  - drag-to-reorder and `Ctrl+↑/↓` in task lists
  - the `Ctrl+N` and `Ctrl+F` shortcuts
  - opening a link from a note

  The underlying logic is unit-tested. These paths need a real mouse or keyboard, which the automated checks couldn't simulate.
