#!/bin/sh
# Install or update nocast:
#   curl -fsSL https://raw.githubusercontent.com/cucumbers2blue/nocast/main/install.sh | sh
# Uninstall:
#   curl -fsSL https://raw.githubusercontent.com/cucumbers2blue/nocast/main/install.sh | sh -s uninstall
set -eu

ID=com.github.cucumbers2blue.nocast
APP="$HOME/Applications/nocast.app"
AGENT="$HOME/Library/LaunchAgents/$ID.plist"
URL=https://github.com/cucumbers2blue/nocast/releases/latest/download/nocast.zip
DOMAIN="gui/$(id -u)"

launchctl bootout "$DOMAIN/$ID" 2>/dev/null || true

if [ "${1:-}" = "uninstall" ]; then
    rm -rf "$APP" "$AGENT"
    tccutil reset Accessibility "$ID" >/dev/null 2>&1 || true
    tccutil reset AppleEvents "$ID" >/dev/null 2>&1 || true
    echo "nocast removed. Your config is still at ~/.config/nocast.conf"
    exit 0
fi

echo "Downloading nocast..."
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
curl -fsSL "$URL" -o "$TMP/nocast.zip"

mkdir -p "$HOME/Applications" "$HOME/Library/LaunchAgents" "$HOME/Library/Logs"
rm -rf "$APP"
ditto -x -k "$TMP/nocast.zip" "$HOME/Applications"

# Each build is a new binary, so macOS forgets the old permissions. Clear the stale entries.
tccutil reset Accessibility "$ID" >/dev/null 2>&1 || true
tccutil reset AppleEvents "$ID" >/dev/null 2>&1 || true

# Start at login, restart if it ever crashes.
cat > "$AGENT" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key><string>$ID</string>
    <key>ProgramArguments</key><array><string>$APP/Contents/MacOS/nocast</string></array>
    <key>RunAtLoad</key><true/>
    <key>KeepAlive</key><dict><key>SuccessfulExit</key><false/></dict>
    <key>ProcessType</key><string>Interactive</string>
    <key>StandardErrorPath</key><string>$HOME/Library/Logs/nocast.log</string>
</dict>
</plist>
PLIST
launchctl bootstrap "$DOMAIN" "$AGENT"

cat <<'MSG'

nocast is installed and running.

1. macOS will ask to give nocast Accessibility access. Click "Open System Settings"
   and turn nocast on. Hotkeys start working right away.
2. To use cmd+space, turn off Spotlight's shortcut:
   System Settings > Keyboard > Keyboard Shortcuts > Spotlight.
3. The first time you open a website, allow nocast to control Safari.
4. Edit your config:  open -e ~/.config/nocast.conf   (saved changes apply instantly)
MSG
