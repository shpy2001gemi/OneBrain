#!/usr/bin/env bash
set -euo pipefail
echo ""
echo "  â•”â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•—"
echo "  â•‘   ðŸ§  OneBrain â€” Installer                   â•‘"
echo "  â•šâ•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•"
echo ""
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
[ ! -f "$SCRIPT_DIR/bin/onebrain" ] && echo "âŒ Package incomplete" && exit 1

if [ "$(id -u)" -eq 0 ]; then PREFIX="/usr/local"; else PREFIX="$HOME/.local"; fi
BIN="$PREFIX/bin"; SHARE="$PREFIX/share/onebrain"
mkdir -p "$BIN" "$SHARE/web"
MANIFEST="$SHARE/.installed-by-onebrain"; > "$MANIFEST"

echo "[1/4] Installing..."
cp "$SCRIPT_DIR/bin/onebrain" "$BIN/onebrain"; chmod +x "$BIN/onebrain"
cp -r "$SCRIPT_DIR/web/"* "$SHARE/web/"
cat > "$BIN/onebrain-dashboard" << EOF
#!/usr/bin/env bash
command -v ollama >/dev/null 2>&1 && { curl -s http://localhost:11434/api/tags >/dev/null 2>&1 || { ollama serve >/dev/null 2>&1 & sleep 2; }; }
exec "$BIN/onebrain" start --api --web-dir "$SHARE/web" "\$@"
EOF
chmod +x "$BIN/onebrain-dashboard"
echo "  âœ“ Installed to $PREFIX"

echo "[2/4] PATH..."
if ! echo "$PATH" | grep -q "$BIN"; then
    SHELL_NAME=$(basename "${SHELL:-bash}")
    case "$SHELL_NAME" in
        zsh)  RC="$HOME/.zshrc" ;;
        fish) RC="$HOME/.config/fish/config.fish" ;;
        *)    RC="$HOME/.bashrc" ;;
    esac
    grep -q "$BIN" "$RC" 2>/dev/null || echo "export PATH=\"$BIN:\$PATH\"" >> "$RC"
    export PATH="$BIN:$PATH"
    echo "  âœ“ Added to $RC"
else echo "  âœ“ Already in PATH"; fi

echo "[3/4] Ollama..."
if command -v ollama >/dev/null 2>&1; then echo "  âœ“ Already installed"
else
    read -p "  âš  Install Ollama for AI? [Y/n] " R; R=${R:-Y}
    if [[ "$R" =~ ^[Yy]$ ]]; then
        curl -fsSL https://ollama.ai/install.sh | sh
        command -v ollama >/dev/null 2>&1 && { echo "ollama" >> "$MANIFEST"; echo "  âœ“ Installed"; } || echo "  âš  Failed"
    else echo "  - Skipped"; fi
fi

echo "[4/4] AI Model..."
if command -v ollama >/dev/null 2>&1; then
    read -p "  Download qwen3:8b (~4.9GB)? [Y/n] " R; R=${R:-Y}
    if [[ "$R" =~ ^[Yy]$ ]]; then
        ollama serve >/dev/null 2>&1 & sleep 3
        ollama pull qwen3:8b && echo "  âœ“ Ready" || echo "  âš  Failed"
        kill %1 2>/dev/null || true
    fi
fi

echo ""
echo "â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•"
echo "  âœ… OneBrain installed!"
echo "  Run: onebrain-dashboard"
echo "  Open: http://localhost:4280"
echo "â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•"
