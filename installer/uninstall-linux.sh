#!/usr/bin/env bash
set -euo pipefail

# OneBrain Uninstaller — Linux / macOS
# Removes binary, web dashboard, and optionally data + AI models

echo "╔══════════════════════════════════════════════╗"
echo "║     OneBrain Uninstaller — Linux/macOS       ║"
echo "╚══════════════════════════════════════════════╝"
echo

# ── Detect install prefix ────────────────────────────────────
if [ "$(id -u)" -eq 0 ]; then
    PREFIX="/usr/local"
else
    PREFIX="$HOME/.local"
fi

INSTALL_BIN="$PREFIX/bin"
INSTALL_SHARE="$PREFIX/share/onebrain"

# ── Detect what's installed ──────────────────────────────────
echo "Scanning installation..."
echo

FOUND_SOMETHING=false

if [ -f "$INSTALL_BIN/onebrain" ]; then
    BIN_SIZE=$(du -h "$INSTALL_BIN/onebrain" 2>/dev/null | cut -f1)
    echo "  [x] Binary:    $INSTALL_BIN/onebrain ($BIN_SIZE)"
    FOUND_SOMETHING=true
else
    echo "  [ ] Binary:    not found"
fi

if [ -f "$INSTALL_BIN/onebrain-dashboard" ]; then
    echo "  [x] Launcher:  $INSTALL_BIN/onebrain-dashboard"
    FOUND_SOMETHING=true
else
    echo "  [ ] Launcher:  not found"
fi

if [ -d "$INSTALL_SHARE/web" ]; then
    WEB_COUNT=$(find "$INSTALL_SHARE/web" -type f 2>/dev/null | wc -l)
    echo "  [x] Web:       $INSTALL_SHARE/web/ ($WEB_COUNT files)"
    FOUND_SOMETHING=true
else
    echo "  [ ] Web:       not found"
fi

# Check for data directories
DATA_DIRS=()
for DIR in "./onebrain_data" "$HOME/.onebrain" "$HOME/onebrain_data"; do
    if [ -d "$DIR" ]; then
        DIR_SIZE=$(du -sh "$DIR" 2>/dev/null | cut -f1)
        echo "  [x] Data:      $DIR ($DIR_SIZE)"
        DATA_DIRS+=("$DIR")
    fi
done

# Check build directory
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
BUILD_DIR="$PROJECT_ROOT/build"
if [ -d "$BUILD_DIR" ]; then
    BUILD_SIZE=$(du -sh "$BUILD_DIR" 2>/dev/null | cut -f1)
    echo "  [x] Build:     $BUILD_DIR ($BUILD_SIZE)"
fi

# Check Ollama models
HAS_OLLAMA=false
if command -v ollama >/dev/null 2>&1; then
    HAS_OLLAMA=true
    echo "  [x] Ollama:    installed"
fi

echo

if [ "$FOUND_SOMETHING" = false ]; then
    echo "OneBrain is not installed at $PREFIX."
    echo "Nothing to remove."
    exit 0
fi

# ── Confirm ──────────────────────────────────────────────────
echo "────────────────────────────────────────────────"
read -p "Remove OneBrain binaries and web dashboard? [Y/n] " CONFIRM
CONFIRM=${CONFIRM:-Y}
if [[ ! "$CONFIRM" =~ ^[Yy]$ ]]; then
    echo "Cancelled."
    exit 0
fi

# ── Remove core installation ─────────────────────────────────
echo
echo "[1/4] Removing binaries..."

if [ -f "$INSTALL_BIN/onebrain" ]; then
    rm -f "$INSTALL_BIN/onebrain"
    echo "  ✓ Removed $INSTALL_BIN/onebrain"
fi

if [ -f "$INSTALL_BIN/onebrain-dashboard" ]; then
    rm -f "$INSTALL_BIN/onebrain-dashboard"
    echo "  ✓ Removed $INSTALL_BIN/onebrain-dashboard"
fi

echo "[2/4] Removing web dashboard..."
if [ -d "$INSTALL_SHARE" ]; then
    rm -rf "$INSTALL_SHARE"
    echo "  ✓ Removed $INSTALL_SHARE"
else
    echo "  - Nothing to remove"
fi

# ── Optional: remove build directory ─────────────────────────
echo "[3/4] Build artifacts..."
if [ -d "$BUILD_DIR" ]; then
    read -p "  Remove build directory ($BUILD_DIR)? [Y/n] " CONFIRM_BUILD
    CONFIRM_BUILD=${CONFIRM_BUILD:-Y}
    if [[ "$CONFIRM_BUILD" =~ ^[Yy]$ ]]; then
        rm -rf "$BUILD_DIR"
        echo "  ✓ Removed $BUILD_DIR"
    else
        echo "  - Kept"
    fi
else
    echo "  - No build directory"
fi

# ── Optional: remove data ────────────────────────────────────
echo "[4/4] Knowledge data..."
if [ ${#DATA_DIRS[@]} -gt 0 ]; then
    echo
    echo "  ⚠ WARNING: This will permanently delete your knowledge data!"
    echo "  Data directories found:"
    for DIR in "${DATA_DIRS[@]}"; do
        echo "    - $DIR"
    done
    echo
    read -p "  Delete ALL knowledge data? [y/N] " CONFIRM_DATA
    CONFIRM_DATA=${CONFIRM_DATA:-N}
    if [[ "$CONFIRM_DATA" =~ ^[Yy]$ ]]; then
        for DIR in "${DATA_DIRS[@]}"; do
            rm -rf "$DIR"
            echo "  ✓ Removed $DIR"
        done
    else
        echo "  - Data preserved"
    fi
else
    echo "  - No data directories found"
fi

# ── Optional: remove prerequisites installed by OneBrain ──────
MANIFEST="$PREFIX/share/onebrain/.installed-by-onebrain"
[ ! -f "$MANIFEST" ] && MANIFEST="$HOME/.local/share/onebrain/.installed-by-onebrain"

if [ -f "$MANIFEST" ]; then
    INSTALLED_TOOLS=$(cat "$MANIFEST" | grep -v '^$')
    if [ -n "$INSTALLED_TOOLS" ]; then
        echo
        echo "────────────────────────────────────────────────"
        echo "The following were installed BY OneBrain auto-installer."
        echo "Each will be asked individually:"
        echo

        # Ask for each tool one by one
        if echo "$INSTALLED_TOOLS" | grep -q "^rust$"; then
            read -p "  Remove Rust (rustup)? [y/N] " CONFIRM_RUST
            CONFIRM_RUST=${CONFIRM_RUST:-N}
            if [[ "$CONFIRM_RUST" =~ ^[Yy]$ ]]; then
                if command -v rustup >/dev/null 2>&1; then
                    rustup self uninstall -y 2>/dev/null && echo "  ✓ Rust uninstalled" || echo "  - Rust: manual removal needed"
                fi
            else
                echo "  - Rust: kept"
            fi
        fi

        if echo "$INSTALLED_TOOLS" | grep -q "^nodejs$"; then
            read -p "  Remove Node.js? [y/N] " CONFIRM_NODE
            CONFIRM_NODE=${CONFIRM_NODE:-N}
            if [[ "$CONFIRM_NODE" =~ ^[Yy]$ ]]; then
                PKG=$(detect_pkg_manager 2>/dev/null || echo "unknown")
                case "$PKG" in
                    apt)     sudo apt-get remove -y nodejs npm 2>/dev/null && echo "  ✓ Node.js uninstalled" ;;
                    dnf)     sudo dnf remove -y nodejs npm 2>/dev/null && echo "  ✓ Node.js uninstalled" ;;
                    pacman)  sudo pacman -Rns --noconfirm nodejs npm 2>/dev/null && echo "  ✓ Node.js uninstalled" ;;
                    brew)    brew uninstall node 2>/dev/null && echo "  ✓ Node.js uninstalled" ;;
                    *)       echo "  - Node.js: remove manually" ;;
                esac
            else
                echo "  - Node.js: kept"
            fi
        fi

        if echo "$INSTALLED_TOOLS" | grep -q "^ollama$"; then
            read -p "  Remove Ollama + AI models? [y/N] " CONFIRM_OLLAMA
            CONFIRM_OLLAMA=${CONFIRM_OLLAMA:-N}
            if [[ "$CONFIRM_OLLAMA" =~ ^[Yy]$ ]]; then
                ollama rm qwen3:8b 2>/dev/null || true
                OS="$(uname -s)"
                if [ "$OS" = "Darwin" ] && command -v brew >/dev/null 2>&1; then
                    brew uninstall ollama 2>/dev/null && echo "  ✓ Ollama uninstalled"
                else
                    sudo rm -f /usr/local/bin/ollama 2>/dev/null
                    sudo rm -rf /usr/local/lib/ollama 2>/dev/null
                    echo "  ✓ Ollama uninstalled"
                fi
            else
                echo "  - Ollama: kept"
            fi
        fi

        if echo "$INSTALLED_TOOLS" | grep -q "^git$"; then
            echo "  - Git: keeping (commonly needed by other tools)"
        fi

        # Remove manifest
        rm -f "$MANIFEST"
    fi
else
    # No manifest — prerequisites were NOT installed by OneBrain
    echo
    echo "────────────────────────────────────────────────"
    echo "No installer manifest found — prerequisites were not installed by OneBrain."
    echo "Keeping: Rust, Node.js, Ollama, Git"

    if [ "$HAS_OLLAMA" = true ]; then
        echo
        read -p "  Remove OneBrain's AI model (qwen3:8b) only? [y/N] " CONFIRM_MODEL
        CONFIRM_MODEL=${CONFIRM_MODEL:-N}
        if [[ "$CONFIRM_MODEL" =~ ^[Yy]$ ]]; then
            ollama rm qwen3:8b 2>/dev/null && echo "  ✓ Removed qwen3:8b model" || echo "  - Model not found"
        else
            echo "  - AI model preserved"
        fi
    fi
fi

# ── Summary ──────────────────────────────────────────────────
echo
echo "══════════════════════════════════════════════"
echo "✅ OneBrain uninstalled."
echo "══════════════════════════════════════════════"

