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

More and more people are vibe-coding — letting Claude write the code while they direct the vision. That's powerful. But it has a gap: **you can ship code you don't understand**.

VibeCheck closes that gap. After Claude finishes a task that changed code, it asks you **one** multiple-choice question about what actually changed. Not a syntax quiz. Not a lecture. A behavioral question: *"What happens when a user does X after this change?"*

- **Diff-grounded** — every question is about the exact code that just changed
- **Behavior-focused** — tests understanding of *what the change does*, not language trivia
- **10 seconds** — one click to answer, brief explanation, done
- **Always skippable** — Yes / No / Snooze 30m / Disable
- **Zero storage** — no scores, no telemetry, no answers saved to disk
- **Works everywhere** — Claude Code CLI + VS Code extension

<!-- TODO: Add demo GIF here showing the quiz flow -->
<!-- ![VibeCheck demo](assets/demo.gif) -->

## Install

```bash
git clone https://github.com/akshan-main/vibe-check.git
bash vibe-check/install/install.sh      # install in current project
```

That's it. Next time Claude finishes a coding task, you'll get a quiz.

<details>
<summary><strong>More install options</strong></summary>

### Global install (all projects)

```bash
bash vibe-check/install/install.sh --global
```

### Skill only (`/vibecheck` command, no auto-trigger)

If you don't want automatic quizzes — just the on-demand `/vibecheck` slash command:

```bash
bash vibe-check/install/install.sh --skill-only
```

### Specific project

```bash
bash vibe-check/install/install.sh /path/to/your/project
```

### Manual setup

1. **Skill only** — copy `templates/project/.claude/skills/vibecheck/` into your project's `.claude/skills/`
2. **Auto-quiz** — additionally:
   - Build the binary: `cargo build --release`
   - Copy `target/release/vibecheck` to `.claude/hooks/vibecheck`
   - Copy `templates/project/.claude/vibecheck.json` to `.claude/vibecheck.json`
   - Add the Stop hook to `.claude/settings.json`:
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

</details>

Works in both **Claude Code CLI** and the **VS Code extension** — they share the same settings files.

## How It Works

### Auto-quiz (default)

```
You: "Add form validation to the signup page"
Claude: *writes code, finishes task*

  ┌─────────────────────────────────────────────────┐
  │  VibeCheck: quick 10-second check?              │
  │                                                 │
  │  ○ Yes (10s)                                    │
  │  ○ No thanks                                    │
  │  ○ Snooze 30m                                   │
  │  ○ Disable                                      │
  └─────────────────────────────────────────────────┘

  ┌─────────────────────────────────────────────────┐
  │  After this change, what happens when a user    │
  │  submits the form with an email that contains   │
  │  no @ symbol?                                   │
  │                                                 │
  │  ○ A) The form submits and saves invalid data   │
  │  ○ B) An inline error appears, form blocked     │
  │  ○ C) The page crashes with unhandled error     │
  │  ○ D) The email field is silently cleared       │
  └─────────────────────────────────────────────────┘

Claude: "Correct! B — the new validation regex rejects
        emails without @, and the error state prevents
        submission. Takeaway: always test validation with
        the simplest invalid input first."
```

Under the hood: a [Claude Code Stop hook](https://docs.anthropic.com/en/docs/claude-code/hooks) checks `git diff`, enforces throttling (1 quiz per 15min max, never the same diff twice), and if conditions pass, asks Claude to generate one behavioral MCQ.

### On-demand (`/vibecheck`)

Type `/vibecheck` anytime in Claude Code to quiz yourself on the current diff. No throttle, no conditions — just the quiz.

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
| `enabled` | `true` | Kill switch for auto-quiz (does not affect `/vibecheck`) |
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

## License

MIT
