<p align="center">
  <h1 align="center">VibeCheck</h1>
  <p align="center">
    <strong>Do you actually know what your app just did?</strong>
  </p>
  <p align="center">
    One question. Your exact diff. 10 seconds. Skip anytime.
  </p>
  <p align="center">
    <a href="#install">Install</a> &middot;
    <a href="#how-it-works">How It Works</a> &middot;
    <a href="#configure">Configure</a> &middot;
    <a href="#faq">FAQ</a>
  </p>
</p>

---

You tell Claude to add rate limiting. It does. But do you know what your users actually see when they hit the limit? A friendly message? A raw 429? Does the page just hang?

VibeCheck asks you stuff like that. One question after Claude finishes a task, based on your actual diff. It reads your original prompt, looks at what was built, and asks if you know what changed in your product. Not code trivia. Not "what does this function do." Just: do you understand what your app does now?

If your prompt was vague and the implementation has gaps you didn't think about, it'll point that out. If your prompt already covered everything, it'll tell you that too.

You can skip it every time. No scores. No answers saved. It's just a quick reality check. (A small local state file tracks when the last quiz ran so it doesn't over-trigger.)

<!-- TODO: Add demo GIF here showing the quiz flow -->
<!-- ![VibeCheck demo](assets/demo.gif) -->

## Install

Three ways to set it up. Pick whatever you're comfortable with.

### Option A: Install script

```bash
git clone https://github.com/akshan-main/vibe-check.git
bash vibe-check/install/install.sh      # install in current project
```

The script copies a binary, a config file, and adds one entry to your `settings.json`. [Read it yourself](install/install.sh) if you want.

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

Works in both **Claude Code CLI** and the **VS Code extension** since they share the same settings files.

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

Claude: "B. The rate limiter returns a 429 with no custom
        message. Your users will see a raw error.

        Your prompt said 'add rate limiting' but didn't
        mention what the user should see when they hit it.
        A more complete prompt: 'Add rate limiting and
        return a friendly error with a Retry-After header
        when the limit is hit.'"
```

Under the hood: a [Claude Code Stop hook](https://docs.anthropic.com/en/docs/claude-code/hooks) reads your git diff, reads your original prompt from the session transcript, and asks one question about what your product actually does now.

### On-demand (`/quiz`)

Type `/quiz` anytime in Claude Code to quiz yourself on the current diff.

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

You can. You won't. VibeCheck is proactive - it catches the gaps you didn't know to ask about. And it compares your original prompt to what was actually built, so you know exactly where the disconnect was.
</details>

<details>
<summary><strong>How is this different from learning mode?</strong></summary>

Learning mode teaches you how the code works. VibeCheck checks if you know what your product does. "What does the user see when they hit the rate limit?" vs "here's how the middleware pipeline works."
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

The installer merges. It adds VibeCheck alongside your existing hooks, never overwrites.
</details>

<details>
<summary><strong>Does it store my answers?</strong></summary>

No answers or scores are stored. A small state file (`.claude/.vibecheck/state.json`) tracks the last quiz timestamp for throttling, but nothing about your responses.
</details>

<details>
<summary><strong>Does the quiz affect what Claude does next?</strong></summary>

No. The quiz runs after the main task is done. Quiz answers don't influence anything.
</details>

<details>
<summary><strong>I only want on-demand quizzes, not auto-trigger.</strong></summary>

Use `--skill-only` during install. Or set `"enabled": false` in `vibecheck.json`.
</details>

## Build from Source

```bash
cargo build --release
# Binary at target/release/vibecheck
```

## Contributing

PRs welcome. Keep it simple. One question, one diff, 10 seconds.
