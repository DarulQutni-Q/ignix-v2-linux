#!/usr/bin/env bash
# Installer for the vendor IGNIX F5 app on Linux (Electron + app.asar).
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/DarulQutni-Q/ignix-v2-linux/main/install.sh | bash
# This installs the vendor app only. It does not build the Rust tool in src/.
set -euo pipefail

REPO="DarulQutni-Q/ignix-v2-linux"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/ignix-f5"
DESKTOP_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
ICON_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/icons"

if command -v curl >/dev/null 2>&1; then
    DL="curl -fsSL"
elif command -v wget >/dev/null 2>&1; then
    DL="wget -qO-"
else
    echo "Error: curl or wget is required." >&2
    exit 1
fi

echo "Checking Electron runtime..."
ELECTRON_FOUND=""
for bin in electron39 electron42 electron43 electron44 electron; do
    if command -v "$bin" >/dev/null 2>&1; then
        ELECTRON_FOUND="$bin"
        break
    fi
done

if [ -n "$ELECTRON_FOUND" ]; then
    echo "Found Electron runtime: $ELECTRON_FOUND"
else
    echo "Warning: no Electron runtime in PATH. Installation continues, but"
    echo "ignix-f5 will not run until Electron is installed (for example:"
    echo "Arch/CachyOS: sudo pacman -S electron; Debian/Ubuntu: sudo apt install electron)."
fi

mkdir -p "$INSTALL_DIR" "$DATA_DIR" "$DESKTOP_DIR" "$ICON_DIR"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

echo "Fetching app.asar from the latest release..."
LATEST_JSON=$($DL "https://api.github.com/repos/$REPO/releases/latest" 2>/dev/null || true)
ASAR_URL=$(echo "$LATEST_JSON" | grep -o 'https://[^"]*app\.asar' | head -1 || true)
TAG=$(echo "$LATEST_JSON" | grep -o '"tag_name": *"[^"]*"' | head -1 | sed 's/.*: *"//; s/"//' || true)

if [ -f "$DATA_DIR/app.asar" ]; then
    echo "Using existing app package at $DATA_DIR/app.asar"
elif [ -n "$ASAR_URL" ]; then
    echo "Downloading app.asar from: $ASAR_URL"
    $DL -o "$DATA_DIR/app.asar" "$ASAR_URL"
else
    echo "Error: no app.asar asset in the latest release. Nothing installed." >&2
    exit 1
fi

echo "${TAG:-unknown}" > "$DATA_DIR/version"

# Runner: single source of truth is bin/ignix-f5 in this repo. When this
# script runs via curl there is no local copy, so fetch that file too
# instead of embedding a second, diverging copy here.
echo "Installing runner to $INSTALL_DIR/ignix-f5..."
if [ -f "./bin/ignix-f5" ]; then
    cp "./bin/ignix-f5" "$INSTALL_DIR/ignix-f5"
else
    $DL -o "$INSTALL_DIR/ignix-f5" "https://raw.githubusercontent.com/$REPO/main/bin/ignix-f5"
fi
chmod +x "$INSTALL_DIR/ignix-f5"
ln -sf "$INSTALL_DIR/ignix-f5" "$INSTALL_DIR/ignix-v2"
ln -sf "$INSTALL_DIR/ignix-f5" "$INSTALL_DIR/ignix-gui"

echo "Installing icon..."
if [ -f "./assets/icon.png" ]; then
    cp "./assets/icon.png" "$DATA_DIR/icon.png"
    cp "./assets/icon.png" "$ICON_DIR/vortex-ignix.png"
elif [ -f "$ICON_DIR/vortex-ignix.png" ]; then
    cp "$ICON_DIR/vortex-ignix.png" "$DATA_DIR/icon.png"
else
    $DL -o "$DATA_DIR/icon.png" "https://raw.githubusercontent.com/$REPO/main/assets/icon.png" 2>/dev/null || true
    [ -f "$DATA_DIR/icon.png" ] && cp "$DATA_DIR/icon.png" "$ICON_DIR/vortex-ignix.png"
fi

if [ -f "$ICON_DIR/vortex-ignix.png" ]; then
    for s in 16 24 32 48 64 128 256 512; do
        HICOLOR_TARGET="$ICON_DIR/hicolor/${s}x${s}/apps"
        mkdir -p "$HICOLOR_TARGET"
        cp "$ICON_DIR/vortex-ignix.png" "$HICOLOR_TARGET/vortex-ignix.png"
    done
fi

echo "Installing desktop entry..."
cat > "$DESKTOP_DIR/ignix-f5.desktop" <<EOF
[Desktop Entry]
Name=IGNIX F5
GenericName=Mouse Configuration Tool
Comment=Configuration tool for VortexSeries IGNIX F5 / V2 mouse
Exec=$INSTALL_DIR/ignix-f5 %U
Icon=vortex-ignix
Type=Application
StartupNotify=true
Categories=Settings;HardwareSettings;
StartupWMClass=ignix-f5
Terminal=false
Keywords=vortex;vortexseries;ignix;f5;v2;mouse;dpi;macro;webhid;pixart;
EOF
chmod 644 "$DESKTOP_DIR/ignix-f5.desktop"
rm -f "$DESKTOP_DIR/ignix-v2.desktop" 2>/dev/null || true

echo "Installing udev rule..."
UDEV_RULE_FILE="/etc/udev/rules.d/99-ignix.rules"
UDEV_CONTENT='# VortexSeries IGNIX F5 / IGNIX V2 Gaming Mouse (PixArt PAW3315)
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="1d57", ATTRS{idProduct}=="22ae", MODE="0666", TAG+="uaccess"
SUBSYSTEM=="usb", ATTRS{idVendor}=="1d57", ATTRS{idProduct}=="22ae", MODE="0666", TAG+="uaccess"
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="1d57", ATTRS{idProduct}=="fa60", MODE="0666", TAG+="uaccess"
SUBSYSTEM=="usb", ATTRS{idVendor}=="1d57", ATTRS{idProduct}=="fa60", MODE="0666", TAG+="uaccess"
'

if [ -w /etc/udev/rules.d ]; then
    echo "$UDEV_CONTENT" > "$UDEV_RULE_FILE"
    udevadm control --reload-rules 2>/dev/null || true
    udevadm trigger 2>/dev/null || true
    echo "udev rule installed directly to $UDEV_RULE_FILE"
elif command -v sudo >/dev/null 2>&1; then
    echo "$UDEV_CONTENT" > "$TMP/99-ignix.rules"
    sudo cp "$TMP/99-ignix.rules" "$UDEV_RULE_FILE"
    sudo udevadm control --reload-rules 2>/dev/null || true
    sudo udevadm trigger 2>/dev/null || true
    echo "udev rule installed via sudo to $UDEV_RULE_FILE"
else
    echo "Warning: could not write to /etc/udev/rules.d/ without sudo."
    echo "Run manually: sudo cp 99-ignix.rules $UDEV_RULE_FILE"
fi

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$DESKTOP_DIR" 2>/dev/null || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -f -t "$ICON_DIR/hicolor" 2>/dev/null || true
fi

echo "Done."
echo "Launcher: $INSTALL_DIR/ignix-f5"
echo "Package: $DATA_DIR/app.asar"
echo "Desktop entry: $DESKTOP_DIR/ignix-f5.desktop"
echo "Run with: ignix-f5"
