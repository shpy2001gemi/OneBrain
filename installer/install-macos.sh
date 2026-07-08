#!/usr/bin/env bash
set -euo pipefail

# OneBrain Installer — macOS
# Installs to /usr/local or $HOME/.local

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
BUILD_DIR="$PROJECT_ROOT/build"

echo "╔══════════════════════════════════════════════╗"
echo "║     OneBrain Installer — macOS               ║"
echo "╚══════════════════════════════════════════════╝"
echo

if [ ! -f "$BUILD_DIR/bin/onebrain" ]; then
    echo "✗ Build not found. Run ./build.sh first."
    exit 1
fi

if [ "$(id -u)" -eq 0 ]; then
    PREFIX="/usr/local"
    echo "Installing system-wide to $PREFIX (root)"
else
    PREFIX="$HOME/.local"
    echo "Installing to $PREFIX (user)"
    echo "Tip: Run with sudo for system-wide install"
fi

INSTALL_BIN="$PREFIX/bin"
INSTALL_SHARE="$PREFIX/share/onebrain"

mkdir -p "$INSTALL_BIN"
mkdir -p "$INSTALL_SHARE/web"

echo
echo "[1/3] Installing binary..."
cp "$BUILD_DIR/bin/onebrain" "$INSTALL_BIN/onebrain"
chmod +x "$INSTALL_BIN/onebrain"
echo "  ✓ $INSTALL_BIN/onebrain"

echo "[2/3] Installing web dashboard..."
cp -r "$BUILD_DIR/web/"* "$INSTALL_SHARE/web/"
echo "  ✓ $INSTALL_SHARE/web/"

echo "[3/3] Creating wrapper script..."
cat > "$INSTALL_BIN/onebrain-dashboard" << EOF
#!/usr/bin/env bash
# OneBrain — Launch with Web Dashboard
exec onebrain start --api --web-dir "$INSTALL_SHARE/web" "\$@"
EOF
chmod +x "$INSTALL_BIN/onebrain-dashboard"
echo "  ✓ $INSTALL_BIN/onebrain-dashboard"

# macOS: remove quarantine attribute
xattr -cr "$INSTALL_BIN/onebrain" 2>/dev/null || true
xattr -cr "$INSTALL_BIN/onebrain-dashboard" 2>/dev/null || true

echo
echo "✅ Installation complete!"
echo

if echo "$PATH" | grep -q "$INSTALL_BIN"; then
    echo "onebrain is ready to use."
else
    SHELL_NAME=$(basename "$SHELL")
    RC_FILE="$HOME/.${SHELL_NAME}rc"
    echo "⚠  Add this to $RC_FILE:"
    echo "   export PATH=\"$INSTALL_BIN:\$PATH\""
fi

echo
echo "Commands:"
echo "  onebrain start                # CLI only"
echo "  onebrain start --api          # CLI + API"
echo "  onebrain-dashboard            # CLI + API + Web Dashboard"
echo
echo "Then open: http://localhost:4280"
echo "Token: onebrain-dev-token"
