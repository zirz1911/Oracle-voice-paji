# Fixing the Ugly Tauri DMG Installer on macOS

**TL;DR**: Tauri's default DMG bundler creates functional but ugly installers. The Applications folder shows as a broken icon, and system files are visible. Here's how to fix it with a post-build script.

---

## The Problem

When you run `tauri build` on macOS, you get a `.dmg` file that works... but looks unprofessional:

1. **`.VolumeIcon.icns` visible** - A system file that shouldn't be shown
2. **Applications folder has broken icon** - Just an empty outline instead of the blue folder
3. **No alias arrow** - Users can't tell it's a shortcut

This happens because Tauri uses a symlink for the Applications folder, and macOS symlinks don't display the folder's icon.

---

## Root Cause

### Why Symlinks Don't Work

```
Symlink → Points to /Applications
         → Doesn't inherit the folder's icon
         → Shows as generic/broken icon
```

### Why Alias Works

```
Alias → macOS Finder construct
      → Inherits visual properties
      → Shows proper icon + alias arrow badge
```

The fix requires:
1. Converting the symlink to an alias
2. Explicitly applying the Applications folder icon
3. Hiding system files

---

## The Solution

### Prerequisites

```bash
brew install fileicon
```

### Post-Build Script

Create `scripts/polish-dmg.sh`:

```bash
#!/bin/bash
# Polish DMG after Tauri build

set -e

APP_NAME="Your App Name"
DMG_PATH="src-tauri/target/release/bundle/dmg/${APP_NAME}_x.x.x_aarch64.dmg"

# Extract Applications folder icon
ICON_SOURCE="/System/Library/CoreServices/CoreTypes.bundle/Contents/Resources/ApplicationsFolderIcon.icns"
sips -s format png "$ICON_SOURCE" --out /tmp/applications-icon.png

# Close Finder windows (prevents "resource busy")
osascript -e 'tell application "Finder" to close every window'

# Convert to read-write
TEMP_DIR=$(mktemp -d)
hdiutil convert "$DMG_PATH" -format UDRW -o "$TEMP_DIR/rw.dmg"

# Mount WITHOUT Finder interference
hdiutil attach "$TEMP_DIR/rw.dmg" -mountpoint "/Volumes/$APP_NAME" \
    -nobrowse -noverify -noautoopen

VOLUME="/Volumes/$APP_NAME"

# 1. Hide .VolumeIcon.icns
chflags hidden "$VOLUME/.VolumeIcon.icns"

# 2. Replace symlink with alias
rm -f "$VOLUME/Applications"
osascript -e "tell application \"Finder\" to make new alias file at POSIX file \"$VOLUME\" to folder \"Applications\" of startup disk"

# 3. Apply icon to alias
fileicon set "$VOLUME/Applications" /tmp/applications-icon.png

# 4. Set positions via AppleScript
osascript <<EOF
tell application "Finder"
    tell disk "$APP_NAME"
        open
        set current view of container window to icon view
        set icon size of icon view options of container window to 128
        set position of item "$APP_NAME.app" to {180, 170}
        set position of item "Applications" to {480, 170}
        close
    end tell
end tell
EOF

# Unmount and convert back
sync && sleep 2
hdiutil detach "$VOLUME" -force
hdiutil convert "$TEMP_DIR/rw.dmg" -format UDZO -o "${DMG_PATH%.dmg}_polished.dmg"

rm -rf "$TEMP_DIR"
echo "Done: ${DMG_PATH%.dmg}_polished.dmg"
```

### Usage

```bash
npm run tauri build
./scripts/polish-dmg.sh
```

---

## Common Pitfalls

### "Resource busy" Error

**Cause**: Finder has the DMG window open

**Fix**: Close Finder windows first, use `-nobrowse` flag when mounting

```bash
osascript -e 'tell application "Finder" to close every window'
hdiutil attach ... -nobrowse -noverify -noautoopen
```

### Icon Still Broken

**Cause**: Using symlink instead of alias, or missing `fileicon`

**Fix**:
1. Install fileicon: `brew install fileicon`
2. Use AppleScript to create alias (not `ln -s`)
3. Apply icon explicitly with `fileicon set`

### Multiple Volumes Mounted

**Cause**: Previous script runs left volumes mounted

**Fix**: Add cleanup at start of script

```bash
for suffix in "" " 1" " 2"; do
    hdiutil detach "/Volumes/${APP_NAME}${suffix}" -force 2>/dev/null || true
done
```

---

## Before & After

| Before | After |
|--------|-------|
| Broken Applications icon | Blue folder with alias arrow |
| .VolumeIcon.icns visible | Hidden |
| Generic look | Professional appearance |

---

## Why This Matters

First impressions count. When users download your app, the DMG installer is often their first interaction. A polished DMG signals quality and attention to detail.

The default Tauri DMG works, but these small fixes make your app feel more native and trustworthy.

---

**Tags**: tauri, macos, dmg, installer, rust, desktop-app
