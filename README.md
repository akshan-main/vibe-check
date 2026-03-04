<p align="center">
  <h1 align="center">VibeCheck</h1>
  <p align="center">
    <strong>Do you actually understand what you just shipped?</strong>
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

More and more people are vibe-coding, letting coding agents like Claude Code write the code while they direct it. That's cool. But it has a gap: **you "might" ship code you don't understand**.

VibeCheck helps narrow that gap. After Claude finishes a task that changed code, it asks you **one** multiple-choice question about what actually changed. Not a syntax quiz. Not a lecture. A behavioral question: *"What happens when a user does X after this change?"*

The main goal of this is, it improves your prompting skills, you can incorporate what it asks next time you prompt for the feature since you have a better understanding of what a llm expects.

- **Diff-grounded** - every question is about the exact code that just changed
- **Behavior-focused** - tests understanding of *what the change does*, not language trivia
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

        If that's not the experience you want, you should
        tell Claude to add a friendly error response with
        a Retry-After header so the client knows when to
        try again."
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

