#!/usr/bin/env bash
set -euo pipefail

# ══════════════════════════════════════════════════════════════
# OneBrain — Auto Installer (Linux / macOS)
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/<org>/OneBrain/main/installer/auto-install.sh | bash
#   OR
#   ./auto-install.sh
#
# What this script does:
#   1. Install Rust, Node.js, Ollama (if missing)
#   2. Clone OneBrain repository
#   3. Build CLI + Web Dashboard
#   4. Install to ~/.local (or /usr/local with sudo)
#   5. Pull default AI model (qwen3:8b)
#   6. Ready to run!
# ══════════════════════════════════════════════════════════════

REPO_URL="https://github.com/<your-org>/OneBrain.git"
DEFAULT_MODEL="qwen3:8b"
BRANCH="main"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

info()  { echo -e "${GREEN}  ✓${NC} $1"; }
warn()  { echo -e "${YELLOW}  ⚠${NC} $1"; }
err()   { echo -e "${RED}  ✗${NC} $1"; }
step()  { echo -e "\n${CYAN}${BOLD}$1${NC}"; }

echo -e "${CYAN}"
echo '  ╔══════════════════════════════════════════════╗'
echo '  ║   🧠 OneBrain — Auto Installer              ║'
echo '  ║   Decentralized Knowledge Network            ║'
echo '  ╚══════════════════════════════════════════════╝'
echo -e "${NC}"

# ── Detect OS ────────────────────────────────────────────────
OS="$(uname -s)"
ARCH="$(uname -m)"
case "$OS" in
    Linux*)  PLATFORM="linux" ;;
    Darwin*) PLATFORM="macos" ;;
    *)       err "Unsupported OS: $OS (use Windows installer)"; exit 1 ;;
esac
echo "  Platform: $PLATFORM ($ARCH)"

# ── Detect package manager ───────────────────────────────────
detect_pkg_manager() {
    if command -v apt-get >/dev/null 2>&1; then echo "apt"
    elif command -v dnf >/dev/null 2>&1; then echo "dnf"
    elif command -v yum >/dev/null 2>&1; then echo "yum"
    elif command -v pacman >/dev/null 2>&1; then echo "pacman"
    elif command -v zypper >/dev/null 2>&1; then echo "zypper"
    elif command -v apk >/dev/null 2>&1; then echo "apk"
    elif command -v brew >/dev/null 2>&1; then echo "brew"
    else echo "unknown"
    fi
}

# ── Manifest: track what WE install (so uninstaller knows) ───
MANIFEST_DIR="$HOME/.local/share/onebrain"
mkdir -p "$MANIFEST_DIR"
MANIFEST="$MANIFEST_DIR/.installed-by-onebrain"
# Start fresh manifest
> "$MANIFEST"

# ══════════════════════════════════════════════════════════════
step "[1/7] Installing prerequisites..."
# ══════════════════════════════════════════════════════════════

# ── Git (required for clone) ──────────────────────────────────
if command -v git >/dev/null 2>&1; then
    info "Git: $(git --version)"
else
    read -p "  ⚠ Git not found. Install Git? [Y/n] " INSTALL_GIT
    INSTALL_GIT=${INSTALL_GIT:-Y}
    if [[ "$INSTALL_GIT" =~ ^[Yy]$ ]]; then
        PKG=$(detect_pkg_manager)
        case "$PKG" in
            apt)     sudo apt-get update -qq && sudo apt-get install -y -qq git ;;
            dnf)     sudo dnf install -y -q git ;;
            yum)     sudo yum install -y -q git ;;
            pacman)  sudo pacman -Sy --noconfirm git ;;
            zypper)  sudo zypper install -y git ;;
            apk)     sudo apk add --no-cache git ;;
            brew)    brew install git ;;
            *)       err "Cannot auto-install git. Install manually: https://git-scm.com"; exit 1 ;;
        esac
        echo "git" >> "$MANIFEST"
        info "Git installed: $(git --version)"
    else
        err "Git is required to continue."; exit 1
    fi
fi

# ── Rust (required for build) ────────────────────────────────
if command -v cargo >/dev/null 2>&1; then
    info "Rust: $(rustc --version)"
else
    read -p "  ⚠ Rust not found. Install Rust (via rustup)? [Y/n] " INSTALL_RUST
    INSTALL_RUST=${INSTALL_RUST:-Y}
    if [[ "$INSTALL_RUST" =~ ^[Yy]$ ]]; then
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
        [ -f "$HOME/.cargo/env" ] && source "$HOME/.cargo/env"
        echo "rust" >> "$MANIFEST"
        info "Rust installed: $(rustc --version)"
    else
        err "Rust is required to build OneBrain."; exit 1
    fi
fi

# ── Node.js (required for web dashboard) ─────────────────────
if command -v node >/dev/null 2>&1; then
    info "Node.js: $(node --version)"
else
    read -p "  ⚠ Node.js not found. Install Node.js? [Y/n] " INSTALL_NODE
    INSTALL_NODE=${INSTALL_NODE:-Y}
    if [[ "$INSTALL_NODE" =~ ^[Yy]$ ]]; then
        if [ "$PLATFORM" = "macos" ]; then
            if command -v brew >/dev/null 2>&1; then
                brew install node
            else
                warn "Installing Homebrew first..."
                /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
                [ -f "/opt/homebrew/bin/brew" ] && eval "$(/opt/homebrew/bin/brew shellenv)"
                brew install node
            fi
        else
            PKG=$(detect_pkg_manager)
            case "$PKG" in
                apt)
                    curl -fsSL https://deb.nodesource.com/setup_20.x | sudo -E bash - 2>/dev/null \
                        && sudo apt-get install -y -qq nodejs \
                        || { sudo apt-get update -qq && sudo apt-get install -y -qq nodejs npm; }
                    ;;
                dnf)     sudo dnf install -y -q nodejs npm ;;
                yum)     curl -fsSL https://rpm.nodesource.com/setup_20.x | sudo bash - && sudo yum install -y -q nodejs ;;
                pacman)  sudo pacman -Sy --noconfirm nodejs npm ;;
                zypper)  sudo zypper install -y nodejs npm ;;
                apk)     sudo apk add --no-cache nodejs npm ;;
                *)       err "Install Node.js manually: https://nodejs.org"; exit 1 ;;
            esac
        fi
        echo "nodejs" >> "$MANIFEST"
        info "Node.js installed: $(node --version)"
    else
        err "Node.js is required to build Web Dashboard."; exit 1
    fi
fi

# ── Ollama (optional, for AI features) ───────────────────────
if command -v ollama >/dev/null 2>&1; then
    info "Ollama: installed"
else
    echo
    read -p "  ⚠ Ollama not found. Install Ollama for AI features? [Y/n] " INSTALL_OLLAMA
    INSTALL_OLLAMA=${INSTALL_OLLAMA:-Y}
    if [[ "$INSTALL_OLLAMA" =~ ^[Yy]$ ]]; then
        if [ "$PLATFORM" = "macos" ] && command -v brew >/dev/null 2>&1; then
            brew install ollama
        else
            curl -fsSL https://ollama.ai/install.sh | sh
        fi
        if command -v ollama >/dev/null 2>&1; then
            echo "ollama" >> "$MANIFEST"
            info "Ollama installed"
        else
            warn "Ollama install failed — AI features will be unavailable"
        fi
    else
        warn "Skipped Ollama — AI features (Chat, Encode) will be unavailable"
    fi
fi

# ══════════════════════════════════════════════════════════════
step "[2/7] Cloning OneBrain..."
# ══════════════════════════════════════════════════════════════

INSTALL_TMP="$HOME/.onebrain-install"

if [ -d "$INSTALL_TMP/.git" ]; then
    info "Repository exists — pulling latest..."
    cd "$INSTALL_TMP"
    git pull --ff-only origin "$BRANCH" 2>/dev/null || true
else
    rm -rf "$INSTALL_TMP"
    git clone --depth 1 --branch "$BRANCH" "$REPO_URL" "$INSTALL_TMP"
    cd "$INSTALL_TMP"
    info "Cloned to $INSTALL_TMP"
fi

# ══════════════════════════════════════════════════════════════
step "[3/7] Building CLI (Rust, release mode)..."
# ══════════════════════════════════════════════════════════════

cd "$INSTALL_TMP/src"
cargo build --release -p onebrain-cli 2>&1 | tail -3
info "CLI binary built"

# ══════════════════════════════════════════════════════════════
step "[4/7] Building Web Dashboard (React/Vite)..."
# ══════════════════════════════════════════════════════════════

cd "$INSTALL_TMP/src/onebrain-web"
[ ! -d "node_modules" ] && npm install --silent
npm run build 2>&1 | tail -3
info "Web Dashboard built"

# ══════════════════════════════════════════════════════════════
step "[5/7] Installing..."
# ══════════════════════════════════════════════════════════════

PREFIX="$HOME/.local"
INSTALL_BIN="$PREFIX/bin"
INSTALL_SHARE="$PREFIX/share/onebrain"

mkdir -p "$INSTALL_BIN"
mkdir -p "$INSTALL_SHARE/web"

# Binary
cp "$INSTALL_TMP/src/target/release/onebrain" "$INSTALL_BIN/onebrain"
chmod +x "$INSTALL_BIN/onebrain"
info "Binary: $INSTALL_BIN/onebrain"

# Web dashboard
cp -r "$INSTALL_TMP/src/onebrain-web/dist/"* "$INSTALL_SHARE/web/"
info "Web:    $INSTALL_SHARE/web/"

# Launcher script
cat > "$INSTALL_BIN/onebrain-dashboard" << EOF
#!/usr/bin/env bash
# OneBrain — Launch with Web Dashboard

# Auto-start Ollama if available
if command -v ollama >/dev/null 2>&1; then
    if ! curl -s http://localhost:11434/api/tags >/dev/null 2>&1; then
        ollama serve >/dev/null 2>&1 &
        sleep 2
    fi
fi

exec onebrain start --api --web-dir "$INSTALL_SHARE/web" "\$@"
EOF
chmod +x "$INSTALL_BIN/onebrain-dashboard"
info "Launcher: $INSTALL_BIN/onebrain-dashboard"

# ══════════════════════════════════════════════════════════════
step "[6/7] Pulling AI model ($DEFAULT_MODEL)..."
# ══════════════════════════════════════════════════════════════

if command -v ollama >/dev/null 2>&1; then
    # Start ollama temporarily
    ollama serve >/dev/null 2>&1 &
    OLLAMA_PID=$!
    sleep 3

    echo "  Downloading $DEFAULT_MODEL (this may take a few minutes)..."
    if ollama pull "$DEFAULT_MODEL" 2>/dev/null; then
        info "Model $DEFAULT_MODEL ready"
    else
        warn "Model pull failed — run 'ollama pull $DEFAULT_MODEL' later"
    fi

    kill $OLLAMA_PID 2>/dev/null || true
else
    warn "Ollama not available — skipping model download"
fi

# ══════════════════════════════════════════════════════════════
step "[7/7] Configuring PATH..."
# ══════════════════════════════════════════════════════════════

PATH_ADDED=false
if echo "$PATH" | grep -q "$INSTALL_BIN"; then
    info "Already in PATH"
else
    # Detect shell
    SHELL_NAME=$(basename "${SHELL:-bash}")
    case "$SHELL_NAME" in
        zsh)  RC_FILE="$HOME/.zshrc" ;;
        fish) RC_FILE="$HOME/.config/fish/config.fish" ;;
        *)    RC_FILE="$HOME/.bashrc" ;;
    esac

    # Add to shell config
    if [ -f "$RC_FILE" ]; then
        if ! grep -q "$INSTALL_BIN" "$RC_FILE" 2>/dev/null; then
            echo "" >> "$RC_FILE"
            echo "# OneBrain" >> "$RC_FILE"
            if [ "$SHELL_NAME" = "fish" ]; then
                echo "set -gx PATH $INSTALL_BIN \$PATH" >> "$RC_FILE"
            else
                echo "export PATH=\"$INSTALL_BIN:\$PATH\"" >> "$RC_FILE"
            fi
            info "Added to $RC_FILE"
            PATH_ADDED=true
        fi
    fi

    # Apply for current session
    export PATH="$INSTALL_BIN:$PATH"
fi

# ── Cleanup ──────────────────────────────────────────────────
rm -rf "$INSTALL_TMP/src/target" 2>/dev/null || true

# ══════════════════════════════════════════════════════════════
# DONE!
# ══════════════════════════════════════════════════════════════
echo ""
echo -e "${CYAN}══════════════════════════════════════════════════${NC}"
echo -e "${GREEN}${BOLD}  ✅ OneBrain installed successfully!${NC}"
echo ""
echo -e "  ${BOLD}Quick start:${NC}"
echo -e "    ${YELLOW}onebrain-dashboard${NC}"
echo ""
echo -e "  ${BOLD}Then open:${NC}"
echo -e "    ${YELLOW}http://localhost:4280${NC}"
echo -e "    Token: ${YELLOW}onebrain-dev-token${NC}"
echo ""
echo -e "  ${BOLD}Commands:${NC}"
echo "    onebrain start              # CLI only"
echo "    onebrain start --api        # CLI + API"
echo "    onebrain-dashboard          # CLI + API + Web (all-in-one)"
echo ""
if [ "$PATH_ADDED" = true ]; then
    echo -e "  ${YELLOW}⚠ Restart terminal or run: source $RC_FILE${NC}"
    echo ""
fi
echo -e "${CYAN}══════════════════════════════════════════════════${NC}"
