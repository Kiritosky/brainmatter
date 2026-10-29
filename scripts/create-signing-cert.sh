#!/usr/bin/env bash
# Creates a self-signed code-signing identity in the login keychain.
#
# Why: Keychain item ACLs are bound to the app's code signature. Ad-hoc
# signatures change with every build, so macOS would ask for Keychain access
# again after each rebuild. Signing every build with one stable identity keeps
# the Touch ID key readable. No Apple Developer account needed.
#
# Usage: scripts/create-signing-cert.sh ["Brainmatter Dev"]
set -euo pipefail

NAME="${1:-Brainmatter Dev}"
KEYCHAIN="$HOME/Library/Keychains/login.keychain-db"
# Apple's LibreSSL writes PKCS#12 files that `security import` understands.
OPENSSL=/usr/bin/openssl

if security find-identity -p codesigning | grep -q "\"$NAME\""; then
  echo "Identity \"$NAME\" already exists."
  exit 0
fi

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

cat > "$TMP/cert.cnf" <<EOF
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = $NAME
[ext]
basicConstraints = critical,CA:false
keyUsage = critical,digitalSignature
extendedKeyUsage = critical,codeSigning
EOF

"$OPENSSL" req -x509 -newkey rsa:3072 -nodes -days 3650 -config "$TMP/cert.cnf" \
  -keyout "$TMP/key.pem" -out "$TMP/cert.pem" 2>/dev/null
PASS="$("$OPENSSL" rand -hex 16)"
"$OPENSSL" pkcs12 -export -inkey "$TMP/key.pem" -in "$TMP/cert.pem" \
  -name "$NAME" -out "$TMP/identity.p12" -passout "pass:$PASS"

security import "$TMP/identity.p12" -k "$KEYCHAIN" -P "$PASS" -T /usr/bin/codesign
# Trust for code signing only. macOS asks for your password or Touch ID here.
security add-trusted-cert -r trustRoot -p codeSign -k "$KEYCHAIN" "$TMP/cert.pem"

security find-identity -v -p codesigning | grep "$NAME"
echo "Done. Sign with: codesign -s \"$NAME\" -f <binary-or-app>"
