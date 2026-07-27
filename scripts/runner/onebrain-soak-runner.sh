#!/usr/bin/env bash
set -Eeuo pipefail

# Portable GitHub Actions runner for OneBrain M5-07 soak jobs.
#
# The default setup is ephemeral: the runner accepts one job, unregisters
# itself automatically, and exits. No systemd service is installed.

REPOSITORY_URL="${ONEBRAIN_RUNNER_REPOSITORY_URL:-https://github.com/shpy2001gemi/OneBrain}"
RUNNER_HOME="${ONEBRAIN_RUNNER_HOME:-${HOME}/.local/share/onebrain-actions-runner}"
RUNNER_NAME="${ONEBRAIN_RUNNER_NAME:-onebrain-soak-$(hostname -s)}"
RUNNER_LABELS="${ONEBRAIN_RUNNER_LABELS:-onebrain-soak}"
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
  onebrain-soak-runner.sh deps            Install build dependencies (apt/dnf/yum)
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

No inbound firewall port is required. The runner and M5-07 loopback QUIC
workload only require outbound HTTPS (TCP 443) to GitHub.
EOF
}

require_linux_x64() {
    [[ "$(uname -s)" == "Linux" ]] || die "This kit supports Linux only."
    case "$(uname -m)" in
        x86_64 | amd64) ;;
        *) die "This workflow requires Linux x64; found $(uname -m)." ;;
    esac
}

require_supported_distribution() {
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
    require_linux_x64
    require_supported_distribution
    local failed=0
    local command_name
    local required_commands=(
        curl git tar gzip sha256sum realpath python3
        cc c++ make cmake pkg-config perl
    )

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
    memory_kib="$(awk '/^MemTotal:/ {print $2}' /proc/meminfo 2>/dev/null || true)"
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

    if command_exists timedatectl; then
        local ntp_state
        ntp_state="$(timedatectl show -p NTPSynchronized --value 2>/dev/null || true)"
        [[ "$ntp_state" == "yes" ]] || warn "System clock is not reported as NTP-synchronized."
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
    require_linux_x64
    require_supported_distribution
    info "Installing build/runtime dependencies. The Actions runner itself remains portable."
    if command_exists apt-get; then
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
    if [[ -x "${RUNNER_HOME}/config.sh" ]]; then
        info "Portable runner already downloaded at ${RUNNER_HOME}."
        return
    fi
    if [[ -e "$RUNNER_HOME" ]]; then
        die "${RUNNER_HOME} exists but is not a complete runner. Move it aside or run purge after inspecting it."
    fi

    github_api_curl_args
    info "Resolving the latest official actions/runner Linux x64 release"
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
assets = [
    asset for asset in release.get("assets", [])
    if re.fullmatch(r"actions-runner-linux-x64-[0-9.]+\.tar\.gz", asset.get("name", ""))
]
if len(assets) != 1:
    raise SystemExit("expected exactly one Linux x64 runner archive")
asset = assets[0]
digest = asset.get("digest") or ""
print(release.get("tag_name", ""))
print(asset.get("browser_download_url", ""))
print(digest)
'
    )"
    local metadata_lines=()
    mapfile -t metadata_lines <<<"$metadata"
    local version="${metadata_lines[0]:-}"
    local download_url="${metadata_lines[1]:-}"
    local digest="${metadata_lines[2]:-}"
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
    if ! printf '%s  %s\n' "$expected_sha" "$archive" | sha256sum --check --status; then
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
    require_linux_x64
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
    require_linux_x64
    require_non_root
    require_configured
    is_background_running && die "Background runner is already active."
    rm -f "$PID_FILE"
    umask 077
    printf '%s\n' "$$" >"$PID_FILE"
    info "Running in foreground. Ctrl+C stops the listener; an active job will fail if interrupted."
    cd "$RUNNER_HOME"
    exec ./run.sh
}

start_background() {
    require_linux_x64
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
        nohup ./run.sh >>"$LOG_FILE" 2>&1 &
        printf '%s\n' "$!" >"$PID_FILE"
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
    require_linux_x64
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
    resolved="$(realpath -m "$RUNNER_HOME")"
    case "$resolved" in
        / | /home | /root | /usr | /opt | /var | "$HOME")
            die "Unsafe purge target: ${resolved}"
            ;;
    esac
    [[ "$resolved" == "$HOME/"* || "$resolved" == /opt/* || "$resolved" == /srv/* ]] ||
        die "Purge target must be below HOME, /opt, or /srv: ${resolved}"
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
