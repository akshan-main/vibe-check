<p align="center">
  <h1 align="center">VibeCheck</h1>
  <p align="center">
    <strong>Do you actually know what your app just did?</strong>
  </p>
  <p align="center">
    One question. Your exact diff. Skip anytime.
  </p>
  <p align="center">
    Works with Claude Code, Cursor, Windsurf, OpenClaw, PicoClaw, NanoClaw, Cline, Aider, and anything that uses git.
  </p>
  <p align="center">
    <a href="#install">Install</a> &middot;
    <a href="#how-it-works">How It Works</a> &middot;
    <a href="#works-with-any-ai-tool">Any AI Tool</a> &middot;
    <a href="#configure">Configure</a> &middot;
    <a href="#faq">FAQ</a>
  </p>
</p>

---

More and more people are vibe coding but barely know what got built. You say "add rate limiting" and your AI does it. But do you know what your users actually see when they hit the limit? A friendly message? A raw 429? Does the page just hang?

VibeCheck asks you stuff like that. One question after your AI finishes a task, based on your actual diff. It looks at what was built, compares it to what you asked for, and checks if you know what changed in your product.

Works with any AI coding tool. Native integration with Claude Code (auto-quiz after every task), and a standalone CLI that works with Cursor, Windsurf, OpenClaw, PicoClaw, NanoClaw, Cline, Aider, or anything else that writes code and commits to git.

Skip it every time if you want. No scores. No answers saved. Just a quick reality check.

<!-- TODO: Add demo GIF here showing the quiz flow -->
<!-- ![VibeCheck demo](assets/demo.gif) -->

## Install

One command. No dependencies. Downloads a pre-built binary and wires everything up.

```bash
curl -fsSL https://raw.githubusercontent.com/akshan-main/vibe-check/main/install/install.sh | bash
```

That's it. Works in both Claude Code CLI and VS Code extension.

<details>
<summary>More options</summary>

```bash
# Install globally (all projects)
curl -fsSL https://raw.githubusercontent.com/akshan-main/vibe-check/main/install/install.sh | bash -s -- --global

# Only the /quiz command, no auto-trigger
curl -fsSL https://raw.githubusercontent.com/akshan-main/vibe-check/main/install/install.sh | bash -s -- --skill-only

# Uninstall
curl -fsSL https://raw.githubusercontent.com/akshan-main/vibe-check/main/install/install.sh | bash -s -- --uninstall

# Or if you have Rust: cargo install vibe-check
```

</details>

[Read the install script](install/install.sh) - it downloads one binary, creates a config file, and adds one entry to your Claude Code settings. Nothing else.

## How It Works

### Auto-quiz (default)

```
You: "Add rate limiting to the API"
Claude: *writes code, finishes task*

  ┌─────────────────────────────────────────────────┐
  │  VibeCheck: quick check on what just changed?    │
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

Under the hood: a [Claude Code Stop hook](https://docs.anthropic.com/en/docs/claude-code/hooks) reads your git diff and asks one question about what your product actually does now. Since the quiz runs inside your Claude Code session, it already has the full conversation context - it knows what you asked for and what got built.

### On-demand (`/quiz`)

Type `/quiz` anytime in Claude Code to quiz yourself on the current diff.

### Standalone CLI

Run `vibecheck quiz` from any terminal. Works with any AI tool. See [Works with Any AI Tool](#works-with-any-ai-tool) for details.

## Works with Any AI Tool

VibeCheck isn't locked to Claude Code. The `vibecheck` binary is a standalone CLI that works anywhere.

### Standalone quiz (any editor)

```bash
# Quiz yourself on uncommitted changes
vibecheck quiz

# Quiz on your latest commit
vibecheck quiz --commit

# Copy quiz context to clipboard, paste into any AI chat
vibecheck quiz | pbcopy

# Pipe directly to an LLM CLI
vibecheck quiz | llm
```

The `quiz` command reads your git diff, runs diff analysis (function detection, pattern matching, change summary), and outputs structured quiz context. Paste it into whatever AI tool you use.

### Auto-quiz after every commit (any editor)

```bash
vibecheck init
```

This installs a git `post-commit` hook. After every commit, VibeCheck prints quiz context to your terminal. Works with Cursor, Windsurf, OpenClaw, PicoClaw, NanoClaw, Cline, Aider, or any tool that commits to git.

```bash
vibecheck remove    # uninstall the hook
```

### Why Rust?

VibeCheck is a single static binary with no runtime dependencies to install. It starts in under a millisecond as a git hook (Python hooks add 200-500ms to every commit). It runs multiple git commands in parallel using OS threads to collect your diff context fast, even on large repos.

## Team Mode

Track your team's product understanding with a shared leaderboard. No server needed - stats sync through git.

```bash
# One person sets it up
vibecheck team init --name "Your Team"

# Each team member joins
vibecheck team join

# View the leaderboard anytime
vibecheck team
```

```
Your Team
============================================

 #   Name           Score   Streak   This Week
 -----------------------------------------------
 1   Milan           80%        5         4/5
 2   Sara            75%        2         3/4
 3   Mike            60%        0         2/3
 -----------------------------------------------
 Team average: 72%  |  12 quizzes this week
```

How it works:
- Stats stored as JSON files in `.vibecheck-team/` at your project root
- Each member identified by a hash of their git email (privacy-friendly)
- Commit the directory to git so the team can see each other's progress
- Weekly stats reset automatically
- When team mode is active and `trackProgress` is enabled, the quiz automatically updates both personal and team stats

```bash
vibecheck team reset    # reset your own stats
```

## Configure

Edit `.claude/vibecheck.json` in your project (or `~/.claude/vibecheck.json` for global):

```json
{
  "enabled": true,
  "minSecondsBetweenQuizzes": 900,
  "maxDiffChars": 2000,
  "difficulty": "normal",
  "trackProgress": false
}
```

| Field | Default | Description |
|-------|---------|-------------|
| `enabled` | `true` | Kill switch for auto-quiz (does not affect `/quiz`) |
| `minSecondsBetweenQuizzes` | `900` | Minimum seconds between auto-quizzes |
| `maxDiffChars` | `2000` | Max diff characters sent as quiz context |
| `difficulty` | `"normal"` | `"beginner"` for obvious changes, `"advanced"` for edge cases and subtle behavior |
| `trackProgress` | `false` | Set to `true` to track quiz stats locally and on the team leaderboard |

## Uninstall

```bash
curl -fsSL https://raw.githubusercontent.com/akshan-main/vibe-check/main/install/install.sh | bash -s -- --uninstall
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
<summary><strong>Does it work with Cursor/Windsurf/OpenClaw/other AI tools?</strong></summary>

Yes. Run `vibecheck quiz` from any terminal, or `vibecheck init` to auto-quiz after every commit. The standalone CLI works with anything that uses git. Claude Code gets the deepest integration (auto-quiz via Stop hooks), but the core quiz works everywhere.
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

Test it:
```bash
./target/release/vibecheck --help
./target/release/vibecheck quiz
```

## Contributing

PRs welcome. Keep it simple. One question, one diff, skip anytime.
