#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
RUNNER_SCRIPT="${ROOT}/scripts/runner/onebrain-soak-runner.sh"
TEMP_ROOT="$(mktemp -d)"
trap 'rm -rf -- "$TEMP_ROOT"' EXIT

FAKE_BIN="${TEMP_ROOT}/bin"
FAKE_HOME="${TEMP_ROOT}/home"
mkdir -p "$FAKE_BIN" "$FAKE_HOME"

cat >"${FAKE_BIN}/tool-stub" <<'EOF'
#!/usr/bin/env bash
set -Eeuo pipefail

tool="$(basename "$0")"
case "$tool" in
    uname)
        case "${1:-}" in
            -s) printf 'Darwin\n' ;;
            -m) printf 'arm64\n' ;;
            *) exit 2 ;;
        esac
        ;;
    sysctl)
        [[ "${1:-}" == "-n" && "${2:-}" == "hw.memsize" ]] || exit 2
        printf '17179869184\n'
        ;;
    xcode-select)
        [[ "${1:-}" == "--print-path" ]] || exit 2
        printf '/Library/Developer/CommandLineTools\n'
        ;;
    brew)
        printf 'fake brew %s\n' "$*"
        ;;
    curl)
        exit 0
        ;;
    *)
        exit 0
        ;;
esac
EOF
chmod +x "${FAKE_BIN}/tool-stub"

for command_name in \
    uname curl git tar gzip shasum python3 cc c++ make cmake pkg-config perl \
    xcode-select sysctl sw_vers caffeinate brew; do
    ln -s tool-stub "${FAKE_BIN}/${command_name}"
done

doctor_output="$(
    HOME="$FAKE_HOME" PATH="${FAKE_BIN}:${PATH}" \
        bash "$RUNNER_SCRIPT" doctor 2>&1
)"
grep -F "[onebrain-runner] Host: macOS ARM64 (Darwin/arm64)" \
    <<<"$doctor_output" >/dev/null
grep -F "[onebrain-runner] Doctor passed." <<<"$doctor_output" >/dev/null

status_output="$(
    HOME="$FAKE_HOME" PATH="${FAKE_BIN}:${PATH}" \
        bash "$RUNNER_SCRIPT" status 2>&1
)"
expected_runner_home="${FAKE_HOME}/.local/share/onebrain-actions-runner"
grep -F "[onebrain-runner] Runner home: ${expected_runner_home}" \
    <<<"$status_output" >/dev/null
[[ "$expected_runner_home" != *" "* ]]

deps_output="$(
    HOME="$FAKE_HOME" PATH="${FAKE_BIN}:${PATH}" \
        bash "$RUNNER_SCRIPT" deps 2>&1
)"
grep -F "[onebrain-runner] Installing build/runtime dependencies." \
    <<<"$deps_output" >/dev/null
grep -F "fake brew install python@3.13 cmake pkgconf" \
    <<<"$deps_output" >/dev/null

printf 'macOS ARM64 doctor/deps dispatch regression: OK\n'
