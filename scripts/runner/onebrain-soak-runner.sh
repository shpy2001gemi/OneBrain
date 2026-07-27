#!/usr/bin/env bash
set -Eeuo pipefail

# Portable GitHub Actions runner for OneBrain M5-07 soak jobs.
#
# The default setup is ephemeral: the runner accepts one job, unregisters
# itself automatically, and exits. No system service is installed.

HOST_OS="$(uname -s)"
HOST_ARCH="$(uname -m)"
HOST_KIND="unsupported"
RUNNER_ASSET_ID=""
RUNNER_DISPLAY_NAME=""
DEFAULT_RUNNER_HOME="${HOME}/.local/share/onebrain-actions-runner"
DEFAULT_RUNNER_LABELS="onebrain-soak"

case "${HOST_OS}/${HOST_ARCH}" in
    Linux/x86_64 | Linux/amd64)
        HOST_KIND="linux-x64"
        RUNNER_ASSET_ID="linux-x64"
        RUNNER_DISPLAY_NAME="Linux x64"
        ;;
    Darwin/arm64 | Darwin/aarch64)
        HOST_KIND="macos-arm64"
        RUNNER_ASSET_ID="osx-arm64"
        RUNNER_DISPLAY_NAME="macOS ARM64"
        DEFAULT_RUNNER_LABELS="onebrain-soak-macos-arm64"
        if [[ -x /opt/homebrew/bin/brew ]]; then
            export PATH="/opt/homebrew/bin:${PATH}"
        fi
        ;;
esac

REPOSITORY_URL="${ONEBRAIN_RUNNER_REPOSITORY_URL:-https://github.com/shpy2001gemi/OneBrain}"
RUNNER_HOME="${ONEBRAIN_RUNNER_HOME:-${DEFAULT_RUNNER_HOME}}"
RUNNER_NAME="${ONEBRAIN_RUNNER_NAME:-onebrain-soak-$(hostname -s)}"
RUNNER_LABELS="${ONEBRAIN_RUNNER_LABELS:-${DEFAULT_RUNNER_LABELS}}"
PID_FILE="${RUNNER_HOME}/.onebrain-runner.pid"
LOG_FILE="${RUNNER_HOME}/.onebrain-runner.log"
MODE_FILE="${RUNNER_HOME}/.onebrain-runner-mode"

info() {
    printf '[onebrain-runner] %s\n' "$*"
}

warn() {
    printf '[onebrain-runner] WARNING: %s\n' "$*" >&2
}

die() {
    printf '[onebrain-runner] ERROR: %s\n' "$*" >&2
    exit 1
}

usage() {
    cat <<'EOF'
OneBrain portable soak runner

Usage:
  onebrain-soak-runner.sh                 Interactive menu
  onebrain-soak-runner.sh doctor          Check OS, tools, resources and network
  onebrain-soak-runner.sh deps            Install/check build dependencies
  onebrain-soak-runner.sh setup           Configure an ephemeral one-job runner
  onebrain-soak-runner.sh setup --persistent
                                         Configure a reusable stopped runner
  onebrain-soak-runner.sh setup-run       Configure ephemeral runner and run now
  onebrain-soak-runner.sh run             Run registered runner in foreground
  onebrain-soak-runner.sh start           Start registered runner in background
  onebrain-soak-runner.sh stop            Gracefully stop background runner
  onebrain-soak-runner.sh status          Show local runner status
  onebrain-soak-runner.sh logs            Follow background runner log
  onebrain-soak-runner.sh remove          Deregister runner from GitHub
  onebrain-soak-runner.sh purge           Delete local portable runner files
  onebrain-soak-runner.sh uninstall       Deregister and delete local files

Optional environment:
  ONEBRAIN_RUNNER_HOME
  ONEBRAIN_RUNNER_NAME
  ONEBRAIN_RUNNER_REPOSITORY_URL
  ONEBRAIN_RUNNER_LABELS
  ONEBRAIN_RUNNER_TOKEN          Short-lived registration token
  ONEBRAIN_RUNNER_REMOVE_TOKEN   Short-lived removal token
  GITHUB_TOKEN                   Optional token for release metadata rate limit

Supported hosts:
  Linux x64
  macOS ARM64 / Apple Silicon (M1 or later)

No inbound firewall port is required. The runner and M5-07 loopback QUIC
workload only require outbound HTTPS (TCP 443) to GitHub. On macOS the runner
uses caffeinate while active so the machine does not sleep during a soak.
EOF
}

require_supported_host() {
    [[ "$HOST_KIND" != "unsupported" ]] ||
        die "Supported hosts are Linux x64 and native macOS ARM64; found ${HOST_OS}/${HOST_ARCH}."
}

require_supported_distribution() {
    [[ "$HOST_KIND" == "linux-x64" ]] || return 0
    [[ -r /etc/os-release ]] || {
        warn "Cannot read /etc/os-release; verify the distribution against GitHub's supported runner list."
        return
    }

    local ID="" ID_LIKE="" VERSION_ID="" PRETTY_NAME=""
    # shellcheck disable=SC1091
    source /etc/os-release
    local distribution_identity=" ${ID:-} ${ID_LIKE:-} "
    local version_major="${VERSION_ID%%.*}"

    if [[ "$distribution_identity" == *" rhel "* ||
          "$distribution_identity" == *" centos "* ||
          "$distribution_identity" == *" rocky "* ||
          "$distribution_identity" == *" almalinux "* ||
          "$distribution_identity" == *" ol "* ]]; then
        if [[ "$version_major" =~ ^[0-9]+$ ]] && ((version_major < 8)); then
            die "${PRETTY_NAME:-This distribution} is unsupported. GitHub Actions requires CentOS/RHEL 8 or later; migrate this server instead of using archived EOL repositories."
        fi
    fi
}

require_non_root() {
    if [[ "$(id -u)" -eq 0 ]]; then
        die "Do not run the Actions runner as root. Use a dedicated unprivileged user."
    fi
}

command_exists() {
    command -v "$1" >/dev/null 2>&1
}

activate_macos_tool_paths() {
    [[ "$HOST_KIND" == "macos-arm64" ]] || return 0
    if [[ -x /opt/homebrew/bin/brew ]]; then
        export PATH="/opt/homebrew/bin:${PATH}"
        local python_prefix
        python_prefix="$(/opt/homebrew/bin/brew --prefix python@3.13 2>/dev/null || true)"
        if [[ -n "$python_prefix" ]]; then
            export PATH="${python_prefix}/libexec/bin:${PATH}"
        fi
    fi
}

resolve_path() {
    local path="$1"
    if [[ "$HOST_KIND" == "linux-x64" ]] && command_exists realpath; then
        realpath -m "$path"
    else
        python3 -c 'import os, sys; print(os.path.realpath(sys.argv[1]))' "$path"
    fi
}

network_probe() {
    local url="$1"
    if curl --silent --show-error --location --head \
        --connect-timeout 10 --max-time 20 "$url" >/dev/null; then
        printf '  OK   %s\n' "$url"
        return 0
    fi
    printf '  FAIL %s\n' "$url" >&2
    return 1
}

doctor() {
    require_supported_host
    require_supported_distribution
    activate_macos_tool_paths
    local failed=0
    local command_name
    local required_commands=()
    if [[ "$HOST_KIND" == "linux-x64" ]]; then
        required_commands=(
            curl git tar gzip sha256sum realpath python3
            cc c++ make cmake pkg-config perl
        )
    else
        required_commands=(
            curl git tar gzip shasum python3
            cc c++ make cmake pkg-config perl
            xcode-select sysctl sw_vers caffeinate
        )
    fi

    info "Host: ${RUNNER_DISPLAY_NAME} (${HOST_OS}/${HOST_ARCH})"
    info "Checking required commands"
    for command_name in "${required_commands[@]}"; do
        if command_exists "$command_name"; then
            printf '  OK   %s\n' "$command_name"
        else
            printf '  FAIL %s\n' "$command_name" >&2
            failed=1
        fi
    done
    if [[ "$failed" -ne 0 ]]; then
        warn "Run '$0 deps', then rerun doctor."
    fi

    local memory_kib
    if [[ "$HOST_KIND" == "linux-x64" ]]; then
        memory_kib="$(awk '/^MemTotal:/ {print $2}' /proc/meminfo 2>/dev/null || true)"
    else
        local memory_bytes
        memory_bytes="$(sysctl -n hw.memsize 2>/dev/null || true)"
        if [[ "$memory_bytes" =~ ^[0-9]+$ ]]; then
            memory_kib=$((memory_bytes / 1024))
        else
            memory_kib=""
        fi
    fi
    if [[ -n "$memory_kib" ]]; then
        local memory_gib=$((memory_kib / 1024 / 1024))
        info "Memory: ${memory_gib} GiB"
        if ((memory_kib < 6 * 1024 * 1024)); then
            warn "Less than 6 GiB RAM; release linking may be slow or fail."
        fi
    fi

    local disk_probe
    disk_probe="$(dirname "$RUNNER_HOME")"
    while [[ ! -e "$disk_probe" && "$disk_probe" != "/" ]]; do
        disk_probe="$(dirname "$disk_probe")"
    done
    local available_kib
    available_kib="$(df -Pk "$disk_probe" | awk 'NR == 2 {print $4}')"
    local available_gib=$((available_kib / 1024 / 1024))
    info "Free disk near runner home: ${available_gib} GiB"
    if ((available_kib < 30 * 1024 * 1024)); then
        warn "Less than 30 GiB free; use at least 50 GiB for build cache and soak evidence."
    fi

    if [[ "$HOST_KIND" == "linux-x64" ]] && command_exists timedatectl; then
        local ntp_state
        ntp_state="$(timedatectl show -p NTPSynchronized --value 2>/dev/null || true)"
        [[ "$ntp_state" == "yes" ]] || warn "System clock is not reported as NTP-synchronized."
    fi

    if [[ "$HOST_KIND" == "macos-arm64" ]] &&
        ! xcode-select --print-path >/dev/null 2>&1; then
        warn "Xcode Command Line Tools are not installed; run '$0 deps'."
        failed=1
    fi

    command_exists curl || return 1
    info "Checking outbound HTTPS; no inbound port is used"
    network_probe "https://github.com" || failed=1
    network_probe "https://api.github.com" || failed=1
    network_probe "https://codeload.github.com" || failed=1
    network_probe "https://release-assets.githubusercontent.com" || failed=1
    network_probe "https://results-receiver.actions.githubusercontent.com" || failed=1

    if [[ "$failed" -ne 0 ]]; then
        die "Doctor found blocking prerequisites."
    fi
    info "Doctor passed."
}

run_privileged() {
    if [[ "$(id -u)" -eq 0 ]]; then
        "$@"
    elif command_exists sudo; then
        sudo "$@"
    else
        die "Dependency installation requires root or sudo. The runner itself must still run as a non-root user."
    fi
}

install_dependencies() {
    require_supported_host
    require_supported_distribution
    info "Installing build/runtime dependencies. The Actions runner itself remains portable."
    if [[ "$HOST_KIND" == "macos-arm64" ]]; then
        require_non_root
        if ! xcode-select --print-path >/dev/null 2>&1; then
            info "Opening Apple's Xcode Command Line Tools installer."
            xcode-select --install || true
            die "Complete the Apple installer, then rerun '$0 deps'."
        fi
        activate_macos_tool_paths
        command_exists brew ||
            die "Homebrew is required for Python, CMake and pkgconf. Install it from https://brew.sh, then rerun '$0 deps'."
        brew install python@3.13 cmake pkgconf
        activate_macos_tool_paths
    elif command_exists apt-get; then
        run_privileged apt-get update
        run_privileged apt-get install -y \
            ca-certificates curl git tar gzip coreutils python3 \
            build-essential cmake pkg-config perl libssl-dev libicu-dev zlib1g-dev
    elif command_exists dnf; then
        run_privileged dnf install -y \
            ca-certificates curl git tar gzip coreutils python3 \
            gcc gcc-c++ make cmake pkgconf-pkg-config perl \
            openssl-devel libicu-devel zlib-devel
    elif command_exists yum; then
        run_privileged yum install -y \
            ca-certificates curl git tar gzip coreutils python3 \
            gcc gcc-c++ make cmake pkgconfig perl \
            openssl-devel libicu-devel zlib-devel
    else
        die "Unsupported package manager. Install the commands listed by doctor manually."
    fi
}

github_api_curl_args() {
    GITHUB_API_CURL_ARGS=(
        --fail --silent --show-error --location
        --retry 3 --retry-all-errors
        --header "Accept: application/vnd.github+json"
        --header "X-GitHub-Api-Version: 2022-11-28"
        --header "User-Agent: OneBrain-Soak-Runner-Kit"
    )
    if [[ -n "${GITHUB_TOKEN:-}" ]]; then
        GITHUB_API_CURL_ARGS+=(--header "Authorization: Bearer ${GITHUB_TOKEN}")
    fi
}

download_runner() {
    require_supported_host
    activate_macos_tool_paths
    if [[ -x "${RUNNER_HOME}/config.sh" ]]; then
        info "Portable runner already downloaded at ${RUNNER_HOME}."
        return
    fi
    if [[ -e "$RUNNER_HOME" ]]; then
        die "${RUNNER_HOME} exists but is not a complete runner. Move it aside or run purge after inspecting it."
    fi

    github_api_curl_args
    info "Resolving the latest official actions/runner ${RUNNER_DISPLAY_NAME} release"
    local release_json
    release_json="$(curl "${GITHUB_API_CURL_ARGS[@]}" \
        "https://api.github.com/repos/actions/runner/releases/latest")"

    local metadata
    metadata="$(
        printf '%s' "$release_json" | python3 -c '
import json
import re
import sys

release = json.load(sys.stdin)
asset_id = sys.argv[1]
display_name = sys.argv[2]
assets = [
    asset for asset in release.get("assets", [])
    if re.fullmatch(
        rf"actions-runner-{re.escape(asset_id)}-[0-9.]+\.tar\.gz",
        asset.get("name", ""),
    )
]
if len(assets) != 1:
    raise SystemExit(f"expected exactly one {display_name} runner archive")
asset = assets[0]
digest = asset.get("digest") or ""
print("\t".join([
    release.get("tag_name", ""),
    asset.get("browser_download_url", ""),
    digest,
]))
' "$RUNNER_ASSET_ID" "$RUNNER_DISPLAY_NAME"
    )"
    local version=""
    local download_url=""
    local digest=""
    IFS=$'\t' read -r version download_url digest <<<"$metadata"
    local expected_sha="${digest#sha256:}"
    [[ "$version" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "Invalid runner release tag: ${version}"
    [[ "$download_url" == https://github.com/actions/runner/releases/download/* ]] ||
        die "Unexpected runner download URL."
    [[ "$expected_sha" =~ ^[0-9a-f]{64}$ ]] ||
        die "GitHub release metadata did not provide a SHA-256 digest."

    local temp_root
    temp_root="$(mktemp -d)"
    local archive="${temp_root}/actions-runner.tar.gz"
    local extracted="${temp_root}/runner"
    mkdir -p "$extracted"

    info "Downloading actions/runner ${version}"
    if ! curl --fail --silent --show-error --location \
        --retry 3 --retry-all-errors \
        --output "$archive" "$download_url"; then
        rm -rf -- "$temp_root"
        die "Runner download failed."
    fi
    local actual_sha=""
    if [[ "$HOST_KIND" == "linux-x64" ]]; then
        if printf '%s  %s\n' "$expected_sha" "$archive" | sha256sum --check --status; then
            actual_sha="$expected_sha"
        fi
    else
        actual_sha="$(shasum -a 256 "$archive" | awk '{print $1}')"
    fi
    if [[ "$actual_sha" != "$expected_sha" ]]; then
        rm -rf -- "$temp_root"
        die "Runner archive SHA-256 verification failed."
    fi
    if ! tar -xzf "$archive" -C "$extracted"; then
        rm -rf -- "$temp_root"
        die "Runner archive extraction failed."
    fi

    mkdir -p "$(dirname "$RUNNER_HOME")"
    mv "$extracted" "$RUNNER_HOME"
    chmod 700 "$RUNNER_HOME"
    rm -rf -- "$temp_root"
    info "Verified runner installed portably at ${RUNNER_HOME}."
}

read_secret() {
    local environment_name="$1"
    local prompt="$2"
    local current_value="${!environment_name:-}"
    if [[ -n "$current_value" ]]; then
        SECRET_VALUE="$current_value"
        return
    fi
    [[ -t 0 ]] || die "${environment_name} is required in non-interactive mode."
    read -r -s -p "$prompt" SECRET_VALUE
    printf '\n'
    [[ -n "$SECRET_VALUE" ]] || die "Token cannot be empty."
}

is_background_running() {
    [[ -f "$PID_FILE" ]] || return 1
    local pid
    pid="$(cat "$PID_FILE" 2>/dev/null || true)"
    [[ "$pid" =~ ^[0-9]+$ ]] || return 1
    kill -0 "$pid" 2>/dev/null
}

require_configured() {
    [[ -f "${RUNNER_HOME}/.runner" ]] ||
        die "Runner is not configured. Run '$0 setup' first."
}

setup_runner() {
    local mode="${1:-ephemeral}"
    require_supported_host
    require_non_root
    [[ "$mode" == "ephemeral" || "$mode" == "persistent" ]] ||
        die "Unknown runner mode: ${mode}"
    doctor
    download_runner
    if [[ -f "${RUNNER_HOME}/.runner" ]]; then
        die "Runner is already configured. Use status/run, or remove it before reconfiguration."
    fi

    read_secret ONEBRAIN_RUNNER_TOKEN \
        "Paste the short-lived registration token from GitHub Settings > Actions > Runners: "
    local config_args=(
        --unattended
        --replace
        --url "$REPOSITORY_URL"
        --token "$SECRET_VALUE"
        --name "$RUNNER_NAME"
        --labels "$RUNNER_LABELS"
        --work "_work"
    )
    if [[ "$mode" == "ephemeral" ]]; then
        config_args+=(--ephemeral)
    fi

    umask 077
    (
        cd "$RUNNER_HOME"
        ./config.sh "${config_args[@]}"
    )
    SECRET_VALUE=""
    unset ONEBRAIN_RUNNER_TOKEN || true
    printf '%s\n' "$mode" >"$MODE_FILE"
    info "Configured ${mode} runner '${RUNNER_NAME}' with label '${RUNNER_LABELS}'."
    if [[ "$mode" == "ephemeral" ]]; then
        info "It will accept one job, deregister automatically, and exit."
    else
        info "It remains stopped until run/start is invoked."
    fi
}

run_foreground() {
    require_supported_host
    require_non_root
    require_configured
    is_background_running && die "Background runner is already active."
    rm -f "$PID_FILE"
    umask 077
    printf '%s\n' "$$" >"$PID_FILE"
    info "Running in foreground. Ctrl+C stops the listener; an active job will fail if interrupted."
    cd "$RUNNER_HOME"
    if [[ "$HOST_KIND" == "macos-arm64" ]]; then
        exec caffeinate -dimsu ./run.sh
    fi
    exec ./run.sh
}

start_background() {
    require_supported_host
    require_non_root
    require_configured
    if is_background_running; then
        info "Runner is already running with PID $(cat "$PID_FILE")."
        return
    fi
    rm -f "$PID_FILE"
    umask 077
    (
        cd "$RUNNER_HOME"
        if [[ "$HOST_KIND" == "macos-arm64" ]]; then
            nohup ./run.sh >>"$LOG_FILE" 2>&1 &
            local runner_pid="$!"
            nohup caffeinate -dimsu -w "$runner_pid" >>"$LOG_FILE" 2>&1 &
            printf '%s\n' "$runner_pid" >"$PID_FILE"
        else
            nohup ./run.sh >>"$LOG_FILE" 2>&1 &
            printf '%s\n' "$!" >"$PID_FILE"
        fi
    )
    sleep 2
    if ! is_background_running; then
        rm -f "$PID_FILE"
        die "Runner exited during startup. Inspect ${LOG_FILE}."
    fi
    info "Runner started in background with PID $(cat "$PID_FILE")."
    info "Log: ${LOG_FILE}"
}

confirm_stop() {
    [[ -t 0 ]] || die "Refusing non-interactive stop. Run from a terminal."
    warn "Stopping during a 24h/72h job makes that job fail."
    local answer
    read -r -p "Type STOP to continue: " answer
    [[ "$answer" == "STOP" ]] || die "Stop cancelled."
}

stop_background() {
    if ! is_background_running; then
        rm -f "$PID_FILE"
        info "No background runner process is active."
        return
    fi
    confirm_stop
    local pid
    pid="$(cat "$PID_FILE")"
    info "Requesting graceful stop for PID ${pid}"
    kill -INT "$pid" 2>/dev/null || true
    local attempt
    for ((attempt = 0; attempt < 30; attempt++)); do
        if ! kill -0 "$pid" 2>/dev/null; then
            rm -f "$PID_FILE"
            info "Runner stopped."
            return
        fi
        sleep 1
    done
    warn "Runner did not exit after 30 seconds; sending TERM."
    kill -TERM "$pid" 2>/dev/null || true
    for ((attempt = 0; attempt < 10; attempt++)); do
        if ! kill -0 "$pid" 2>/dev/null; then
            rm -f "$PID_FILE"
            info "Runner stopped."
            return
        fi
        sleep 1
    done
    die "Runner is still active. Inspect the process before taking stronger action."
}

show_status() {
    info "Host: ${RUNNER_DISPLAY_NAME} (${HOST_OS}/${HOST_ARCH})"
    info "Repository: ${REPOSITORY_URL}"
    info "Runner home: ${RUNNER_HOME}"
    info "Runner name: ${RUNNER_NAME}"
    info "Custom labels: ${RUNNER_LABELS}"
    if [[ -f "${RUNNER_HOME}/.runner" ]]; then
        info "Registration: configured ($(cat "$MODE_FILE" 2>/dev/null || printf 'unknown') mode)"
    else
        info "Registration: not configured"
    fi
    if is_background_running; then
        info "Tracked runner process: running (PID $(cat "$PID_FILE"))"
    else
        rm -f "$PID_FILE"
        info "Tracked runner process: stopped"
    fi
    info "GitHub status: ${REPOSITORY_URL}/settings/actions/runners"
}

follow_logs() {
    [[ -f "$LOG_FILE" ]] || die "No background log exists at ${LOG_FILE}."
    exec tail -n 100 -f "$LOG_FILE"
}

remove_registration() {
    require_supported_host
    require_non_root
    [[ -x "${RUNNER_HOME}/config.sh" ]] || die "Runner files are not present."
    is_background_running && die "Stop the background runner before removing registration."
    if [[ ! -f "${RUNNER_HOME}/.runner" ]]; then
        info "No local registration is present. It may already have auto-deregistered."
        return
    fi
    read_secret ONEBRAIN_RUNNER_REMOVE_TOKEN \
        "Paste the short-lived removal token from the runner's GitHub Remove page: "
    (
        cd "$RUNNER_HOME"
        ./config.sh remove --token "$SECRET_VALUE"
    )
    SECRET_VALUE=""
    unset ONEBRAIN_RUNNER_REMOVE_TOKEN || true
    rm -f "$MODE_FILE" "$PID_FILE"
    info "Runner deregistered from GitHub."
}

validate_purge_target() {
    local resolved
    resolved="$(resolve_path "$RUNNER_HOME")"
    case "$resolved" in
        / | /home | /root | /usr | /opt | /var | "$HOME")
            die "Unsafe purge target: ${resolved}"
            ;;
    esac
    if [[ "$HOST_KIND" == "macos-arm64" ]]; then
        [[ "$resolved" == "$HOME/"* ]] ||
            die "macOS purge target must be below HOME: ${resolved}"
    else
        [[ "$resolved" == "$HOME/"* || "$resolved" == /opt/* || "$resolved" == /srv/* ]] ||
            die "Purge target must be below HOME, /opt, or /srv: ${resolved}"
    fi
}

purge_local() {
    is_background_running && die "Stop the runner before purging local files."
    [[ -e "$RUNNER_HOME" ]] || {
        info "No local runner directory exists."
        return
    }
    validate_purge_target
    warn "This deletes runner binaries, workspaces, caches and local soak data."
    warn "If GitHub still lists the runner, use remove first or delete it in repository Settings."
    [[ -t 0 ]] || die "Refusing non-interactive purge."
    local answer
    read -r -p "Type PURGE to delete ${RUNNER_HOME}: " answer
    [[ "$answer" == "PURGE" ]] || die "Purge cancelled."
    rm -rf -- "$RUNNER_HOME"
    info "Local portable runner files deleted."
}

uninstall_runner() {
    if is_background_running; then
        stop_background
    fi
    if [[ -f "${RUNNER_HOME}/.runner" ]]; then
        remove_registration
    else
        info "Runner is already deregistered locally."
    fi
    purge_local
}

interactive_menu() {
    cat <<'EOF'

OneBrain soak runner
  1) Doctor / preflight
  2) First-time ephemeral setup and run
  3) Run configured runner in foreground
  4) Start configured runner in background
  5) Stop background runner
  6) Status
  7) Follow logs
  8) Remove GitHub registration
  9) Uninstall (remove + purge)
  0) Exit
EOF
    local choice
    read -r -p "Select: " choice
    case "$choice" in
        1) doctor ;;
        2)
            setup_runner ephemeral
            run_foreground
            ;;
        3) run_foreground ;;
        4) start_background ;;
        5) stop_background ;;
        6) show_status ;;
        7) follow_logs ;;
        8) remove_registration ;;
        9) uninstall_runner ;;
        0) exit 0 ;;
        *) die "Unknown menu choice." ;;
    esac
}

main() {
    local command="${1:-menu}"
    shift || true
    case "$command" in
        menu) interactive_menu ;;
        help | -h | --help) usage ;;
        doctor) doctor ;;
        deps) install_dependencies ;;
        setup)
            if [[ "${1:-}" == "--persistent" ]]; then
                setup_runner persistent
            elif [[ $# -eq 0 ]]; then
                setup_runner ephemeral
            else
                die "Usage: $0 setup [--persistent]"
            fi
            ;;
        setup-run)
            [[ $# -eq 0 ]] || die "Usage: $0 setup-run"
            setup_runner ephemeral
            run_foreground
            ;;
        run) run_foreground ;;
        start) start_background ;;
        stop) stop_background ;;
        status) show_status ;;
        logs) follow_logs ;;
        remove) remove_registration ;;
        purge) purge_local ;;
        uninstall) uninstall_runner ;;
        *) usage; die "Unknown command: ${command}" ;;
    esac
}

main "$@"
