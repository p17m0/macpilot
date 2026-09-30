#!/bin/zsh
# A stable signing identity for builds on your own Mac, so macOS remembers MacPilot's permissions
# (Full Disk Access, Documents/Desktop/Downloads, controlling Finder) across rebuilds.
#
# With the default ad-hoc signature macOS identifies the app by the hash of its binary, so every
# rebuild looks like a new app and every permission is asked again. A certificate — even a
# self-signed one — gives the app an identity that stays the same.
#
# The certificate lives in its own keychain file (~/Library/Keychains/macpilot-signing.keychain-db);
# your login keychain and trust settings are not touched. Delete that file to remove it.
#
# Prints: <identity name>\t<keychain path>
set -euo pipefail

NAME="MacPilot Local Signing"
KC="$HOME/Library/Keychains/macpilot-signing.keychain-db"
PW="macpilot-local" # protects only this throwaway signing key

if [[ ! -f $KC ]]; then
  TMP=$(mktemp -d)
  trap 'rm -rf "$TMP"' EXIT
  cat > "$TMP/cert.cnf" <<EOF
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = $NAME
[ext]
basicConstraints = critical, CA:false
keyUsage = critical, digitalSignature
extendedKeyUsage = critical, codeSigning
EOF
  openssl req -x509 -newkey rsa:2048 -nodes -days 3650 -config "$TMP/cert.cnf" \
    -keyout "$TMP/key.pem" -out "$TMP/cert.pem" >/dev/null 2>&1
  # -legacy: the macOS keychain cannot read the newer PKCS#12 encryption of OpenSSL 3.
  openssl pkcs12 -export -legacy -inkey "$TMP/key.pem" -in "$TMP/cert.pem" -name "$NAME" \
    -out "$TMP/id.p12" -passout "pass:$PW" 2>/dev/null \
    || openssl pkcs12 -export -inkey "$TMP/key.pem" -in "$TMP/cert.pem" -name "$NAME" -out "$TMP/id.p12" -passout "pass:$PW"
  security create-keychain -p "$PW" "$KC"
  security set-keychain-settings "$KC" # never lock automatically
  security unlock-keychain -p "$PW" "$KC"
  security import "$TMP/id.p12" -k "$KC" -P "$PW" -T /usr/bin/codesign >/dev/null
  security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$PW" "$KC" >/dev/null
fi
security unlock-keychain -p "$PW" "$KC"
printf '%s\t%s\n' "$NAME" "$KC"
