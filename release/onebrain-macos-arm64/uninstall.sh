#!/usr/bin/env bash
set -euo pipefail
echo "ðŸ§  OneBrain Uninstaller"
read -p "Remove OneBrain? [Y/n] " C; C=${C:-Y}
[[ ! "$C" =~ ^[Yy]$ ]] && echo "Cancelled." && exit 0
if [ "$(id -u)" -eq 0 ]; then P="/usr/local"; else P="$HOME/.local"; fi
M="$P/share/onebrain/.installed-by-onebrain"
TOOLS=""; [ -f "$M" ] && TOOLS=$(cat "$M" | grep -v '^$')
rm -f "$P/bin/onebrain" "$P/bin/onebrain-dashboard"
rm -rf "$P/share/onebrain"
echo "  âœ“ OneBrain removed"
if echo "$TOOLS" | grep -q "^ollama$"; then
    read -p "  Remove Ollama (installed by OneBrain)? [y/N] " R
    if [[ "${R:-N}" =~ ^[Yy]$ ]]; then
        ollama rm qwen3:8b 2>/dev/null || true
        sudo rm -f /usr/local/bin/ollama 2>/dev/null
        sudo rm -rf /usr/local/lib/ollama 2>/dev/null
        echo "  âœ“ Ollama removed"
    fi
fi
echo "Done."
