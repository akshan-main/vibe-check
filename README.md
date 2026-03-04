<p align="center">
  <h1 align="center">VibeCheck</h1>
  <p align="center">
    <strong>Become a better prompter, one diff at a time.</strong>
  </p>
  <p align="center">
    One question. Based on your exact diff. 10 seconds. Always skippable.
  </p>
  <p align="center">
    <a href="#install">Install</a> &middot;
    <a href="#how-it-works">How It Works</a> &middot;
    <a href="#configure">Configure</a> &middot;
    <a href="#faq">FAQ</a>
  </p>
</p>

---

Every time Claude Code finishes a task, VibeCheck asks you **one** quick question about what just changed in your product — and then tells you **how to prompt better next time**.

You prompted *"Add rate limiting."* Claude built it. But did it return a friendly error or a raw 429? Is there a Retry-After header? VibeCheck surfaces exactly these gaps — then suggests a better prompt: *"Next time, try: 'Add rate limiting AND return a friendly error with a Retry-After header.'"*

Over time, your prompts get more complete. Fewer surprises. Better products.

- **Improves your prompting** - every question ends with a concrete prompting tip you can use next time
- **Product-focused** - asks what your *app does*, not how the code works
- **AI-picked** - uses Claude's understanding of your code to find the most important change, not the biggest diff
- **10 seconds** - one click to answer, brief explanation, done
- **Always skippable** - Yes / No / Snooze 30m / Disable
- **Zero storage** - no scores, no telemetry, no answers saved to disk

<!-- TODO: Add demo GIF here showing the quiz flow -->
<!-- ![VibeCheck demo](assets/demo.gif) -->

## Install

Pick what works for you. All three methods give you the same result.

### Option A: Install script

```bash
git clone https://github.com/akshan-main/vibe-check.git
bash vibe-check/install/install.sh      # install in current project
```

The script is [~100 lines of bash](install/install.sh) — it copies a binary, a config file, and merges one entry into your `settings.json`. You can read the whole thing in 2 minutes.

<details>
<summary>More script options</summary>

```bash
bash vibe-check/install/install.sh --global          # all projects
bash vibe-check/install/install.sh --skill-only      # only /quiz, no auto-trigger
bash vibe-check/install/install.sh /path/to/project  # specific project
```

</details>

### Option B: Manual setup (no script)

**For just the `/quiz` command** (on-demand only):

Copy the skill folder into your project:
```bash
git clone https://github.com/akshan-main/vibe-check.git
cp -r vibe-check/templates/project/.claude/skills/quiz .claude/skills/quiz
```

Done. Type `/quiz` in Claude Code anytime.

**For auto-quiz after every task**, additionally:

1. Build or download the binary:
   ```bash
   cd vibe-check && cargo build --release
   ```

2. Copy the binary and config into your project:
   ```bash
   mkdir -p .claude/hooks
   cp vibe-check/target/release/vibecheck .claude/hooks/vibecheck
   cp vibe-check/templates/project/.claude/vibecheck.json .claude/vibecheck.json
   ```

3. Add the Stop hook to `.claude/settings.json` (create the file if it doesn't exist):
   ```json
   {
     "hooks": {
       "Stop": [
         {
           "hooks": [
             {
               "type": "command",
               "command": "$CLAUDE_PROJECT_DIR/.claude/hooks/vibecheck",
               "timeout": 5
             }
           ]
         }
       ]
     }
   }
   ```

   If you already have a `settings.json`, just add the Stop hook entry alongside your existing config.

### Option C: Global install (all projects)

```bash
bash vibe-check/install/install.sh --global
```

Or manually: follow Option B but place everything under `~/.claude/` instead of `.claude/`, and use `$HOME/.claude/hooks/vibecheck` as the hook command.

---

Works in both **Claude Code CLI** and the **VS Code extension** — they share the same settings files.

## How It Works

### Auto-quiz (default)

```
You: "Add rate limiting to the API"
Claude: *writes code, finishes task*

  ┌─────────────────────────────────────────────────┐
  │  VibeCheck: quick 10-second check?              │
  │                                                 │
  │  ○ Yes (10s)                                    │
  │  ○ No thanks                                    │
  │  ○ Snooze 30m                                   │
  │  ○ Disable                                      │
  └─────────────────────────────────────────────────┘

  ┌─────────────────────────────────────────────────────┐
  │  You just added rate limiting. When a user hits     │
  │  the limit, what do they actually see?              │
  │                                                     │
  │  ○ A) A friendly "slow down" message with a         │
  │       retry timer                                   │
  │  ○ B) A raw "429 Too Many Requests" error with      │
  │       no explanation                                │
  │  ○ C) The page just hangs until the limit resets    │
  │  ○ D) They get redirected to the homepage           │
  └─────────────────────────────────────────────────────┘

Claude: "B — the rate limiter returns a 429 status code
        with no custom message or retry-after header.
        Your users will see a raw error.

        Prompting tip: Next time, try: 'Add rate limiting
        AND return a friendly error message with a
        Retry-After header when the limit is hit.' This
        way Claude handles the UX in the same pass."
```

Under the hood: a [Claude Code Stop hook](https://docs.anthropic.com/en/docs/claude-code/hooks) checks `git diff`, figures out what feature was added/changed/removed, and asks one product comprehension question.

### On-demand (`/quiz`)

Type `/quiz` anytime in Claude Code to quiz yourself on the current diff. No throttle, no conditions — just the quiz.

## Configure

Edit `.claude/vibecheck.json` in your project (or `~/.claude/vibecheck.json` for global):

```json
{
  "enabled": true,
  "minSecondsBetweenQuizzes": 900,
  "maxDiffChars": 2000
}
```

| Field | Default | Description |
|-------|---------|-------------|
| `enabled` | `true` | Kill switch for auto-quiz (does not affect `/quiz`) |
| `minSecondsBetweenQuizzes` | `900` | Minimum seconds between auto-quizzes |
| `maxDiffChars` | `2000` | Max diff characters sent as quiz context |

## Uninstall

```bash
bash vibe-check/install/install.sh --uninstall
# or for global:
bash vibe-check/install/install.sh --global --uninstall
```

## FAQ

<details>
<summary><strong>Can't I just ask Claude about my code?</strong></summary>

You can. You won't. VibeCheck is proactive — it catches gaps you didn't know to ask about. More importantly, it gives you a prompting tip so your *next* prompt is better. Asking Claude "what did you just do?" teaches you about this change. VibeCheck teaches you how to prompt so the next change doesn't have the same gaps.
</details>

<details>
<summary><strong>How is this different from Claude's learning mode?</strong></summary>

Learning mode explains *how the code works* while Claude writes it. VibeCheck does something different: it checks if you understand what your *product does* after the change, and gives you a better prompt for next time. "What does the user see when they hit the rate limit?" vs "here's how the middleware pipeline works."
</details>

<details>
<summary><strong>Will it loop forever?</strong></summary>

No. Claude Code sets `stop_hook_active=true` on re-entry, and the quiz ends with a `[vibecheck:done]` sentinel. Both prevent re-triggering.
</details>

<details>
<summary><strong>Does it work in VS Code?</strong></summary>

Yes. The VS Code extension reads the same `.claude/settings.json` and `~/.claude/settings.json` files as the CLI.
</details>

<details>
<summary><strong>Can I change the frequency?</strong></summary>

Yes. Set `minSecondsBetweenQuizzes` in `vibecheck.json`. Default is 900 (15 minutes). Set to `60` for every minute, `3600` for hourly.
</details>

<details>
<summary><strong>What if I already have Stop hooks?</strong></summary>

The installer merges — it adds VibeCheck alongside your existing hooks, never overwrites.
</details>

<details>
<summary><strong>Does it store my answers?</strong></summary>

No. Answers exist only in the chat session transcript. No scores, no telemetry, no files written to disk.
</details>

<details>
<summary><strong>Does the quiz affect what Claude does next?</strong></summary>

No. The quiz runs after the main task is fully complete. The instruction explicitly tells Claude not to let quiz answers influence behavior.
</details>

<details>
<summary><strong>I only want on-demand quizzes, not auto-trigger.</strong></summary>

Use `--skill-only` during install. Or set `"enabled": false` in `vibecheck.json` to disable auto-trigger while keeping the hook installed.
</details>

## Build from Source

```bash
cargo build --release
# Binary at target/release/vibecheck
```

## Contributing

PRs welcome. Keep it simple — the core product is one question, one diff, 10 seconds.

