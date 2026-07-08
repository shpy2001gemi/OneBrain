#!/usr/bin/env bash
set -euo pipefail

# ══════════════════════════════════════════════════════════════
# OneBrain Release Builder — Linux / macOS
#
# Developer runs this to create distributable packages.
# Output: onebrain-<platform>-<arch>.tar.gz
#
# Users only need to extract and run install.sh
# ══════════════════════════════════════════════════════════════

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
SRC_DIR="$PROJECT_ROOT/src"
RELEASE_DIR="$PROJECT_ROOT/release"

OS="$(uname -s)"
ARCH="$(uname -m)"
case "$OS" in
    Linux*)  PLATFORM="linux" ;;
    Darwin*) PLATFORM="macos" ;;
    *)       echo "Unsupported: $OS"; exit 1 ;;
esac

PACKAGE_NAME="onebrain-${PLATFORM}-${ARCH}"

echo "╔══════════════════════════════════════════════╗"
echo "║   OneBrain — Release Builder                 ║"
echo "║   Target: $PACKAGE_NAME"
echo "╚══════════════════════════════════════════════╝"
echo

# ── Check prerequisites ──────────────────────────────────────
echo "[1/5] Checking build tools..."
command -v cargo >/dev/null 2>&1 || { echo "❌ Rust required"; exit 1; }
command -v node >/dev/null 2>&1  || { echo "❌ Node.js required"; exit 1; }
echo "  ✓ Rust: $(rustc --version)"
echo "  ✓ Node: $(node --version)"
echo

# ── Build CLI ────────────────────────────────────────────────
echo "[2/5] Building CLI (release mode)..."
cd "$SRC_DIR"
cargo build --release -p onebrain-cli 2>&1 | tail -3
echo "  ✓ CLI built"
echo

# ── Build Web Dashboard ─────────────────────────────────────
echo "[3/5] Building Web Dashboard..."
cd "$SRC_DIR/onebrain-web"
[ ! -d "node_modules" ] && npm install --silent
npm run build 2>&1 | tail -3
echo "  ✓ Web built"
echo

# ── Create package ───────────────────────────────────────────
echo "[4/5] Creating release package..."
DIST_DIR="$RELEASE_DIR/$PACKAGE_NAME"
rm -rf "$DIST_DIR"
mkdir -p "$DIST_DIR/bin"
mkdir -p "$DIST_DIR/web"

# Copy binary
cp "$SRC_DIR/target/release/onebrain" "$DIST_DIR/bin/onebrain"
chmod +x "$DIST_DIR/bin/onebrain"

# Copy web
cp -r "$SRC_DIR/onebrain-web/dist/"* "$DIST_DIR/web/"

# ── Create install.sh (bundled with package) ─────────────────
cat > "$DIST_DIR/install.sh" << 'INSTALL_SCRIPT'
#!/usr/bin/env bash
set -euo pipefail

echo ""
echo "  ╔══════════════════════════════════════════════╗"
echo "  ║   🧠 OneBrain — Installer                   ║"
echo "  ╚══════════════════════════════════════════════╝"
echo ""

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# Validate package contents
if [ ! -f "$SCRIPT_DIR/bin/onebrain" ]; then
    echo "❌ Package incomplete: bin/onebrain not found"
    exit 1
fi
if [ ! -d "$SCRIPT_DIR/web" ]; then
    echo "❌ Package incomplete: web/ not found"
    exit 1
fi

# Determine prefix
if [ "$(id -u)" -eq 0 ]; then
    PREFIX="/usr/local"
else
    PREFIX="$HOME/.local"
fi
INSTALL_BIN="$PREFIX/bin"
INSTALL_SHARE="$PREFIX/share/onebrain"

echo "Install to: $PREFIX"
echo ""

# ── Manifest ─────────────────────────────────────────────────
MANIFEST_DIR="$INSTALL_SHARE"
mkdir -p "$MANIFEST_DIR"
MANIFEST="$MANIFEST_DIR/.installed-by-onebrain"
> "$MANIFEST"

# ── [1/4] Install files ─────────────────────────────────────
echo "[1/4] Installing OneBrain..."
mkdir -p "$INSTALL_BIN"
mkdir -p "$INSTALL_SHARE/web"

cp "$SCRIPT_DIR/bin/onebrain" "$INSTALL_BIN/onebrain"
chmod +x "$INSTALL_BIN/onebrain"
echo "  ✓ Binary: $INSTALL_BIN/onebrain"

cp -r "$SCRIPT_DIR/web/"* "$INSTALL_SHARE/web/"
echo "  ✓ Web:    $INSTALL_SHARE/web/"

# Create launcher
cat > "$INSTALL_BIN/onebrain-dashboard" << EOF
#!/usr/bin/env bash
if command -v ollama >/dev/null 2>&1; then
    if ! curl -s http://localhost:11434/api/tags >/dev/null 2>&1; then
        ollama serve >/dev/null 2>&1 &
        sleep 2
    fi
fi
exec "$INSTALL_BIN/onebrain" start --api --web-dir "$INSTALL_SHARE/web" "\$@"
EOF
chmod +x "$INSTALL_BIN/onebrain-dashboard"
echo "  ✓ Launcher: $INSTALL_BIN/onebrain-dashboard"
echo ""

# ── [2/4] PATH ───────────────────────────────────────────────
echo "[2/4] Configuring PATH..."
if echo "$PATH" | grep -q "$INSTALL_BIN"; then
    echo "  ✓ Already in PATH"
else
    SHELL_NAME=$(basename "${SHELL:-bash}")
    case "$SHELL_NAME" in
        zsh)  RC_FILE="$HOME/.zshrc" ;;
        fish) RC_FILE="$HOME/.config/fish/config.fish" ;;
        *)    RC_FILE="$HOME/.bashrc" ;;
    esac
    if [ -f "$RC_FILE" ] && ! grep -q "$INSTALL_BIN" "$RC_FILE" 2>/dev/null; then
        echo "" >> "$RC_FILE"
        echo "# OneBrain" >> "$RC_FILE"
        if [ "$SHELL_NAME" = "fish" ]; then
            echo "set -gx PATH $INSTALL_BIN \$PATH" >> "$RC_FILE"
        else
            echo "export PATH=\"$INSTALL_BIN:\$PATH\"" >> "$RC_FILE"
        fi
        echo "  ✓ Added to $RC_FILE"
    fi
    export PATH="$INSTALL_BIN:$PATH"
fi
echo ""

# ── [3/4] Ollama ─────────────────────────────────────────────
echo "[3/4] AI Engine (Ollama)..."
if command -v ollama >/dev/null 2>&1; then
    echo "  ✓ Ollama: already installed"
else
    echo ""
    read -p "  ⚠ Ollama not found. Install Ollama for AI features? [Y/n] " INSTALL_OLLAMA
    INSTALL_OLLAMA=${INSTALL_OLLAMA:-Y}
    if [[ "$INSTALL_OLLAMA" =~ ^[Yy]$ ]]; then
        OS_NAME="$(uname -s)"
        if [ "$OS_NAME" = "Darwin" ] && command -v brew >/dev/null 2>&1; then
            brew install ollama
        else
            curl -fsSL https://ollama.ai/install.sh | sh
        fi
        if command -v ollama >/dev/null 2>&1; then
            echo "ollama" >> "$MANIFEST"
            echo "  ✓ Ollama installed"
        else
            echo "  ⚠ Install failed — AI features unavailable"
        fi
    else
        echo "  - Skipped (AI features will be unavailable)"
    fi
fi
echo ""

# ── [4/4] Pull AI model ─────────────────────────────────────
echo "[4/4] AI Model..."
if command -v ollama >/dev/null 2>&1; then
    read -p "  Download AI model (qwen3:8b, ~4.9GB)? [Y/n] " PULL_MODEL
    PULL_MODEL=${PULL_MODEL:-Y}
    if [[ "$PULL_MODEL" =~ ^[Yy]$ ]]; then
        # Start ollama temporarily
        ollama serve >/dev/null 2>&1 &
        OLLAMA_PID=$!
        sleep 3
        ollama pull qwen3:8b && echo "  ✓ Model ready" || echo "  ⚠ Download failed — run 'ollama pull qwen3:8b' later"
        kill $OLLAMA_PID 2>/dev/null || true
    else
        echo "  - Skipped (run 'ollama pull qwen3:8b' later)"
    fi
else
    echo "  - Ollama not installed, skipping"
fi

# ── Done! ────────────────────────────────────────────────────
echo ""
echo "══════════════════════════════════════════════════"
echo "  ✅ OneBrain installed successfully!"
echo ""
echo "  Quick start:"
echo "    onebrain-dashboard"
echo ""
echo "  Then open:"
echo "    http://localhost:4280"
echo "    Token: onebrain-dev-token"
echo ""
echo "  Commands:"
echo "    onebrain start              # CLI only"
echo "    onebrain start --api        # CLI + API + Web"
echo "    onebrain-dashboard          # All-in-one launcher"
echo "══════════════════════════════════════════════════"
INSTALL_SCRIPT
chmod +x "$DIST_DIR/install.sh"

# ── Create uninstall.sh (bundled with package) ───────────────
cat > "$DIST_DIR/uninstall.sh" << 'UNINSTALL_SCRIPT'
#!/usr/bin/env bash
set -euo pipefail

echo "🧠 OneBrain Uninstaller"
echo ""

if [ "$(id -u)" -eq 0 ]; then PREFIX="/usr/local"; else PREFIX="$HOME/.local"; fi
INSTALL_BIN="$PREFIX/bin"
INSTALL_SHARE="$PREFIX/share/onebrain"
MANIFEST="$INSTALL_SHARE/.installed-by-onebrain"

# Remove OneBrain
read -p "Remove OneBrain? [Y/n] " CONFIRM
CONFIRM=${CONFIRM:-Y}
[[ ! "$CONFIRM" =~ ^[Yy]$ ]] && echo "Cancelled." && exit 0

rm -f "$INSTALL_BIN/onebrain"
rm -f "$INSTALL_BIN/onebrain-dashboard"
rm -rf "$INSTALL_SHARE"
echo "  ✓ OneBrain removed"

# Remove prerequisites installed by us
if [ -f "$MANIFEST" ]; then
    if grep -q "^ollama$" "$MANIFEST" 2>/dev/null; then
        read -p "  Remove Ollama (installed by OneBrain)? [y/N] " C
        if [[ "${C:-N}" =~ ^[Yy]$ ]]; then
            ollama rm qwen3:8b 2>/dev/null || true
            OS="$(uname -s)"
            if [ "$OS" = "Darwin" ] && command -v brew >/dev/null 2>&1; then
                brew uninstall ollama 2>/dev/null
            else
                sudo rm -f /usr/local/bin/ollama 2>/dev/null
                sudo rm -rf /usr/local/lib/ollama 2>/dev/null
            fi
            echo "  ✓ Ollama removed"
        fi
    fi
fi

echo ""
echo "✅ OneBrain uninstalled."
UNINSTALL_SCRIPT
chmod +x "$DIST_DIR/uninstall.sh"

# ── Create README ────────────────────────────────────────────
cat > "$DIST_DIR/README.txt" << 'README'
OneBrain — Decentralized Knowledge Network

INSTALL:
  chmod +x install.sh
  ./install.sh

AFTER INSTALL:
  onebrain-dashboard          # Start everything
  Open http://localhost:4280   # Web Dashboard
  Token: onebrain-dev-token

UNINSTALL:
  chmod +x uninstall.sh
  ./uninstall.sh

REQUIREMENTS:
  - Ollama (optional, for AI) — installer will ask to install
  - No other dependencies needed!
README

echo "  ✓ Package contents ready"
echo

# ── Create archive ───────────────────────────────────────────
echo "[5/5] Creating archive..."
cd "$RELEASE_DIR"
tar -czf "${PACKAGE_NAME}.tar.gz" "$PACKAGE_NAME"

ARCHIVE_SIZE=$(du -h "${PACKAGE_NAME}.tar.gz" | cut -f1)
BIN_SIZE=$(du -h "$DIST_DIR/bin/onebrain" | cut -f1)
WEB_FILES=$(find "$DIST_DIR/web" -type f | wc -l)

echo ""
echo "══════════════════════════════════════════════════"
echo "✅ Release package created!"
echo ""
echo "  📦 $RELEASE_DIR/${PACKAGE_NAME}.tar.gz ($ARCHIVE_SIZE)"
echo ""
echo "  Contents:"
echo "    bin/onebrain      — CLI + API server ($BIN_SIZE)"
echo "    web/              — Web Dashboard ($WEB_FILES files)"
echo "    install.sh        — User installer"
echo "    uninstall.sh      — User uninstaller"
echo "    README.txt        — Quick start guide"
echo ""
echo "  Send this file to users. They extract and run:"
echo "    tar xzf ${PACKAGE_NAME}.tar.gz"
echo "    cd $PACKAGE_NAME"
echo "    ./install.sh"
echo "══════════════════════════════════════════════════"
