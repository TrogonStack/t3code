#!/bin/sh
# Wraps the built binary in a minimal macOS .app whose Info.plist registers the T3 Connect sign-in
# scheme (t3code-dev in debug, t3code in release, as the Electron desktop does), then runs it from
# inside the bundle so the browser callback reaches this process. Usage: bundle.sh debug|release [run]
set -eu

profile="${1:-debug}"
action="${2:-run}"
here="$(cd "$(dirname "$0")/.." && pwd)"
binary="t3-desktop-gpui"

if [ "$profile" = "release" ]; then
  scheme="t3code"
  name="T3 Code (GPUI)"
  identifier="com.t3tools.t3code.gpui"
  cargo build --locked --release
else
  scheme="t3code-dev"
  name="T3 Code (GPUI Dev)"
  identifier="com.t3tools.t3code.gpui.dev"
  cargo build --locked
fi

app="$here/target/bundle/$profile/$name.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS"
cp "$here/target/$profile/$binary" "$app/Contents/MacOS/$binary"
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key><string>$binary</string>
  <key>CFBundleIdentifier</key><string>$identifier</string>
  <key>CFBundleName</key><string>$name</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>CFBundleURLTypes</key>
  <array>
    <dict>
      <key>CFBundleURLName</key><string>T3 Code sign-in</string>
      <key>CFBundleURLSchemes</key><array><string>$scheme</string></array>
    </dict>
  </array>
</dict>
</plist>
PLIST

lsregister="/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister"
[ -x "$lsregister" ] && "$lsregister" -f "$app" >/dev/null 2>&1 || true

if [ "$action" = "run" ]; then
  if [ $# -ge 2 ]; then shift 2; else shift $#; fi
  exec "$app/Contents/MacOS/$binary" "$@"
fi
echo "$app"
