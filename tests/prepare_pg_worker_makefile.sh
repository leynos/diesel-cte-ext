#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT

fake_bin="$scratch/bin"
workspace="$scratch/workspace"
manifest_dir="$scratch/pg-embed-setup-unpriv"
log_file="$scratch/calls.log"
manifest_path="$manifest_dir/Cargo.toml"

mkdir -p "$fake_bin" "$workspace" "$manifest_dir"
touch "$log_file" "$manifest_path"

cat > "$fake_bin/cargo" << 'EOF'
#!/usr/bin/env bash
set -euo pipefail

printf 'cargo %s\n' "$*" >> "$PG_WORKER_TEST_LOG"

case "$1" in
  metadata)
    printf '{"packages":[{"name":"pg-embed-setup-unpriv","manifest_path":"%s"}]}\n' \
      "$PG_WORKER_TEST_MANIFEST"
    ;;
  build)
    manifest_path=
    profile=
    target_dir=
    while [ "$#" -gt 0 ]; do
      case "$1" in
        --manifest-path)
          shift
          manifest_path="$1"
          ;;
        --profile)
          shift
          profile="$1"
          ;;
        --target-dir)
          shift
          target_dir="$1"
          ;;
      esac
      shift
    done

    if [ ! -f "$manifest_path" ]; then
      printf 'manifest does not exist: %s\n' "$manifest_path" >&2
      exit 65
    fi

    if [ "${PG_WORKER_TEST_FAIL_BUILD:-}" = "1" ]; then
      printf 'forced pg_worker build failure\n' >&2
      exit 66
    fi

    case "$profile" in
      dev | test)
        build_dir=debug
        ;;
      release | bench)
        build_dir=release
        ;;
      *)
        build_dir="$profile"
        ;;
    esac

    mkdir -p "$target_dir/$build_dir"
    printf 'pg_worker built with profile %s\n' "$profile" > "$target_dir/$build_dir/pg_worker"
    chmod 0755 "$target_dir/$build_dir/pg_worker"
    ;;
  *)
    printf 'unexpected cargo invocation: %s\n' "$*" >&2
    exit 64
    ;;
esac
EOF

cat > "$fake_bin/jq" << 'EOF'
#!/usr/bin/env bash
set -euo pipefail

cat >/dev/null
if [ "${PG_WORKER_TEST_EMPTY_MANIFEST:-}" = "1" ]; then
  exit 0
fi
if [ "${PG_WORKER_TEST_JQ_FAIL:-}" = "1" ]; then
  exit 22
fi
printf '%s\n' "${PG_WORKER_TEST_MANIFEST_OUTPUT:-$PG_WORKER_TEST_MANIFEST}"
EOF

cat > "$fake_bin/install" << 'EOF'
#!/usr/bin/env bash
set -euo pipefail

if [ "$1" = "-m" ]; then
  mode="$2"
  shift 2
else
  mode=0755
fi

source_path="$1"
destination_path="$2"

printf 'install %s %s\n' "$source_path" "$destination_path" >> "$PG_WORKER_TEST_LOG"
cp "$source_path" "$destination_path"
chmod "$mode" "$destination_path"
EOF

chmod 0755 "$fake_bin/cargo" "$fake_bin/jq" "$fake_bin/install"

run_make() {
  PATH="$fake_bin:$PATH" \
    PG_WORKER_TEST_LOG="$log_file" \
    PG_WORKER_TEST_MANIFEST="$manifest_path" \
    PG_WORKER_TEST_MANIFEST_OUTPUT="${PG_WORKER_TEST_MANIFEST_OUTPUT:-}" \
    PG_WORKER_TEST_EMPTY_MANIFEST="${PG_WORKER_TEST_EMPTY_MANIFEST:-}" \
    PG_WORKER_TEST_JQ_FAIL="${PG_WORKER_TEST_JQ_FAIL:-}" \
    PG_WORKER_TEST_FAIL_BUILD="${PG_WORKER_TEST_FAIL_BUILD:-}" \
    make -f "$repo_root/Makefile" CURDIR="$workspace" "$@"
}

assert_log_contains() {
  local expected="$1"
  if ! grep -F -- "$expected" "$log_file" >/dev/null; then
    printf 'expected log to contain: %s\n' "$expected" >&2
    printf 'actual log:\n' >&2
    cat "$log_file" >&2
    exit 1
  fi
}

assert_log_excludes() {
  local unexpected="$1"
  if grep -F -- "$unexpected" "$log_file" >/dev/null; then
    printf 'expected log to exclude: %s\n' "$unexpected" >&2
    printf 'actual log:\n' >&2
    cat "$log_file" >&2
    exit 1
  fi
}

assert_profile_installs_from() {
  local profile="$1"
  local expected_build_dir="$2"
  local output_path="$scratch/output/$profile/pg_worker"

  : > "$log_file"
  mkdir -p "$(dirname "$output_path")"
  run_make PG_WORKER_PROFILE="$profile" PG_WORKER_PATH="$output_path" prepare-pg-worker >/dev/null

  test -x "$output_path"
  assert_log_contains "cargo build --locked --manifest-path $manifest_path --bin pg_worker --profile $profile"
  assert_log_contains "install $workspace/target/$expected_build_dir/pg_worker $output_path"
}

assert_default_profile_installs_from_debug() {
  local output_path="$scratch/output/default/pg_worker"

  : > "$log_file"
  mkdir -p "$(dirname "$output_path")"
  run_make PG_WORKER_PATH="$output_path" prepare-pg-worker >/dev/null

  test -x "$output_path"
  assert_log_contains "cargo build --locked --manifest-path $manifest_path --bin pg_worker --profile dev"
  assert_log_contains "install $workspace/target/debug/pg_worker $output_path"
}

assert_empty_manifest_stops_before_build() {
  local output_path="$scratch/output/empty-manifest/pg_worker"

  : > "$log_file"
  mkdir -p "$(dirname "$output_path")"
  if PG_WORKER_TEST_EMPTY_MANIFEST=1 run_make PG_WORKER_PATH="$output_path" prepare-pg-worker >/dev/null 2>&1; then
    printf 'expected prepare-pg-worker to fail with an empty manifest_path\n' >&2
    exit 1
  fi

  assert_log_contains 'cargo metadata --format-version 1 --locked'
  assert_log_excludes 'cargo build'
  assert_log_excludes 'install '
  test ! -e "$output_path"
}

assert_bad_jq_lookup_stops_before_build() {
  local output_path="$scratch/output/bad-jq/pg_worker"

  : > "$log_file"
  mkdir -p "$(dirname "$output_path")"
  if PG_WORKER_TEST_JQ_FAIL=1 run_make PG_WORKER_PATH="$output_path" prepare-pg-worker >/dev/null 2>&1; then
    printf 'expected prepare-pg-worker to fail when jq lookup fails\n' >&2
    exit 1
  fi

  assert_log_contains 'cargo metadata --format-version 1 --locked'
  assert_log_excludes 'cargo build'
  assert_log_excludes 'install '
  test ! -e "$output_path"
}

assert_missing_manifest_stops_before_install() {
  local missing_manifest="$scratch/missing/Cargo.toml"
  local output_path="$scratch/output/missing-manifest/pg_worker"

  : > "$log_file"
  mkdir -p "$(dirname "$output_path")"
  if PG_WORKER_TEST_MANIFEST_OUTPUT="$missing_manifest" \
    run_make PG_WORKER_PATH="$output_path" prepare-pg-worker >/dev/null 2>&1; then
    printf 'expected prepare-pg-worker to fail when manifest_path is missing\n' >&2
    exit 1
  fi

  assert_log_contains "cargo build --locked --manifest-path $missing_manifest --bin pg_worker --profile dev"
  assert_log_excludes 'install '
  test ! -e "$output_path"
}

assert_failed_build_stops_before_install() {
  local output_path="$scratch/output/failed-build/pg_worker"

  : > "$log_file"
  mkdir -p "$(dirname "$output_path")"
  if PG_WORKER_TEST_FAIL_BUILD=1 run_make PG_WORKER_PATH="$output_path" prepare-pg-worker >/dev/null 2>&1; then
    printf 'expected prepare-pg-worker to fail when cargo build fails\n' >&2
    exit 1
  fi

  assert_log_contains "cargo build --locked --manifest-path $manifest_path --bin pg_worker --profile dev"
  assert_log_excludes 'install '
  test ! -e "$output_path"
}

assert_default_profile_installs_from_debug
assert_profile_installs_from release release
assert_profile_installs_from test debug
assert_profile_installs_from bench release
assert_profile_installs_from custom custom
assert_empty_manifest_stops_before_build
assert_bad_jq_lookup_stops_before_build
assert_missing_manifest_stops_before_install
assert_failed_build_stops_before_install
