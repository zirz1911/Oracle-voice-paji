#!/bin/bash
# Polish DMG after Tauri build
# Based on Oracle learning: 2026-01-03_dmg-icon-polish.md

set -e

APP_NAME="Oracle Voice Tray"
VERSION=$(grep '"version"' src-tauri/tauri.conf.json | head -1 | sed 's/.*: *"\([^"]*\)".*/\1/')
DMG_PATH="src-tauri/target/release/bundle/dmg/${APP_NAME}_${VERSION}_aarch64.dmg"

if [ ! -f "$DMG_PATH" ]; then
    echo "DMG not found at: $DMG_PATH"
    echo "Run 'npm run tauri build' first"
    exit 1
fi

echo "=== Polishing DMG: $DMG_PATH ==="
echo ""

# Step 0: Prepare Applications icon
echo "0. Preparing Applications folder icon..."
ICON_SOURCE="/System/Library/CoreServices/CoreTypes.bundle/Contents/Resources/ApplicationsFolderIcon.icns"
ICON_PNG="/tmp/applications-icon.png"
sips -s format png "$ICON_SOURCE" --out "$ICON_PNG" >/dev/null 2>&1
echo "   Icon extracted to $ICON_PNG"

# Step 1: Close Finder windows
echo "1. Closing Finder windows..."
osascript -e 'tell application "Finder" to close every window' 2>/dev/null || true
sleep 1

# Step 2: Clean up any existing mounts with same name
echo "2. Cleaning up existing mounts..."
for suffix in "" " 1" " 2" " 3"; do
    vol="/Volumes/${APP_NAME}${suffix}"
    if [ -d "$vol" ]; then
        hdiutil detach "$vol" -force 2>/dev/null || true
    fi
done
sleep 1

# Step 3: Create temp directory
TEMP_DIR=$(mktemp -d)
POLISHED_DMG="$TEMP_DIR/polished.dmg"
trap "rm -rf $TEMP_DIR" EXIT

# Step 4: Convert to read-write
echo "3. Converting to read-write..."
hdiutil convert "$DMG_PATH" -format UDRW -o "$POLISHED_DMG" -quiet

# Step 5: Mount with nobrowse (prevents Finder auto-open)
echo "4. Mounting (nobrowse)..."
hdiutil attach "$POLISHED_DMG" -mountpoint "/Volumes/${APP_NAME}" -nobrowse -noverify -noautoopen
sleep 1

VOLUME="/Volumes/${APP_NAME}"

# Step 6: Hide .VolumeIcon.icns
echo "5. Hiding .VolumeIcon.icns..."
if [ -f "${VOLUME}/.VolumeIcon.icns" ]; then
    chflags hidden "${VOLUME}/.VolumeIcon.icns"
    echo "   Done"
else
    echo "   Not found (ok)"
fi

# Step 7: Fix Applications folder
echo "6. Fixing Applications folder..."
# Remove existing (symlink or alias)
rm -f "${VOLUME}/Applications" 2>/dev/null || true

# Create alias via AppleScript
osascript <<EOF
tell application "Finder"
    make new alias file at POSIX file "${VOLUME}" to folder "Applications" of startup disk
end tell
EOF
echo "   Alias created"

# Step 8: Apply icon to alias
echo "7. Applying Applications folder icon..."
if command -v fileicon &> /dev/null; then
    fileicon set "${VOLUME}/Applications" "$ICON_PNG" 2>/dev/null && echo "   Done" || echo "   (icon set skipped)"
else
    echo "   fileicon not found (install with: brew install fileicon)"
fi

# Step 9: Set positions via AppleScript
echo "8. Setting icon positions..."
osascript <<EOF
tell application "Finder"
    tell disk "${APP_NAME}"
        open
        set current view of container window to icon view
        set toolbar visible of container window to false
        set statusbar visible of container window to false
        set bounds of container window to {100, 100, 760, 500}
        set theViewOptions to the icon view options of container window
        set arrangement of theViewOptions to not arranged
        set icon size of theViewOptions to 128
        set position of item "${APP_NAME}.app" of container window to {180, 170}
        set position of item "Applications" of container window to {480, 170}
        update without registering applications
        close
    end tell
end tell
EOF
echo "   Done"

# Step 10: Sync and unmount
echo "9. Syncing and unmounting..."
sync
sleep 2
hdiutil detach "$VOLUME" -force

# Step 11: Convert back to compressed
echo "10. Converting to final DMG..."
FINAL_DMG="${DMG_PATH%.dmg}_polished.dmg"
rm -f "$FINAL_DMG"
hdiutil convert "$POLISHED_DMG" -format UDZO -o "$FINAL_DMG" -quiet

echo ""
echo "=== Done! ==="
echo "Polished DMG: $FINAL_DMG"
echo ""
echo "To test: open \"$FINAL_DMG\""
