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
  icon="app-icon"
  cargo build --locked --release
else
  scheme="t3code-dev"
  name="T3 Code (GPUI Dev)"
  identifier="com.t3tools.t3code.gpui.dev"
  icon="app-icon-dev"
  cargo build --locked
fi

app="$here/target/bundle/$profile/$name.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$here/target/$profile/$binary" "$app/Contents/MacOS/$binary"

source="$here/resources/$icon.png"
icns="$here/target/bundle/$icon.icns"
if [ ! -f "$icns" ] || [ "$source" -nt "$icns" ]; then
  icons="$here/target/bundle/$icon.iconset"
  rm -rf "$icons" && mkdir -p "$icons"
  for size in 16 32 128 256 512; do
    sips -z "$size" "$size" "$source" --out "$icons/icon_${size}x${size}.png" >/dev/null
    double=$((size * 2))
    sips -z "$double" "$double" "$source" --out "$icons/icon_${size}x${size}@2x.png" >/dev/null
  done
  iconutil -c icns "$icons" -o "$icns"
  rm -rf "$icons"
fi
cp "$icns" "$app/Contents/Resources/AppIcon.icns"
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key><string>$binary</string>
  <key>CFBundleIdentifier</key><string>$identifier</string>
  <key>CFBundleName</key><string>$name</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
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

# An ad-hoc signature pins the Keychain's "Always Allow" to one build's hash, so every rebuild asks
# again. Signing each build with one local certificate keeps the app's identity, and the grant.
if [ "$profile" = "debug" ]; then
  cert="T3 Code GPUI Dev Signing"
  if ! security find-identity -p codesigning 2>/dev/null | grep -q "\"$cert\""; then
    work="$(mktemp -d)"
    pass="$(openssl rand -hex 16)"
    cat > "$work/req.cnf" <<CNF
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = $cert
[ext]
basicConstraints = critical,CA:false
keyUsage = critical,digitalSignature
extendedKeyUsage = critical,codeSigning
CNF
    openssl req -x509 -newkey rsa:2048 -nodes -days 3650 -config "$work/req.cnf" \
      -keyout "$work/key.pem" -out "$work/cert.pem" >/dev/null 2>&1
    # macOS only reads the legacy SHA1/3DES PKCS12 encoding, not OpenSSL 3's default.
    openssl pkcs12 -export -inkey "$work/key.pem" -in "$work/cert.pem" -name "$cert" \
      -keypbe PBE-SHA1-3DES -certpbe PBE-SHA1-3DES -macalg sha1 \
      -passout "pass:$pass" -out "$work/cert.p12" >/dev/null 2>&1
    security import "$work/cert.p12" -P "$pass" -T /usr/bin/codesign >/dev/null
    rm -rf "$work"
  fi
  codesign --force --sign "$cert" --identifier "$identifier" "$app" >/dev/null 2>&1
fi

lsregister="/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister"
[ -x "$lsregister" ] && "$lsregister" -f "$app" >/dev/null 2>&1 || true

if [ "$action" = "run" ]; then
  if [ $# -ge 2 ]; then shift 2; else shift $#; fi
  exec "$app/Contents/MacOS/$binary" "$@"
fi
echo "$app"
