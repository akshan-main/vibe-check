#!/usr/bin/env bash
set -euo pipefail

# VibeCheck installer
# Installs the VibeCheck Stop hook into a Claude Code project or globally.

VERSION="0.1.0"
REPO="akshan-main/vibe-check"
BINARY_NAME="vibecheck"
CONFIG_NAME="vibecheck.json"
HOOK_MARKER="vibecheck"  # used to detect existing installs

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
  --update      Update to the latest version (pulls latest, re-installs)
  --uninstall   Remove VibeCheck
  --help        Show this help message

Examples:
  bash install.sh                     # Install everything in current project
  bash install.sh /path/to/project    # Install in specific project
  bash install.sh --skill-only        # Only the /quiz command, no auto-quiz
  bash install.sh --global            # Install globally
  bash install.sh --update            # Update to latest version
  bash install.sh --uninstall         # Uninstall from current project
EOF
}

# ---- Argument parsing ----
GLOBAL=false
UNINSTALL=false
SKILL_ONLY=false
UPDATE=false
TARGET_DIR="."

while [[ $# -gt 0 ]]; do
    case "$1" in
        --global)     GLOBAL=true; shift ;;
        --uninstall)  UNINSTALL=true; shift ;;
        --skill-only) SKILL_ONLY=true; shift ;;
        --update)     UPDATE=true; shift ;;
        --help|-h)   usage; exit 0 ;;
        -*)          error "Unknown option: $1"; usage; exit 1 ;;
        *)           TARGET_DIR="$1"; shift ;;
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
        *)      error "Unsupported OS: $os"; exit 1 ;;
    esac

    case "$arch" in
        x86_64|amd64)  arch="x86_64" ;;
        arm64|aarch64) arch="arm64" ;;
        *)             error "Unsupported architecture: $arch"; exit 1 ;;
    esac

    echo "${os}-${arch}"
}

# ---- Download binary ----
download_binary() {
    local platform="$1"
    local dest="$2"
    local url="https://github.com/${REPO}/releases/latest/download/vibecheck-${platform}"

    info "Downloading vibecheck binary for ${platform}..."
    if curl -fsSL --retry 3 -o "$dest" "$url" 2>/dev/null; then
        chmod +x "$dest"
        ok "Binary downloaded."
        return 0
    fi

    warn "Download failed. Trying to build from source..."
    return 1
}

# ---- Build from source ----
build_from_source() {
    local dest="$1"

    if ! command -v cargo &>/dev/null; then
        error "Cannot download binary and cargo is not installed."
        error "Install Rust (https://rustup.rs) or download a binary manually."
        exit 1
    fi

    # Find the repo root (where Cargo.toml is)
    local script_dir
    script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    local repo_root="$script_dir/.."

    if [[ ! -f "$repo_root/Cargo.toml" ]]; then
        error "Cargo.toml not found. Run install.sh from the vibecheck repo."
        exit 1
    fi

    info "Building from source..."
    cargo build --release --manifest-path "$repo_root/Cargo.toml"
    cp "$repo_root/target/release/vibecheck" "$dest"
    chmod +x "$dest"
    ok "Built from source."
}

# ---- Merge settings.json ----
merge_settings() {
    local settings_file="$1"
    local hook_command="$2"

    # Create file if missing
    if [[ ! -f "$settings_file" ]]; then
        echo '{}' > "$settings_file"
    fi

    # Backup
    cp "$settings_file" "${settings_file}.bak.$(date +%s)"

    if command -v python3 &>/dev/null; then
        merge_settings_python "$settings_file" "$hook_command"
    elif command -v jq &>/dev/null; then
        merge_settings_jq "$settings_file" "$hook_command"
    else
        error "Neither python3 nor jq found. Please manually add the hook to $settings_file"
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
import json, sys, os, shutil, time

settings_file = sys.argv[1]
hook_command = sys.argv[2]

# Read existing
try:
    with open(settings_file, 'r') as f:
        data = json.load(f)
except (json.JSONDecodeError, FileNotFoundError):
    data = {}

if not isinstance(data, dict):
    data = {}

# Ensure structure
data.setdefault("hooks", {})
data["hooks"].setdefault("Stop", [])

# Check for duplicate
for entry in data["hooks"]["Stop"]:
    for hook in entry.get("hooks", []):
        if "vibecheck" in hook.get("command", ""):
            print("Already installed - skipping settings merge.")
            sys.exit(0)

# Add our entry
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

    # Check for existing
    local existing
    existing=$(jq -r \
        --arg cmd "$hook_command" \
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

# Filter out entries containing "vibecheck"
data["hooks"]["Stop"] = [
    entry for entry in data["hooks"]["Stop"]
    if not any("vibecheck" in h.get("command", "") for h in entry.get("hooks", []))
]

# Clean up empty structures
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

    local script_dir
    script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

    # Always install the /quiz skill (on-demand command)
    local skill_dir="$CLAUDE_DIR/skills/quiz"
    local template_skill="$script_dir/../templates/project/.claude/skills/quiz/SKILL.md"
    mkdir -p "$skill_dir"
    if [[ -f "$template_skill" ]]; then
        # Back up existing skill if user has customized it
        if [[ -f "$skill_dir/SKILL.md" ]]; then
            if ! diff -q "$template_skill" "$skill_dir/SKILL.md" &>/dev/null; then
                cp "$skill_dir/SKILL.md" "$skill_dir/SKILL.md.bak.$(date +%s)"
                info "Existing skill backed up (had custom changes)."
            fi
        fi
        cp "$template_skill" "$skill_dir/SKILL.md"
        ok "Skill /quiz installed (on-demand quiz)."
    fi

    if $SKILL_ONLY; then
        echo ""
        ok "VibeCheck installed (skill only)!"
        info "Type /quiz in Claude Code to quiz yourself on your current diff."
        return 0
    fi

    # --- Full install: hook binary + config + settings merge ---

    # Create directories
    mkdir -p "$HOOKS_DIR"

    # Get the binary
    local binary_dest="$HOOKS_DIR/$BINARY_NAME"
    local platform
    platform="$(detect_platform)"

    if ! download_binary "$platform" "$binary_dest"; then
        build_from_source "$binary_dest"
    fi

    # Copy config (don't overwrite existing)
    local template_config="$script_dir/../templates/project/.claude/$CONFIG_NAME"

    if [[ ! -f "$CLAUDE_DIR/$CONFIG_NAME" ]]; then
        if [[ -f "$template_config" ]]; then
            cp "$template_config" "$CLAUDE_DIR/$CONFIG_NAME"
            ok "Config created at $CLAUDE_DIR/$CONFIG_NAME"
        else
            # Create default config inline
            cat > "$CLAUDE_DIR/$CONFIG_NAME" <<'JSONEOF'
{
  "enabled": true,
  "minSecondsBetweenQuizzes": 900,
  "maxDiffChars": 2000
}
JSONEOF
            ok "Default config created."
        fi
    else
        info "Config already exists, keeping your settings."
    fi

    # Update .gitignore
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
    info "Config: $CLAUDE_DIR/$CONFIG_NAME"
    info "Binary: $binary_dest"
    info "Settings: $SETTINGS_FILE"
    info "Skill: /quiz (on-demand quiz)"
    echo ""
    info "Start using Claude Code - VibeCheck will auto-trigger after code changes."
    info "Or type /quiz anytime for an on-demand quiz."
}

# ---- Uninstall ----
do_uninstall() {
    info "Uninstalling VibeCheck..."

    # Remove binary
    if [[ -f "$HOOKS_DIR/$BINARY_NAME" ]]; then
        rm "$HOOKS_DIR/$BINARY_NAME"
        ok "Binary removed."
    fi

    # Remove state directory
    if [[ -d "$STATE_DIR" ]]; then
        rm -rf "$STATE_DIR"
        ok "State directory removed."
    fi

    # Remove skill (back up if customized)
    if [[ -d "$CLAUDE_DIR/skills/quiz" ]]; then
        local skill_file="$CLAUDE_DIR/skills/quiz/SKILL.md"
        local script_dir
        script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
        local template_skill="$script_dir/../templates/project/.claude/skills/quiz/SKILL.md"
        if [[ -f "$skill_file" ]] && [[ -f "$template_skill" ]]; then
            if ! diff -q "$template_skill" "$skill_file" &>/dev/null; then
                cp "$skill_file" "$CLAUDE_DIR/skills/quiz_SKILL.md.bak.$(date +%s)"
                info "Custom skill backed up before removal."
            fi
        fi
        rm -rf "$CLAUDE_DIR/skills/quiz"
        ok "Skill /quiz removed."
    fi

    # Remove from settings.json
    remove_from_settings "$SETTINGS_FILE"

    # Remove .gitignore entry
    local gitignore="$CLAUDE_DIR/.gitignore"
    if [[ -f "$gitignore" ]]; then
        if command -v sed &>/dev/null; then
            sed -i.bak '/^\.vibecheck\/$/d' "$gitignore" 2>/dev/null || true
            rm -f "${gitignore}.bak"
        fi
    fi

    echo ""
    ok "VibeCheck uninstalled."
    info "Config file left at $CLAUDE_DIR/$CONFIG_NAME (delete manually if desired)."
}

# ---- Update ----
do_update() {
    local script_dir
    script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    local repo_root="$script_dir/.."

    if [[ ! -d "$repo_root/.git" ]]; then
        error "Not a git checkout. Clone the repo first, then run --update."
        exit 1
    fi

    info "Pulling latest version..."
    git -C "$repo_root" pull --ff-only origin main || {
        error "Failed to pull. You may have local changes. Try: git -C $repo_root pull"
        exit 1
    }

    ok "Updated to latest."
    info "Re-running install..."
    echo ""

    # Re-run install (without --update to avoid loop)
    local args=()
    $GLOBAL && args+=(--global)
    $SKILL_ONLY && args+=(--skill-only)
    [[ "$TARGET_DIR" != "$(pwd)" ]] && args+=("$TARGET_DIR")

    bash "$repo_root/install/install.sh" "${args[@]}"
}

# ---- Main ----
if $UPDATE; then
    do_update
elif $UNINSTALL; then
    do_uninstall
else
    do_install
fi
