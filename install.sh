#!/usr/bin/env bash
# Installer for ignix-v2-linux
# Usage: curl -fsSL https://raw.githubusercontent.com/DarulQutni-Q/ignix-v2-linux/main/install.sh | bash
set -euo pipefail

REPO="DarulQutni-Q/ignix-v2-linux"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
DESKTOP_DIR="$HOME/.local/share/applications"

echo "==> Installing Ignix V2 Linux config tool..."

# --- find latest release ---
if command -v curl >/dev/null; then
    DL="curl -fsSL"
elif command -v wget >/dev/null; then
    DL="wget -qO-"
else
    echo "Need curl or wget." >&2; exit 1
fi

TAG=$($DL "https://api.github.com/repos/$REPO/releases/latest" | grep -o '"tag_name": *"[^"]*"' | head -1 | cut -d'"' -f4)
if [ -z "$TAG" ]; then echo "Could not find latest release." >&2; exit 1; fi
echo "==> Latest release: $TAG"

# --- download binary ---
mkdir -p "$INSTALL_DIR"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
$DL -o "$TMP/ignix-gui" "https://github.com/$REPO/releases/download/$TAG/ignix-gui-linux-x86_64" \
    || { echo "Download failed." >&2; exit 1; }
chmod +x "$TMP/ignix-gui"
mv "$TMP/ignix-gui" "$INSTALL_DIR/ignix-gui"
echo "==> Binary installed to $INSTALL_DIR/ignix-gui"

# --- udev rule (no sudo needed to access the mouse) ---
if [ -w /etc/udev/rules.d ]; then
    $DL -o /etc/udev/rules.d/99-ignix-v2.rules \
        "https://raw.githubusercontent.com/$REPO/main/99-ignix-v2.rules"
    udevadm control --reload-rules 2>/dev/null || true
    udevadm trigger 2>/dev/null || true
    echo "==> udev rule installed"
else
    echo "==> Installing udev rule (needs sudo)..."
    $DL -o "$TMP/99-ignix-v2.rules" \
        "https://raw.githubusercontent.com/$REPO/main/99-ignix-v2.rules"
    sudo cp "$TMP/99-ignix-v2.rules" /etc/udev/rules.d/
    sudo udevadm control --reload-rules
    sudo udevadm trigger
    echo "==> udev rule installed"
fi

# --- desktop entry ---
mkdir -p "$DESKTOP_DIR"
cat > "$DESKTOP_DIR/ignix-v2.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Ignix V2 Config
Comment=Configure Vortex Ignix V2 mouse
Exec=$INSTALL_DIR/ignix-gui
Icon=input-mouse
Categories=Settings;HardwareSettings;
Terminal=false
EOF
echo "==> Desktop entry installed"

# --- PATH hint ---
case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *) echo "==> Add to PATH: export PATH=\"\$PATH:$INSTALL_DIR\"" ;;
esac

echo ""
echo "Done! Run: ignix-gui"
echo "Plug in your mouse and it will auto-detect."
