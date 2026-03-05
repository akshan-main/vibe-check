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
    <a href="#modes">Modes</a> &middot;
    <a href="#configure">Configure</a> &middot;
    <a href="#ci-gate">CI Gate</a> &middot;
    <a href="#team-mode">Team Mode</a> &middot;
    <a href="#update">Update</a> &middot;
    <a href="#faq">FAQ</a>
  </p>
</p>

---
** Actively in development. Always update. Update instructions are available under update section **

More and more people are vibe coding but barely know what got built. You say "add rate limiting" and your AI does it. But do you know what your users actually see when they hit the limit? A friendly message? A raw 429? Does the page just hang?

VibeCheck asks you stuff like that. One question after your AI finishes a task, based on your actual diff. It looks at what was built, compares it to what you asked for, and checks if you know what changed in your product.

Works with any AI coding tool that uses git. Auto-quiz after every commit with a single setup command, or run on-demand whenever you want.

Skip it every time if you want. No scores. No answers saved. Just a quick reality check.


## Install

<details open>
<summary><strong>Quick install (any tool)</strong></summary>

```bash
cargo install vibe-check
```

Then set up auto-quiz in any project:
```bash
vibecheck init
```

This installs a git `post-commit` hook. After every commit, VibeCheck prints a quiz to your terminal. Works with Cursor, Windsurf, OpenClaw, PicoClaw, NanoClaw, Cline, Aider, or anything that commits to git.

</details>

<details>
<summary><strong>Claude Code (deeper integration)</strong></summary>

```bash
curl -fsSL https://raw.githubusercontent.com/akshan-main/vibe-check/main/install/install.sh | bash
```

This downloads a pre-built binary and wires up a [Stop hook](https://docs.anthropic.com/en/docs/claude-code/hooks) so quizzes trigger automatically after every task - no commit needed. Also installs a `/quiz` slash command for on-demand quizzes. Works in both the CLI and VS Code extension.

More options:
```bash
# Install globally (all projects)
curl -fsSL https://raw.githubusercontent.com/akshan-main/vibe-check/main/install/install.sh | bash -s -- --global

# Only the /quiz command, no auto-trigger
curl -fsSL https://raw.githubusercontent.com/akshan-main/vibe-check/main/install/install.sh | bash -s -- --skill-only
```

[Read the install script](install/install.sh) - it downloads one binary, creates a config file, and registers a hook. Nothing else.

</details>

<details>
<summary><strong>Manual binary download</strong></summary>

Grab the latest binary for your platform from [Releases](https://github.com/akshan-main/vibe-check/releases):

- `vibecheck-darwin-arm64` (macOS Apple Silicon)
- `vibecheck-darwin-x86_64` (macOS Intel)
- `vibecheck-linux-x86_64` (Linux x86_64)
- `vibecheck-linux-arm64` (Linux ARM64)
- `vibecheck-windows-x86_64.exe` (Windows)

Put it somewhere on your PATH, then run `vibecheck init` in any project.

> **Windows users**: The bash installer is macOS and Linux only. Use `cargo install vibe-check` and then `vibecheck init` instead.

</details>

## How It Works

```
You: "Add rate limiting to the API"
AI: *writes code, commits*

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

Answer: B. The rate limiter returns a 429 with no custom
        message. Your users will see a raw error.

        Your prompt said 'add rate limiting' but didn't
        mention what the user should see when they hit it.
        A more complete prompt: 'Add rate limiting and
        return a friendly error with a Retry-After header
        when the limit is hit.'
```

### Auto-quiz after every commit

```bash
vibecheck init
```

Installs a git `post-commit` hook. After every commit, VibeCheck reads the diff and prints quiz context. Works with any AI tool that commits to git.

```bash
vibecheck remove    # uninstall the hook
```

### On-demand quiz

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

### Explain mode

Don't want a quiz? Just get a plain-language walkthrough of what changed:

```bash
vibecheck explain

# Explain latest commit
vibecheck explain --commit

# Pipe to any LLM
vibecheck explain | llm
```

Set `"hookAction": "explain"` in your config to use explain mode as the default for auto-triggers instead of quizzes.

### Claude Code bonus features

Claude Code gets a couple extras because of its [hooks system](https://docs.anthropic.com/en/docs/claude-code/hooks):

- **Auto-quiz without committing**: triggers after every task, not just commits, so you get quizzed even on uncommitted changes
- **`/quiz` slash command**: type `/quiz` anytime for an on-demand quiz without leaving your session
- **Full conversation context**: the quiz knows what you asked for and what got built, so it can compare intent vs. implementation

### Why Rust?

VibeCheck is a single static binary with no runtime dependencies. It starts in under a millisecond as a git hook (Python hooks add 200-500ms to every commit). It runs multiple git commands in parallel using OS threads to collect your diff context fast, even on large repos.

## Modes

Different people need different things. Set your mode and the quiz adapts - trigger sensitivity, question style, and difficulty all change.

```bash
vibecheck mode              # show current mode
vibecheck mode developer    # switch modes
```

| Mode | Who it's for | How it works |
|------|-------------|--------------|
| `vibe_coder` | Flow-first builders | Light questions, casual tone. L1 difficulty (what changed). Default mode. |
| `developer` | Working engineers | Risk-scaled difficulty. Low-risk diffs get L2, high-risk get L3-L4. Verification-focused. |
| `hardcore` | "I don't trust myself at 2am" | Always L4. Failure modes, rollback plans, security implications. High-risk diffs get a follow-up question. |
| `learning` | Leveling up | Adaptive difficulty based on your accuracy. Starts easy, escalates as you prove competence. |

Difficulty is based on **what changed, not how much**. A 2-line auth bug is harder than a 500-line refactor. The risk engine looks at file paths and diff content - auth, payments, migrations, public API, dependencies, infra, concurrency, error handling - and scores accordingly.

Or set mode in config:
```json
{ "mode": "developer" }
```

### Stats and weak-area tracking

Track what categories you struggle with:

```bash
vibecheck stats
```

```
vibecheck stats

Overall: 8/12 correct (67%)
Streak: 3

By category:
  Category               Score   Quizzes
  --------------------------------------
  security                 33%    1/3   <-- weak
  api                      75%    3/4
  general                  80%    4/5
```

Enable tracking in your config:
```json
{ "trackProgress": true }
```

When weak areas are detected (under 60% accuracy with 3+ quizzes), the quiz automatically focuses on building understanding in that area.

## CI Gate

Post a VibeCheck quiz as a PR comment so the author has to think about what they're merging.

```bash
# Generate quiz markdown for a PR
vibecheck ci --base origin/main --head HEAD

# Pipe directly to GitHub CLI
vibecheck ci | gh pr comment --body-file -
```

### GitHub Actions

Copy `templates/ci/vibecheck.yml` to your repo's `.github/workflows/` directory. It will:
1. Install vibecheck
2. Generate a quiz from the PR diff
3. Post it as a PR comment

Or add the steps to your existing workflow:

```yaml
- name: Install vibecheck
  run: |
    curl -fsSL https://raw.githubusercontent.com/akshan-main/vibe-check/main/install/install.sh | sh
    echo "$HOME/.local/bin" >> $GITHUB_PATH

- name: Generate quiz
  id: quiz
  run: |
    vibecheck ci --base origin/${{ github.base_ref }} --head ${{ github.sha }} > quiz.md 2>/dev/null || true
    if [ ! -s quiz.md ]; then echo "skip=true" >> $GITHUB_OUTPUT; fi

- name: Post quiz as PR comment
  if: steps.quiz.outputs.skip != 'true'
  uses: actions/github-script@v7
  with:
    script: |
      const fs = require('fs');
      const body = fs.readFileSync('quiz.md', 'utf8');
      if (!body.trim()) return;
      await github.rest.issues.createComment({
        owner: context.repo.owner,
        repo: context.repo.repo,
        issue_number: context.issue.number,
        body: body
      });
```

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

Create or edit `.claude/vibecheck.json` in your project root (or `~/.claude/vibecheck.json` for global). This path is used regardless of which AI tool you use:

```json
{
  "enabled": true,
  "mode": "vibe_coder",
  "minSecondsBetweenQuizzes": 900,
  "maxDiffChars": 2000,
  "trackProgress": false,
  "hookAction": "quiz"
}
```

| Field | Default | Description |
|-------|---------|-------------|
| `enabled` | `true` | Kill switch for auto-quiz (does not affect `/quiz` or `vibecheck quiz`) |
| `mode` | `"vibe_coder"` | Quiz mode: `vibe_coder`, `developer`, `hardcore`, `learning` |
| `minSecondsBetweenQuizzes` | `900` | Minimum seconds between auto-quizzes |
| `maxDiffChars` | `2000` | Max diff characters sent as quiz context |
| `difficulty` | auto | Override difficulty: `"L1"`, `"L2"`, `"L3"`, `"L4"`. Normally set by mode + risk. |
| `trackProgress` | `false` | Track quiz stats locally and on the team leaderboard. Enables weak-area detection. |
| `hookAction` | `"quiz"` | What the auto-trigger does: `"quiz"` or `"explain"` |

## Update

Your `vibecheck.json` config is never overwritten.

<details>
<summary><strong>Cargo (any tool)</strong></summary>

```bash
cargo install vibe-check --force
```

Updates the binary on your PATH. If you set up auto-quiz with `vibecheck init`, the git post-commit hook automatically picks up the new version - no re-setup needed.

</details>

<details>
<summary><strong>Claude Code (installer)</strong></summary>

```bash
curl -fsSL https://raw.githubusercontent.com/akshan-main/vibe-check/main/install/install.sh | bash -s -- --update
```

What gets updated:
- The `vibecheck` binary in `.claude/hooks/`
- The `/quiz` skill prompt (`SKILL.md`) - asks before overwriting if you customized it

What stays the same:
- Your `vibecheck.json` config (frequency, difficulty, etc.)
- Your `settings.json` hook setup
- Your `.vibecheck/` state (quiz timestamps)

</details>

<details>
<summary><strong>Manual binary download</strong></summary>

Grab the latest binary for your platform from the [Releases](https://github.com/akshan-main/vibe-check/releases) page and replace the old one wherever you put it.

</details>

## Uninstall

**Standalone CLI (git hook)**
```bash
vibecheck remove
```

**Cargo**
```bash
cargo uninstall vibe-check
```

**Claude Code (installer)**
```bash
curl -fsSL https://raw.githubusercontent.com/akshan-main/vibe-check/main/install/install.sh | bash -s -- --uninstall
```

## FAQ

<details>
<summary><strong>Can't I just ask my AI about my code?</strong></summary>

You can. You won't. VibeCheck is proactive - it catches the gaps you didn't know to ask about. And it compares your original prompt to what was actually built, so you know exactly where the disconnect was.
</details>

<details>
<summary><strong>How is this different from learning mode?</strong></summary>

Learning mode teaches you how the code works. VibeCheck checks if you know what your product does. "What does the user see when they hit the rate limit?" vs "here's how the middleware pipeline works."
</details>

<details>
<summary><strong>Will it loop forever?</strong></summary>

No. The quiz outputs a `[vibecheck:done]` sentinel that prevents re-triggering. In Claude Code, the Stop hook also checks `stop_hook_active` to avoid loops.
</details>

<details>
<summary><strong>Does it work with Cursor/Windsurf/OpenClaw/Cline/Aider?</strong></summary>

Yes. Run `vibecheck init` to auto-quiz after every commit, or `vibecheck quiz` for on-demand quizzes. Works with anything that uses git.
</details>

<details>
<summary><strong>Can I change the frequency?</strong></summary>

Yes. Set `minSecondsBetweenQuizzes` in `vibecheck.json`. Default is 900 (15 minutes). Set to `60` for every minute, `3600` for hourly.
</details>

<details>
<summary><strong>Does it store my answers?</strong></summary>

By default, no. A small state file (`.claude/.vibecheck/state.json`) tracks the last quiz timestamp for throttling. If you enable `trackProgress` in your config, scores are stored locally in that same state file, per-category accuracy goes in `categories.json`, and stats optionally sync to your team leaderboard.
</details>

<details>
<summary><strong>Does the quiz affect what my AI does next?</strong></summary>

No. The quiz runs after the task is done. Quiz answers don't influence anything.
</details>

<details>
<summary><strong>I only want on-demand quizzes, not auto-trigger.</strong></summary>

Just don't run `vibecheck init`. Use `vibecheck quiz` whenever you want. For Claude Code users: use `--skill-only` during install, or set `"enabled": false` in `vibecheck.json`.
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

## Security and Privacy

VibeCheck runs entirely on your machine. It never phones home, sends telemetry, or talks to any server.

**What it reads:**
- Your git diff (via `git diff` and `git show`)
- Your config at `.claude/vibecheck.json` or `~/.claude/vibecheck.json`

**What it writes:**
- `.claude/.vibecheck/state.json` - last quiz timestamp, diff hash, and (if `trackProgress` is on) your score
- `.vibecheck-team/members/<hash>.json` - if team mode is active, stores your display name, a SHA-256 hash of your git email (not the email itself), and quiz stats

Nothing is sent over the network. The install script downloads the binary from GitHub Releases (with SHA-256 checksum verification) and that is the only network call VibeCheck ever makes.

## Limitations

- **Untracked files are invisible.** VibeCheck uses `git diff`, which only sees tracked files. New files that haven't been `git add`-ed won't appear in the quiz.
- **Large diffs get truncated.** The `maxDiffChars` setting (default 2000) caps how much diff context is sent. If your change is bigger than that, the quiz only covers the first portion.
- **Bash installer is macOS/Linux only.** Windows users should use `cargo install vibe-check` instead.
- **Pattern detection is keyword-based.** The security, error handling, and API change flags use simple string matching, not AST parsing. They may flag false positives (e.g., a variable named `error_count`) or miss changes that don't use common keywords.

## Diagnose

If something isn't working, run:

```bash
vibecheck doctor
```

This prints your config path, git repo status, team mode status, post-commit hook status, and current config values.

## Contributing

PRs welcome. Keep it simple. One question, one diff, skip anytime.
