#!/usr/bin/env bash
# Sets the four GitHub secrets the release workflow signs the APK with (P-131),
# from one keystore file and one password, after checking locally that the
# password opens that keystore. Nothing is echoed or written to disk.
#
#   packages/mobile/set-signing-secrets.sh [keystore] [alias]
#     keystore  default: ~/moonkale-release.jks
#     alias     default: moonkale
#
# Needs keytool (a JDK) and gh logged in with access to the repository. For a
# PKCS12 keystore the key password is the store password, so one is asked for.
set -euo pipefail

KS="${1:-$HOME/moonkale-release.jks}"
ALIAS="${2:-moonkale}"
REPO="${MOONKALE_REPO:-MathStruct/Moonkale}"

[ -f "$KS" ] || { echo "no keystore at $KS" >&2; exit 1; }

read -r -s -p "Password for $KS: " PW
echo

# The same check the workflow runs, with the password from the environment.
export KS_PW="$PW"
if ! keytool -list -keystore "$KS" -storepass:env KS_PW -alias "$ALIAS" >/dev/null 2>&1; then
  echo "that password does not open $KS, or it has no key named '$ALIAS' — nothing was changed" >&2
  echo "(list the aliases with: keytool -list -keystore $KS)" >&2
  exit 1
fi
echo "ok: the password opens $KS and it holds '$ALIAS'"
# The certificate every released APK will carry; later releases must match it.
keytool -list -v -keystore "$KS" -storepass:env KS_PW -alias "$ALIAS" 2>/dev/null | grep -m1 "SHA256:" || true

# printf without a newline: the secrets hold exactly these bytes.
base64 -w0 "$KS" | gh secret set -R "$REPO" ANDROID_KEYSTORE_BASE64
printf '%s' "$PW" | gh secret set -R "$REPO" ANDROID_KEYSTORE_PASSWORD
printf '%s' "$PW" | gh secret set -R "$REPO" ANDROID_KEY_PASSWORD
printf '%s' "$ALIAS" | gh secret set -R "$REPO" ANDROID_KEY_ALIAS
unset PW KS_PW

echo "set ANDROID_KEYSTORE_BASE64, ANDROID_KEYSTORE_PASSWORD, ANDROID_KEY_PASSWORD and ANDROID_KEY_ALIAS on $REPO"
