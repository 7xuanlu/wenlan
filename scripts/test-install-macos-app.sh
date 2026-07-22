#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd "$(dirname "$0")/.." && pwd)
INSTALLER="$ROOT_DIR/scripts/install-macos-app.sh"
TMP_DIR=$(mktemp -d "${TMPDIR:-/tmp}/wenlan-app-installer-test.XXXXXX")
trap 'rm -rf "$TMP_DIR"' EXIT

make_fixture() {
  local fixture_dir=$1
  local digest=$2

  mkdir -p "$fixture_dir/archive/Wenlan.app/Contents/MacOS"
  printf '#!/usr/bin/env bash\nexit 0\n' > "$fixture_dir/archive/Wenlan.app/Contents/MacOS/wenlan-app"
  chmod +x "$fixture_dir/archive/Wenlan.app/Contents/MacOS/wenlan-app"
  xattr -w com.apple.quarantine '0081;test;Codex;' "$fixture_dir/archive/Wenlan.app"
  xattr -w com.wenlan.test-marker 'preserve-me' "$fixture_dir/archive/Wenlan.app"
  tar -czf "$fixture_dir/Wenlan_aarch64.app.tar.gz" -C "$fixture_dir/archive" Wenlan.app

  local actual_digest
  actual_digest=$(shasum -a 256 "$fixture_dir/Wenlan_aarch64.app.tar.gz" | awk '{print $1}')
  if [[ "$digest" == "actual" ]]; then
    digest=$actual_digest
  fi

  cat > "$fixture_dir/release.json" <<JSON
{
  "tag_name": "v-test",
  "assets": [
    {
      "name": "Wenlan_aarch64.app.tar.gz",
      "browser_download_url": "file://$fixture_dir/Wenlan_aarch64.app.tar.gz",
      "digest": "sha256:$digest"
    }
  ]
}
JSON
}

test_installs_verified_app_without_quarantine() {
  local fixture_dir="$TMP_DIR/success"
  local install_dir="$fixture_dir/Applications"
  mkdir -p "$fixture_dir"
  make_fixture "$fixture_dir" actual

  WENLAN_APP_RELEASE_JSON_URL="file://$fixture_dir/release.json" \
    WENLAN_APP_INSTALL_DIR="$install_dir" \
    WENLAN_APP_NO_LAUNCH=1 \
    WENLAN_APP_SKIP_PLATFORM_CHECK=1 \
    bash "$INSTALLER"

  test -x "$install_dir/Wenlan.app/Contents/MacOS/wenlan-app"
  if xattr -p com.apple.quarantine "$install_dir/Wenlan.app" >/dev/null 2>&1; then
    echo "installed app still has com.apple.quarantine" >&2
    return 1
  fi
  if [[ $(xattr -p com.wenlan.test-marker "$install_dir/Wenlan.app") != preserve-me ]]; then
    echo "installer removed a non-quarantine extended attribute" >&2
    return 1
  fi
}

test_rejects_bad_digest_before_replacing_existing_app() {
  local fixture_dir="$TMP_DIR/bad-digest"
  local install_dir="$fixture_dir/Applications"
  mkdir -p "$fixture_dir" "$install_dir/Wenlan.app"
  printf 'keep me\n' > "$install_dir/Wenlan.app/existing-marker"
  make_fixture "$fixture_dir" "0000000000000000000000000000000000000000000000000000000000000000"

  if WENLAN_APP_RELEASE_JSON_URL="file://$fixture_dir/release.json" \
    WENLAN_APP_INSTALL_DIR="$install_dir" \
    WENLAN_APP_NO_LAUNCH=1 \
    WENLAN_APP_SKIP_PLATFORM_CHECK=1 \
    bash "$INSTALLER"; then
    echo "installer accepted a mismatched digest" >&2
    return 1
  fi

  test -f "$install_dir/Wenlan.app/existing-marker"
}

test_restores_existing_app_when_interrupted_after_backup() {
  local fixture_dir="$TMP_DIR/interrupted"
  local install_dir="$fixture_dir/Applications"
  local fake_bin="$fixture_dir/fake-bin"
  mkdir -p "$fixture_dir" "$install_dir/Wenlan.app" "$fake_bin"
  printf 'keep me\n' > "$install_dir/Wenlan.app/existing-marker"
  make_fixture "$fixture_dir" actual

  cat > "$fake_bin/mv" <<'SH'
#!/usr/bin/env bash
/bin/mv "$@"
case ${2:-} in
  *.Wenlan.app.backup.*) kill -TERM "$PPID" ;;
esac
SH
  chmod +x "$fake_bin/mv"

  if PATH="$fake_bin:$PATH" \
    WENLAN_APP_RELEASE_JSON_URL="file://$fixture_dir/release.json" \
    WENLAN_APP_INSTALL_DIR="$install_dir" \
    WENLAN_APP_NO_LAUNCH=1 \
    WENLAN_APP_SKIP_PLATFORM_CHECK=1 \
    bash "$INSTALLER"; then
    echo "interrupted installer unexpectedly succeeded" >&2
    return 1
  fi

  test -f "$install_dir/Wenlan.app/existing-marker"
  if compgen -G "$install_dir/.Wenlan.app.backup.*" >/dev/null; then
    echo "interrupted installer left a hidden backup behind" >&2
    return 1
  fi
}

test_installs_verified_app_without_quarantine
test_rejects_bad_digest_before_replacing_existing_app
test_restores_existing_app_when_interrupted_after_backup
echo "install-macos-app tests passed"
