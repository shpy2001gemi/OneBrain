#!/usr/bin/env bash
set -euo pipefail

# OneBrain Build Script — Linux / macOS
# Builds both the Rust CLI binary and the Web Dashboard
# Auto-installs missing prerequisites (Rust, Node.js, Ollama)

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
SRC_DIR="$PROJECT_ROOT/src"
BUILD_DIR="$PROJECT_ROOT/build"

echo "╔══════════════════════════════════════════════╗"
echo "║       OneBrain — Build System                ║"
echo "╚══════════════════════════════════════════════╝"
echo

# ── Detect OS ────────────────────────────────────────────────
OS="$(uname -s)"
case "$OS" in
    Linux*)  PLATFORM="linux" ;;
    Darwin*) PLATFORM="macos" ;;
    *)       echo "⚠ Unsupported OS: $OS"; PLATFORM="linux" ;;
esac
echo "  Platform: $PLATFORM"
echo

# ── Helper: detect package manager (Linux) ───────────────────
detect_pkg_manager() {
    if command -v apt-get >/dev/null 2>&1; then echo "apt"
    elif command -v dnf >/dev/null 2>&1; then echo "dnf"
    elif command -v yum >/dev/null 2>&1; then echo "yum"
    elif command -v pacman >/dev/null 2>&1; then echo "pacman"
    elif command -v zypper >/dev/null 2>&1; then echo "zypper"
    elif command -v apk >/dev/null 2>&1; then echo "apk"
    else echo "unknown"
    fi
}

# ══════════════════════════════════════════════════════════════
# [1/6] Check & Install Prerequisites
# ══════════════════════════════════════════════════════════════
echo "[1/6] Checking & installing prerequisites..."
echo

INSTALLED_SOMETHING=false

# ── 1a. Rust ─────────────────────────────────────────────────
if command -v cargo >/dev/null 2>&1; then
    RUST_VER=$(rustc --version)
    echo "  ✓ Rust: $RUST_VER"
else
    echo "  ⚠ Rust not found — installing via rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
    # Load cargo into current shell
    if [ -f "$HOME/.cargo/env" ]; then
        source "$HOME/.cargo/env"
    fi
    RUST_VER=$(rustc --version)
    echo "  ✓ Rust installed: $RUST_VER"
    INSTALLED_SOMETHING=true
fi

# ── 1b. Node.js ──────────────────────────────────────────────
if command -v node >/dev/null 2>&1; then
    NODE_VER=$(node --version)
    echo "  ✓ Node.js: $NODE_VER"
else
    echo "  ⚠ Node.js not found — installing..."

    if [ "$PLATFORM" = "macos" ]; then
        if command -v brew >/dev/null 2>&1; then
            echo "    Installing via Homebrew..."
            brew install node
        else
            echo "    Installing Homebrew first..."
            /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
            # Add brew to PATH for Apple Silicon
            if [ -f "/opt/homebrew/bin/brew" ]; then
                eval "$(/opt/homebrew/bin/brew shellenv)"
            fi
            brew install node
        fi
    else
        PKG_MGR=$(detect_pkg_manager)
        case "$PKG_MGR" in
            apt)
                echo "    Installing via apt (NodeSource LTS)..."
                # Try NodeSource setup script
                if curl -fsSL https://deb.nodesource.com/setup_20.x | sudo -E bash - 2>/dev/null; then
                    sudo apt-get install -y nodejs
                else
                    # Fallback: system repo
                    sudo apt-get update && sudo apt-get install -y nodejs npm
                fi
                ;;
            dnf)
                echo "    Installing via dnf..."
                sudo dnf install -y nodejs npm
                ;;
            yum)
                echo "    Installing via yum..."
                curl -fsSL https://rpm.nodesource.com/setup_20.x | sudo bash -
                sudo yum install -y nodejs
                ;;
            pacman)
                echo "    Installing via pacman..."
                sudo pacman -Sy --noconfirm nodejs npm
                ;;
            zypper)
                echo "    Installing via zypper..."
                sudo zypper install -y nodejs npm
                ;;
            apk)
                echo "    Installing via apk..."
                sudo apk add --no-cache nodejs npm
                ;;
            *)
                echo "    ✗ Could not detect package manager."
                echo "    Please install Node.js manually: https://nodejs.org"
                exit 1
                ;;
        esac
    fi

    NODE_VER=$(node --version)
    echo "  ✓ Node.js installed: $NODE_VER"
    INSTALLED_SOMETHING=true
fi

# ── 1c. npm (usually comes with Node.js) ─────────────────────
if ! command -v npm >/dev/null 2>&1; then
    echo "  ⚠ npm not found (unusual). Trying to install..."
    if [ "$PLATFORM" = "macos" ]; then
        brew install npm 2>/dev/null || true
    else
        PKG_MGR=$(detect_pkg_manager)
        case "$PKG_MGR" in
            apt)     sudo apt-get install -y npm ;;
            dnf)     sudo dnf install -y npm ;;
            pacman)  sudo pacman -Sy --noconfirm npm ;;
            *)       echo "    Please install npm manually."; exit 1 ;;
        esac
    fi
    echo "  ✓ npm installed: $(npm --version)"
fi

# ── 1d. Ollama (optional, for AI) ────────────────────────────
if command -v ollama >/dev/null 2>&1; then
    OLLAMA_VER=$(ollama --version 2>/dev/null || echo "installed")
    echo "  ✓ Ollama: $OLLAMA_VER"
else
    echo "  ⚠ Ollama not found — installing (for AI features)..."
    if [ "$PLATFORM" = "macos" ]; then
        if command -v brew >/dev/null 2>&1; then
            brew install ollama
        else
            echo "    Downloading Ollama installer..."
            curl -fsSL https://ollama.ai/install.sh | sh
        fi
    else
        echo "    Installing via official script..."
        curl -fsSL https://ollama.ai/install.sh | sh
    fi

    if command -v ollama >/dev/null 2>&1; then
        echo "  ✓ Ollama installed"
        INSTALLED_SOMETHING=true

        # Pull default model
        echo
        echo "  Pulling default AI model (qwen3:8b)..."
        echo "  This may take a few minutes depending on your connection."
        # Start ollama in background if not running
        ollama serve >/dev/null 2>&1 &
        OLLAMA_PID=$!
        sleep 2
        ollama pull qwen3:8b || echo "  ⚠ Model pull failed — you can run 'ollama pull qwen3:8b' later"
        kill $OLLAMA_PID 2>/dev/null || true
    else
        echo "  ⚠ Ollama install failed — AI features will be unavailable"
        echo "    Install manually: https://ollama.ai"
    fi
fi

echo
if [ "$INSTALLED_SOMETHING" = true ]; then
    echo "  ────────────────────────────────────────────"
    echo "  New software was installed. If you see errors"
    echo "  below, restart your terminal and re-run."
    echo "  ────────────────────────────────────────────"
    echo
fi

# ══════════════════════════════════════════════════════════════
# [2/6] Build Rust CLI
# ══════════════════════════════════════════════════════════════
echo "[2/6] Building OneBrain CLI (Rust, release mode)..."
cd "$SRC_DIR"
cargo build --release -p onebrain-cli 2>&1 | tail -5
echo "  ✓ CLI binary built"
echo

# ══════════════════════════════════════════════════════════════
# [3/6] Build Web Dashboard
# ══════════════════════════════════════════════════════════════
echo "[3/6] Building Web Dashboard (React/Vite)..."
cd "$SRC_DIR/onebrain-web"
if [ ! -d "node_modules" ]; then
    echo "  Installing npm dependencies..."
    npm install --silent
fi
npm run build 2>&1 | tail -5
echo "  ✓ Web Dashboard built"
echo

# ══════════════════════════════════════════════════════════════
# [4/6] Create distribution
# ══════════════════════════════════════════════════════════════
echo "[4/6] Creating distribution package..."
rm -rf "$BUILD_DIR"
mkdir -p "$BUILD_DIR/bin"
mkdir -p "$BUILD_DIR/web"

cp "$SRC_DIR/target/release/onebrain" "$BUILD_DIR/bin/onebrain"
chmod +x "$BUILD_DIR/bin/onebrain"
cp -r "$SRC_DIR/onebrain-web/dist/"* "$BUILD_DIR/web/"

echo "  ✓ Distribution created at: $BUILD_DIR"
echo

# ══════════════════════════════════════════════════════════════
# [5/6] Create launcher
# ══════════════════════════════════════════════════════════════
echo "[5/6] Creating launcher..."

cat > "$BUILD_DIR/start.sh" << 'LAUNCHER'
#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BIN="$SCRIPT_DIR/bin/onebrain"
WEB_DIR="$SCRIPT_DIR/web"

if [ ! -f "$BIN" ]; then
    echo "❌ OneBrain binary not found at $BIN"
    exit 1
fi

# Start Ollama in background if available and not running
if command -v ollama >/dev/null 2>&1; then
    if ! curl -s http://localhost:11434/api/tags >/dev/null 2>&1; then
        echo "  Starting Ollama..."
        ollama serve >/dev/null 2>&1 &
        sleep 2
    fi
fi

echo "🧠 Starting OneBrain..."
echo

"$BIN" start --api --web-dir "$WEB_DIR" "$@"
LAUNCHER
chmod +x "$BUILD_DIR/start.sh"

echo "  ✓ Launcher created"
echo

# ══════════════════════════════════════════════════════════════
# [6/6] Verify
# ══════════════════════════════════════════════════════════════
echo "[6/6] Verifying..."
BIN_SIZE=$(du -h "$BUILD_DIR/bin/onebrain" | cut -f1)
WEB_FILES=$(find "$BUILD_DIR/web" -type f | wc -l)
echo "  ✓ Binary: $BIN_SIZE"
echo "  ✓ Web Dashboard: $WEB_FILES files"
echo

# ── Summary ──────────────────────────────────────────────────
echo "══════════════════════════════════════════════"
echo "✅ Build complete!"
echo
echo "Distribution: $BUILD_DIR"
echo "  bin/onebrain     — CLI + API server"
echo "  web/             — Web Dashboard (static)"
echo "  start.sh         — Quick launcher (auto-starts Ollama)"
echo
echo "To run:   cd $BUILD_DIR && ./start.sh"
echo "Browser:  http://localhost:4280"
echo "══════════════════════════════════════════════"
