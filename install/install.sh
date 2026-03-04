#!/usr/bin/env bash
set -euo pipefail

# VibeCheck installer - works standalone (curl pipe) or from repo clone
# curl -fsSL https://raw.githubusercontent.com/akshan-main/vibe-check/main/install/install.sh | bash

VERSION="0.2.0"
REPO="akshan-main/vibe-check"
RAW_URL="https://raw.githubusercontent.com/${REPO}/main"
BINARY_NAME="vibecheck"
CONFIG_NAME="vibecheck.json"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

info()  { echo -e "${BLUE}[vibecheck]${NC} $*"; }
ok()    { echo -e "${GREEN}[vibecheck]${NC} $*"; }
warn()  { echo -e "${YELLOW}[vibecheck]${NC} $*"; }
error() { echo -e "${RED}[vibecheck]${NC} $*" >&2; }

usage() {
    cat <<'EOF'
Usage: install.sh [OPTIONS] [TARGET_DIR]

Install VibeCheck into a Claude Code project.

Arguments:
  TARGET_DIR    Project directory (default: current directory)

Options:
  --global      Install to ~/.claude/ (applies to all projects)
  --skill-only  Only install the /quiz slash command (no auto-trigger hook)
  --uninstall   Remove VibeCheck
  --help        Show this help message

One-liner install:
  curl -fsSL https://raw.githubusercontent.com/akshan-main/vibe-check/main/install/install.sh | bash

Examples:
  bash install.sh                     # Install in current project
  bash install.sh --global            # Install globally (all projects)
  bash install.sh --skill-only        # Only /quiz command, no auto-quiz
  bash install.sh --uninstall         # Remove VibeCheck
EOF
}

# ---- Argument parsing ----
GLOBAL=false
UNINSTALL=false
SKILL_ONLY=false
TARGET_DIR="."

while [[ $# -gt 0 ]]; do
    case "$1" in
        --global)     GLOBAL=true; shift ;;
        --uninstall)  UNINSTALL=true; shift ;;
        --skill-only) SKILL_ONLY=true; shift ;;
        --help|-h)    usage; exit 0 ;;
        -*)           error "Unknown option: $1"; usage; exit 1 ;;
        *)            TARGET_DIR="$1"; shift ;;
    esac
done

TARGET_DIR="$(cd "$TARGET_DIR" && pwd)"

if $GLOBAL; then
    CLAUDE_DIR="$HOME/.claude"
else
    CLAUDE_DIR="$TARGET_DIR/.claude"
fi

HOOKS_DIR="$CLAUDE_DIR/hooks"
SETTINGS_FILE="$CLAUDE_DIR/settings.json"
STATE_DIR="$CLAUDE_DIR/.vibecheck"

# ---- Detect platform ----
detect_platform() {
    local os arch
    os="$(uname -s)"
    arch="$(uname -m)"

    case "$os" in
        Darwin) os="darwin" ;;
        Linux)  os="linux" ;;
        *)      error "Unsupported OS: $os. Use 'cargo install vibecheck' instead."; exit 1 ;;
    esac

    case "$arch" in
        x86_64|amd64)  arch="x86_64" ;;
        arm64|aarch64) arch="arm64" ;;
        *)             error "Unsupported architecture: $arch. Use 'cargo install vibecheck' instead."; exit 1 ;;
    esac

    echo "${os}-${arch}"
}

# ---- Download binary ----
download_binary() {
    local platform="$1"
    local dest="$2"
    local url="https://github.com/${REPO}/releases/latest/download/vibecheck-${platform}"

    info "Downloading vibecheck for ${platform}..."
    if curl -fsSL --retry 3 -o "$dest" "$url" 2>/dev/null; then
        chmod +x "$dest"
        ok "Binary downloaded."
        return 0
    fi

    warn "Download failed."

    # Try building from source if cargo is available
    if command -v cargo &>/dev/null; then
        info "Trying cargo install..."
        cargo install vibecheck 2>/dev/null && return 0
    fi

    error "Could not download or build vibecheck."
    error "Try: cargo install vibecheck"
    exit 1
}

# ---- Embedded config (no repo clone needed) ----
write_default_config() {
    local dest="$1"
    cat > "$dest" <<'JSONEOF'
{
  "enabled": true,
  "minSecondsBetweenQuizzes": 900,
  "maxDiffChars": 2000,
  "difficulty": "normal",
  "trackProgress": false
}
JSONEOF
}

# ---- Embedded skill (no repo clone needed) ----
write_skill() {
    local dest="$1"
    cat > "$dest" <<'SKILLEOF'
---
name: quiz
description: On-demand product comprehension quiz about your current code changes
---

Run a VibeCheck quiz right now based on the current git diff.

STEP 1: Run these git commands to gather context:
- `git diff --name-only` and `git diff --staged --name-only` to get changed files
- `git diff --unified=3` and `git diff --staged --unified=3` to get the diff (truncate to 2000 chars if very long)

If there are no changes, say "No code changes to quiz on. Make some changes first!" and stop.

STEP 2: Analyze the diff and create ONE multiple-choice question.

IMPORTANT: Focus on the MOST IMPORTANT change, not the largest one. A 2-line behavior change can matter more than 50 lines of boilerplate. Read the entire diff, use your understanding of the code and the product, and pick the single change that has the most meaningful impact on what users experience. Ignore formatting, imports, renaming, and refactors that don't change behavior.

Classify what happened:
- Was a feature ADDED? (new capability that didn't exist before)
- Was a feature CHANGED? (existing behavior now works differently)
- Was something REMOVED? (capability or safeguard that's now gone)
- Was it a FIX? (broken thing that now works)

Then ask a question that tests whether the developer understands the REAL-WORLD IMPACT of this specific change on their product. Vibe coders build products - they need to understand what their product does, not how to read code.

QUESTION FORMULA - pick one:
  * WHAT CHANGED: "Before this change, [X happened]. What happens now instead?"
  * WHAT'S NEW: "A user tries [action] for the first time. What do they experience?"
  * WHAT'S GONE: "You removed [feature/check/step]. What can users do now that they couldn't before - or what protection is no longer there?"
  * SIDE EFFECTS: "This change also affects [related area]. What's different there now?"
  * EDGE CASE: "A user does [unusual but realistic action]. How does your app handle it after this change?"
  * LIMITS: "What's the maximum/minimum [value/count/size] this feature now supports? What happens at the boundary?"
  * DATA: "After this change, what new data is being stored/sent/exposed? Who can see it?"

NEVER ASK:
  * About code syntax, language features, or programming concepts
  * About which library or framework is used
  * Anything a developer would need to read code to answer - the question should be answerable by someone who understands the PRODUCT but not the code
  * Generic questions unrelated to this specific diff

WRONG ANSWERS: Each should be a plausible misunderstanding of what the product change does. Things a developer might assume if they didn't pay attention to what Claude actually built.

Format: exactly 4 options (labels "A", "B", "C", "D"), one correct. Ask via AskUserQuestion with header: "VibeCheck", multiSelect: false.

STEP 3: After the user answers:
1. Explain the correct answer in plain language - what the product does now and why
2. If wrong: explain what they misunderstood about the change and what their answer would have meant for users
3. PROMPTING TIP: You have the full conversation context - you know what the user asked for and what you built. Compare those. If their prompt was vague and the implementation has gaps or surprises they might not expect, suggest a more specific prompt that would have covered those gaps. If their prompt was already detailed and the implementation matches well, say so. Don't fabricate issues.

End your message with: [vibecheck:done]

IMPORTANT: Do NOT use Edit, Write, or any code-modifying tools. This is learning-only.
SKILLEOF
}

# ---- Merge settings.json ----
merge_settings() {
    local settings_file="$1"
    local hook_command="$2"

    if [[ ! -f "$settings_file" ]]; then
        echo '{}' > "$settings_file"
    fi

    cp "$settings_file" "${settings_file}.bak.$(date +%s)"

    if command -v python3 &>/dev/null; then
        merge_settings_python "$settings_file" "$hook_command"
    elif command -v jq &>/dev/null; then
        merge_settings_jq "$settings_file" "$hook_command"
    else
        error "Neither python3 nor jq found. Add the hook manually to $settings_file"
        cat <<EOF

Add this to your $settings_file under "hooks.Stop":

{
  "hooks": [
    {
      "type": "command",
      "command": "$hook_command",
      "timeout": 5
    }
  ]
}
EOF
        exit 1
    fi
}

merge_settings_python() {
    local settings_file="$1"
    local hook_command="$2"

    python3 - "$settings_file" "$hook_command" <<'PYEOF'
import json, sys

settings_file = sys.argv[1]
hook_command = sys.argv[2]

try:
    with open(settings_file, 'r') as f:
        data = json.load(f)
except (json.JSONDecodeError, FileNotFoundError):
    data = {}

if not isinstance(data, dict):
    data = {}

data.setdefault("hooks", {})
data["hooks"].setdefault("Stop", [])

for entry in data["hooks"]["Stop"]:
    for hook in entry.get("hooks", []):
        if "vibecheck" in hook.get("command", ""):
            print("Already installed - skipping settings merge.")
            sys.exit(0)

new_entry = {
    "hooks": [
        {
            "type": "command",
            "command": hook_command,
            "timeout": 5
        }
    ]
}
data["hooks"]["Stop"].append(new_entry)

with open(settings_file, 'w') as f:
    json.dump(data, f, indent=2)
    f.write('\n')

print("Hook registered in settings.json")
PYEOF
}

merge_settings_jq() {
    local settings_file="$1"
    local hook_command="$2"

    local existing
    existing=$(jq -r \
        '[.hooks.Stop // [] | .[] | .hooks // [] | .[] | select(.command | contains("vibecheck"))] | length' \
        "$settings_file" 2>/dev/null || echo "0")

    if [[ "$existing" != "0" ]]; then
        info "Already installed - skipping settings merge."
        return 0
    fi

    local result
    result=$(jq \
        --arg cmd "$hook_command" \
        '.hooks //= {} | .hooks.Stop //= [] | .hooks.Stop += [{"hooks": [{"type": "command", "command": $cmd, "timeout": 5}]}]' \
        "$settings_file")

    echo "$result" > "$settings_file"
    info "Hook registered in settings.json"
}

# ---- Remove from settings.json ----
remove_from_settings() {
    local settings_file="$1"

    if [[ ! -f "$settings_file" ]]; then
        return 0
    fi

    cp "$settings_file" "${settings_file}.bak.$(date +%s)"

    if command -v python3 &>/dev/null; then
        python3 - "$settings_file" <<'PYEOF'
import json, sys

settings_file = sys.argv[1]

try:
    with open(settings_file, 'r') as f:
        data = json.load(f)
except (json.JSONDecodeError, FileNotFoundError):
    sys.exit(0)

if "hooks" not in data or "Stop" not in data["hooks"]:
    sys.exit(0)

data["hooks"]["Stop"] = [
    entry for entry in data["hooks"]["Stop"]
    if not any("vibecheck" in h.get("command", "") for h in entry.get("hooks", []))
]

if not data["hooks"]["Stop"]:
    del data["hooks"]["Stop"]
if not data["hooks"]:
    del data["hooks"]

with open(settings_file, 'w') as f:
    json.dump(data, f, indent=2)
    f.write('\n')

print("Hook removed from settings.json")
PYEOF
    elif command -v jq &>/dev/null; then
        local result
        result=$(jq '
            if .hooks.Stop then
                .hooks.Stop |= map(
                    select((.hooks | map(select(.command | contains("vibecheck"))) | length) == 0)
                ) |
                if .hooks.Stop == [] then del(.hooks.Stop) else . end |
                if .hooks == {} then del(.hooks) else . end
            else . end
        ' "$settings_file")
        echo "$result" > "$settings_file"
        info "Hook removed from settings.json"
    else
        warn "Cannot auto-remove hook. Please manually remove the vibecheck entry from $settings_file"
    fi
}

# ---- Install ----
do_install() {
    info "Installing VibeCheck..."

    # Install /quiz skill
    local skill_dir="$CLAUDE_DIR/skills/quiz"
    mkdir -p "$skill_dir"
    if [[ -f "$skill_dir/SKILL.md" ]]; then
        info "Skill already exists, keeping your version."
    else
        write_skill "$skill_dir/SKILL.md"
        ok "Skill /quiz installed."
    fi

    if $SKILL_ONLY; then
        echo ""
        ok "Done! Type /quiz in Claude Code to quiz yourself."
        return 0
    fi

    # --- Full install: binary + config + settings ---
    mkdir -p "$HOOKS_DIR"

    # Binary
    local binary_dest="$HOOKS_DIR/$BINARY_NAME"
    local platform
    platform="$(detect_platform)"
    download_binary "$platform" "$binary_dest"

    # Config (don't overwrite)
    if [[ ! -f "$CLAUDE_DIR/$CONFIG_NAME" ]]; then
        write_default_config "$CLAUDE_DIR/$CONFIG_NAME"
        ok "Config created."
    else
        info "Config already exists, keeping your settings."
    fi

    # .gitignore
    local gitignore="$CLAUDE_DIR/.gitignore"
    if [[ -f "$gitignore" ]]; then
        if ! grep -qxF ".vibecheck/" "$gitignore" 2>/dev/null; then
            echo ".vibecheck/" >> "$gitignore"
        fi
    else
        echo ".vibecheck/" > "$gitignore"
    fi

    # Merge settings.json
    local hook_command
    if $GLOBAL; then
        hook_command="\$HOME/.claude/hooks/vibecheck"
    else
        hook_command="\$CLAUDE_PROJECT_DIR/.claude/hooks/vibecheck"
    fi
    merge_settings "$SETTINGS_FILE" "$hook_command"

    echo ""
    ok "VibeCheck installed!"
    echo ""
    info "Auto-quiz triggers after Claude Code finishes a task."
    info "Or type /quiz anytime for an on-demand quiz."
    info "Config: $CLAUDE_DIR/$CONFIG_NAME"
}

# ---- Uninstall ----
do_uninstall() {
    info "Uninstalling VibeCheck..."

    if [[ -f "$HOOKS_DIR/$BINARY_NAME" ]]; then
        rm "$HOOKS_DIR/$BINARY_NAME"
        ok "Binary removed."
    fi

    if [[ -d "$STATE_DIR" ]]; then
        rm -rf "$STATE_DIR"
        ok "State removed."
    fi

    if [[ -d "$CLAUDE_DIR/skills/quiz" ]]; then
        rm -rf "$CLAUDE_DIR/skills/quiz"
        ok "Skill removed."
    fi

    remove_from_settings "$SETTINGS_FILE"

    local gitignore="$CLAUDE_DIR/.gitignore"
    if [[ -f "$gitignore" ]]; then
        sed -i.bak '/^\.vibecheck\/$/d' "$gitignore" 2>/dev/null || true
        rm -f "${gitignore}.bak"
    fi

    echo ""
    ok "VibeCheck uninstalled."
}

# ---- Main ----
if $UNINSTALL; then
    do_uninstall
else
    do_install
fi
