#!/usr/bin/env bash
# ==============================================================================
# VortexSeries IGNIX F5 / IGNIX V2 Mouse Driver - Linux Installer
# Reverse-Engineered from Official OEM Windows Application (IGNIX V2-Setup-1.0.0.exe)
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/DarulQutni-Q/ignix-v2-linux/main/install.sh | bash
# ==============================================================================
set -euo pipefail

REPO="DarulQutni-Q/ignix-v2-linux"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/ignix-f5"
DESKTOP_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
ICON_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/icons"

echo "======================================================================"
echo "  VortexSeries IGNIX F5 / IGNIX V2 Linux Native Driver Installer"
echo "  Reverse-Engineered from Official OEM Application (BK-MouseHub/Next.js)"
echo "======================================================================"

# --- 1. Downloader Check ---
if command -v curl >/dev/null 2>&1; then
    DL="curl -fsSL"
elif command -v wget >/dev/null 2>&1; then
    DL="wget -qO-"
else
    echo "Error: curl or wget is required." >&2
    exit 1
fi

# --- 2. Electron Dependency Check ---
echo "==> Checking Electron runtime..."
ELECTRON_FOUND=""
for bin in electron39 electron42 electron43 electron44 electron; do
    if command -v "$bin" >/dev/null 2>&1; then
        ELECTRON_FOUND="$bin"
        break
    fi
done

if [ -n "$ELECTRON_FOUND" ]; then
    echo "    Found compatible Electron runtime: $ELECTRON_FOUND"
else
    echo "    [!] Warning: Electron runtime not found in PATH."
    echo "    IGNIX F5 runs natively via Electron + WebHID."
    echo "    Please install electron using your distribution's package manager:"
    if command -v pacman >/dev/null 2>&1; then
        echo "      Arch / CachyOS: sudo pacman -S electron"
    elif command -v apt-get >/dev/null 2>&1; then
        echo "      Debian / Ubuntu: sudo apt install electron (or npm install -g electron)"
    elif command -v dnf >/dev/null 2>&1; then
        echo "      Fedora: sudo dnf install electron"
    fi
    echo "    You can continue the installation; just install electron before launching."
fi

# --- 3. Directory Setup ---
mkdir -p "$INSTALL_DIR" "$DATA_DIR" "$DESKTOP_DIR" "$ICON_DIR"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

# --- 4. Install Application Package (app.asar) ---
echo "==> Setting up official reverse-engineered application package..."
# If app.asar is already present locally (e.g. installed or cloned), reuse it
if [ -f "$DATA_DIR/app.asar" ]; then
    echo "    Using existing app package at $DATA_DIR/app.asar"
elif [ -f "./app.asar" ]; then
    cp "./app.asar" "$DATA_DIR/app.asar"
    echo "    Copied app.asar from local directory to $DATA_DIR/app.asar"
else
    # Fetch from latest GitHub release asset
    echo "    Fetching release metadata from GitHub..."
    LATEST_JSON=$($DL "https://api.github.com/repos/$REPO/releases/latest" 2>/dev/null || true)
    ASAR_URL=$(echo "$LATEST_JSON" | grep -o 'https://[^"]*app\.asar' | head -1 || true)
    
    if [ -n "$ASAR_URL" ]; then
        echo "    Downloading app.asar from release: $ASAR_URL..."
        $DL -o "$DATA_DIR/app.asar" "$ASAR_URL"
    else
        # Fallback to direct releases download endpoint
        echo "    Downloading app.asar from release repository..."
        if ! $DL -o "$DATA_DIR/app.asar" "https://github.com/$REPO/releases/download/v2.0.0/app.asar" 2>/dev/null; then
            echo "    [Notice] Release asset downloading. If on a fresh clone without published release assets,"
            echo "    you can place the extracted 'app.asar' into $DATA_DIR/app.asar"
        fi
    fi
fi

# Record installed version
echo "1.0.0" > "$DATA_DIR/version"

# --- 5. Install Runner Binary ---
echo "==> Installing runner script to $INSTALL_DIR/ignix-f5..."
cat > "$INSTALL_DIR/ignix-f5" << 'EOF'
#!/usr/bin/env bash
# Runner for VortexSeries IGNIX F5 / IGNIX V2 Mouse Driver
# Reverse-engineered from official OEM Windows application
set -eo pipefail

APP_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/ignix-f5"
ASAR_PATH="$APP_DIR/app.asar"

if [ ! -f "$ASAR_PATH" ]; then
    echo "Error: IGNIX F5 app package not found at $ASAR_PATH." >&2
    echo "Please reinstall using:" >&2
    echo "  curl -fsSL https://raw.githubusercontent.com/DarulQutni-Q/ignix-v2-linux/main/install.sh | bash" >&2
    exit 1
fi

ELECTRON_BIN=""
for candidate in electron39 electron42 electron43 electron44 electron; do
    if command -v "$candidate" >/dev/null 2>&1; then
        ELECTRON_BIN="$candidate"
        break
    fi
done

if [ -z "$ELECTRON_BIN" ]; then
    echo "Error: No compatible Electron binary found on system." >&2
    echo "Please install electron (e.g. 'sudo pacman -S electron')." >&2
    exit 1
fi

EXTRA_FLAGS=()
if [ -n "$WAYLAND_DISPLAY" ]; then
    EXTRA_FLAGS+=("--ozone-platform-hint=auto" "--enable-features=WaylandWindowDecorations")
fi

exec "$ELECTRON_BIN" "$ASAR_PATH" "${EXTRA_FLAGS[@]}" "$@"
EOF
chmod +x "$INSTALL_DIR/ignix-f5"

# Symlink ignix-gui / ignix-v2 for backwards compatibility
ln -sf "$INSTALL_DIR/ignix-f5" "$INSTALL_DIR/ignix-v2"
ln -sf "$INSTALL_DIR/ignix-f5" "$INSTALL_DIR/ignix-gui"

# --- 6. Install Icons ---
echo "==> Installing icons..."
# Download or copy icon
if [ -f "./assets/vortex-ignix.png" ]; then
    cp "./assets/vortex-ignix.png" "$DATA_DIR/icon.png"
    cp "./assets/vortex-ignix.png" "$ICON_DIR/vortex-ignix.png"
elif [ -f "$ICON_DIR/vortex-ignix.png" ]; then
    cp "$ICON_DIR/vortex-ignix.png" "$DATA_DIR/icon.png"
else
    $DL -o "$DATA_DIR/icon.png" "https://raw.githubusercontent.com/$REPO/main/assets/vortex-ignix.png" 2>/dev/null || true
    [ -f "$DATA_DIR/icon.png" ] && cp "$DATA_DIR/icon.png" "$ICON_DIR/vortex-ignix.png"
fi

# Populate standard hicolor sizes
if [ -f "$ICON_DIR/vortex-ignix.png" ]; then
    for s in 16 24 32 48 64 128 256 512; do
        HICOLOR_TARGET="$ICON_DIR/hicolor/${s}x${s}/apps"
        mkdir -p "$HICOLOR_TARGET"
        cp "$ICON_DIR/vortex-ignix.png" "$HICOLOR_TARGET/vortex-ignix.png"
    done
fi

# Also update active icon themes if present
for theme_dir in "$ICON_DIR/WhiteSur/apps/scalable" "$ICON_DIR/WhiteSur-dark/apps/scalable"; do
    if [ -d "$theme_dir" ] && [ -f "$ICON_DIR/vortex-ignix.png" ]; then
        cp "$ICON_DIR/vortex-ignix.png" "$theme_dir/vortex-ignix.png"
    fi
done

# --- 7. Install Desktop Entry ---
echo "==> Installing desktop entry..."
cat > "$DESKTOP_DIR/ignix-f5.desktop" <<EOF
[Desktop Entry]
Name=IGNIX F5
GenericName=Mouse Driver & Configuration Tool
Comment=Official configuration tool for VortexSeries IGNIX F5 / IGNIX V2 Mouse (Reverse-Engineered for Linux)
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

# Clean up deprecated / duplicate entries
rm -f "$DESKTOP_DIR/ignix-v2.desktop" 2>/dev/null || true

# --- 8. Install Udev Rules ---
echo "==> Configuring udev permissions (no root required for mouse access)..."
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
    echo "    udev rules installed directly to $UDEV_RULE_FILE"
else
    echo "    Installing udev rule via sudo..."
    echo "$UDEV_CONTENT" > "$TMP/99-ignix.rules"
    if command -v sudo >/dev/null 2>&1; then
        sudo cp "$TMP/99-ignix.rules" "$UDEV_RULE_FILE"
        sudo udevadm control --reload-rules 2>/dev/null || true
        sudo udevadm trigger 2>/dev/null || true
        echo "    udev rules installed successfully."
    else
        echo "    [!] Warning: Could not write to /etc/udev/rules.d/ without sudo."
        echo "    Please run manually: sudo cp $TMP/99-ignix.rules $UDEV_RULE_FILE"
    fi
fi

# --- 9. Refresh Desktop Database & Icon Cache ---
echo "==> Refreshing system caches..."
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$DESKTOP_DIR" 2>/dev/null || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -f -t "$ICON_DIR/hicolor" 2>/dev/null || true
fi

# --- 10. Summary & Finish ---
echo ""
echo "======================================================================"
echo "  Installation Successful!"
echo "======================================================================"
echo "  Application : IGNIX F5 (VortexSeries IGNIX V2 / F5 Driver)"
echo "  Binary Path : $INSTALL_DIR/ignix-f5"
echo "  Package Path: $DATA_DIR/app.asar"
echo "  Desktop File: $DESKTOP_DIR/ignix-f5.desktop"
echo "  Devices     : Wired (1D57:22AE) & 2.4G Wireless Dongle (1D57:FA60)"
echo ""
echo "  Launch the app from your application launcher (search 'IGNIX F5')"
echo "  or directly run: ignix-f5"
echo "======================================================================"
