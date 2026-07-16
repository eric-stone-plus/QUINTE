#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
guard="$repo_root/scripts/release-guard.sh"
tmp="$(mktemp -d)"
trap '[[ -n "${KEEP_RELEASE_TEST_TMP:-}" ]] || rm -rf "$tmp"' EXIT

git -C "$tmp" init -q
git -C "$tmp" config user.name release-test
git -C "$tmp" config user.email release-test@example.invalid
printf '[package]\nname = "quinte"\nversion = "0.1.5"\n' >"$tmp/Cargo.toml"
printf '# lock fixture\n' >"$tmp/Cargo.lock"
printf 'fixture\n' >"$tmp/tracked"
git -C "$tmp" add .
git -C "$tmp" commit -qm fixture
candidate="$(git -C "$tmp" rev-parse HEAD)"
git -C "$tmp" update-ref refs/remotes/origin/main "$candidate"
mkdir -p "$tmp/.github" "$tmp/bin" "$tmp/api"
cp "$repo_root/.github/release-history.txt" "$tmp/.github/release-history.txt"

cat >"$tmp/bin/cargo" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
[[ "$*" == "metadata --locked --no-deps --format-version 1" ]]
printf '{"packages":[{"name":"quinte","version":"%s"}]}\n' "${FAKE_CARGO_VERSION:-0.1.5}"
EOF

cat >"$tmp/bin/fake-api" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
mode="$1"
case "$2" in
  */git/ref/tags/*) key=tag ;;
  */releases/tags/*) key=release ;;
  */git/matching-refs/*) key=tags ;;
  */releases?*) key=releases ;;
  */actions/workflows/*) key=runs ;;
  *) exit 99 ;;
esac
if [[ "$mode" == LIST ]]; then
  cat "$FAKE_API_ROOT/$key.json"
  list_status="$(cat "$FAKE_API_ROOT/$key.status")"
  [[ "$list_status" == 0 ]] && exit 0
  exit 1
fi
[[ "$mode" == LOOKUP ]]
status_file="$FAKE_API_ROOT/$key.status"
body_file="$FAKE_API_ROOT/$key.json"
status="$(cat "$status_file")"
if [[ "$status" == 0 ]]; then
  printf 'HTTP/2.0 200 OK\r\nContent-Type: application/json\r\n\r\n'
  cat "$body_file"
  exit 0
elif [[ "$status" == 44 ]]; then
  printf 'HTTP/2.0 404 Not Found\r\nContent-Type: application/json\r\n\r\n{"status":"404"}\n'
  exit 1
else
  printf 'HTTP/2.0 %s Error\r\nContent-Type: application/json\r\n\r\n{"status":"%s"}\n' "$status" "$status"
  exit 1
fi
EOF
chmod +x "$tmp/bin/cargo" "$tmp/bin/fake-api"

reset_api() {
  printf '44\n' >"$tmp/api/tag.status"
  printf '44\n' >"$tmp/api/release.status"
  printf '0\n' >"$tmp/api/tags.status"
  printf '0\n' >"$tmp/api/releases.status"
  printf '0\n' >"$tmp/api/runs.status"
  printf '[]\n' >"$tmp/api/tags.json"
  printf '[]\n' >"$tmp/api/releases.json"
  printf '[{"total_count":0,"workflow_runs":[]}]\n' >"$tmp/api/runs.json"
  printf '{}\n' >"$tmp/api/tag.json"
  printf '{}\n' >"$tmp/api/release.json"
}

run_guard() {
  (
    cd "$tmp"
    env \
      PATH="$tmp/bin:$PATH" \
      GITHUB_REPOSITORY=owner/quinte \
      GITHUB_RUN_ID="${TEST_RUN_ID:-700}" \
      RELEASE_GUARD_OFFLINE=1 \
      RELEASE_GUARD_FAKE_API="$tmp/bin/fake-api" \
      FAKE_API_ROOT="$tmp/api" \
      "$guard" "${1:-0.1.5}" "${2:-$candidate}"
  )
}

expect_pass() {
  local name="$1"
  shift
  if ! "$@" >"$tmp/command-output" 2>&1; then
    printf 'not ok - %s\n' "$name" >&2
    cat "$tmp/command-output" >&2
    exit 1
  fi
  printf 'ok - %s\n' "$name"
}

expect_fail() {
  local name="$1"
  shift
  if "$@" >"$tmp/command-output" 2>&1; then
    printf 'not ok - %s unexpectedly passed\n' "$name" >&2
    cat "$tmp/command-output" >&2
    exit 1
  fi
  printf 'ok - %s\n' "$name"
}

reset_api
expect_pass 'exact 404 absence is eligible' run_guard

for status in 401 403 409 422 429 500 503; do
  reset_api
  printf '%s\n' "$status" >"$tmp/api/tag.status"
  expect_fail "tag API status $status fails closed" run_guard
done

reset_api
printf '0\n' >"$tmp/api/tag.status"
printf '{"ref":"refs/tags/v0.1.5","object":{"sha":"%s"}}\n' "$candidate" >"$tmp/api/tag.json"
expect_fail 'existing tag fails even at the same candidate' run_guard

reset_api
printf '0\n' >"$tmp/api/tag.status"
printf '{}\n' >"$tmp/api/tag.json"
expect_fail 'malformed successful tag response fails closed' run_guard

reset_api
printf '43\n' >"$tmp/api/release.status"
expect_fail 'release API authorization error fails closed' run_guard

reset_api
printf '50\n' >"$tmp/api/runs.status"
expect_fail 'Actions API server error fails closed' run_guard

reset_api
printf '[[{"ref":"refs/tags/v0.1.7","object":{"sha":"%s"}}]]\n' "$candidate" >"$tmp/api/tags.json"
expect_fail 'higher historical tag blocks a lower request' run_guard

reset_api
printf '[[{"tag_name":"v0.1.5"}]]\n' >"$tmp/api/releases.json"
expect_fail 'historical release burns the same version' run_guard

reset_api
printf '{}\n' >"$tmp/api/runs.json"
expect_fail 'malformed Actions response fails closed' run_guard

reset_api
printf '[{"total_count":1,"workflow_runs":[{"id":699,"head_sha":"%s","display_title":"Release v0.1.5 from %s","event":"workflow_dispatch","conclusion":"cancelled"}]}]\n' "$candidate" "$candidate" >"$tmp/api/runs.json"
expect_fail 'cancelled prior attempt burns the candidate' run_guard

reset_api
other_sha="0000000000000000000000000000000000000000"
printf '[{"total_count":1,"workflow_runs":[{"id":699,"head_sha":"%s","display_title":"Release v0.1.5 from %s","event":"workflow_dispatch"}]}]\n' "$other_sha" "$other_sha" >"$tmp/api/runs.json"
expect_fail 'same version on another candidate is permanently burned' run_guard

reset_api
printf '[{"total_count":1,"workflow_runs":[{"id":699,"head_sha":"%s","display_title":"Release v0.1.7 from %s","event":"workflow_dispatch"}]}]\n' "$other_sha" "$other_sha" >"$tmp/api/runs.json"
expect_fail 'higher attempted version blocks a lower request' run_guard

reset_api
printf '[{"total_count":1,"workflow_runs":[{"id":699,"head_sha":"%s","event":"workflow_dispatch"}]}]\n' "$other_sha" >"$tmp/api/runs.json"
expect_fail 'malformed run entry fails closed' run_guard

reset_api
printf '[{"total_count":101,"workflow_runs":[]}]\n' >"$tmp/api/runs.json"
expect_fail 'incomplete Actions history fails closed' run_guard

reset_api
printf '[{"total_count":1,"workflow_runs":[{"id":700,"head_sha":"%s","display_title":"Release v0.1.5 from %s","event":"workflow_dispatch","status":"in_progress"}]}]\n' "$candidate" "$candidate" >"$tmp/api/runs.json"
expect_pass 'publish recheck ignores only its current run id' run_guard

reset_api
expect_fail 'burned version cannot be reused' run_guard 0.1.4 "$candidate"

reset_api
FAKE_CARGO_VERSION=0.1.6 expect_fail 'Cargo mismatch fails' run_guard

reset_api
expect_fail 'candidate must be a full commit SHA' run_guard 0.1.5 deadbeef

asset_dir="$tmp/assets"
mkdir "$asset_dir"
assets=(
  quinte-aarch64-apple-darwin.tar.gz
  quinte-aarch64-unknown-linux-gnu.tar.gz
  quinte-x86_64-apple-darwin.tar.gz
  quinte-x86_64-pc-windows-msvc.zip
  quinte-x86_64-unknown-linux-gnu.tar.gz
)
for asset in "${assets[@]}"; do printf '%s\n' "$asset" >"$asset_dir/$asset"; done
expect_pass 'exact five-archive set passes' "$guard" --assets "$asset_dir"
rm "$asset_dir/${assets[0]}"
expect_fail 'missing archive fails' "$guard" --assets "$asset_dir"
printf '%s\n' "${assets[0]}" >"$asset_dir/${assets[0]}"
printf 'extra\n' >"$asset_dir/quinte-extra.tar.gz"
expect_fail 'extra archive fails' "$guard" --assets "$asset_dir"
rm "$asset_dir/quinte-extra.tar.gz"
(
  cd "$asset_dir"
  LC_ALL=C sha256sum "${assets[@]}" > SHA256SUMS
)
expect_pass 'six-file checksummed set passes' "$guard" --assets "$asset_dir" --checksums
printf 'bad line\n' >>"$asset_dir/SHA256SUMS"
expect_fail 'malformed checksum set fails' "$guard" --assets "$asset_dir" --checksums

printf 'release guard fixture tests passed\n'
