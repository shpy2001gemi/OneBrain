#!/usr/bin/env bash
set -Eeuo pipefail

# OneBrain Concept Registry Task 21 runner.
#
# This script deliberately accepts only a mode, a fixed command, and (for the
# resource lane) one closed profile name. Candidate paths, tooling paths,
# receipt paths, and release identity always come from the reviewed checkout or
# the fixed target/base-v1/registry staging layout.

readonly RUNNER_FORMAT="onebrain/concept-registry-runner/1"
readonly TARGET_TRIPLE="x86_64-unknown-linux-gnu"
readonly SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
readonly REPOSITORY_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd -P)"
readonly REGISTRY_STAGE_ROOT="${REPOSITORY_ROOT}/target/base-v1/registry"
readonly CANDIDATE_ROOT="${REGISTRY_STAGE_ROOT}/candidate"
readonly PREVIOUS_ROOT="${REGISTRY_STAGE_ROOT}/previous"
readonly ENVIRONMENT_ROOT="${REGISTRY_STAGE_ROOT}/environment"
readonly WORK_ROOT="${REPOSITORY_ROOT}/target/base-v1/work/registry"
readonly PRODUCTION_PROFILE="${REPOSITORY_ROOT}/src/test-vectors/vnext/concept-registry-production-qualification-v1.json"
readonly APPROVER_POLICY="${REPOSITORY_ROOT}/src/test-vectors/vnext/base-v1-qualification-approver-policy-v1.json"
readonly IDL_HISTORY="${ENVIRONMENT_ROOT}/append-only-idl-history-root.txt"
readonly LABELS_FILE="${REPOSITORY_ROOT}/scripts/concept_registry/qualification-labels-v1.txt"
readonly RELEASE_PROBE="${REPOSITORY_ROOT}/src/target/release/examples/registry_probe"
readonly RELEASE_OPS="${REPOSITORY_ROOT}/src/target/release/examples/concept_registry_release_ops"
readonly FAILURE_HARNESS="${REPOSITORY_ROOT}/src/target/release/examples/concept_registry_failure_qualification"
readonly GENERATION_HARNESS="${REPOSITORY_ROOT}/src/target/release/examples/concept_registry_production_qualification"
readonly CANDIDATE_QUALIFIER_TOOL="${REPOSITORY_ROOT}/scripts/base/qualify_base.py"
readonly CANDIDATE_REQUEST_TOOL="${REPOSITORY_ROOT}/scripts/release/create_base_release_request.py"
readonly CANDIDATE_CLEAN_WORKTREE_TOOL="${REPOSITORY_ROOT}/scripts/release/prepare_clean_candidate.py"
readonly CANDIDATE_RELEASE_WRAPPER_TOOL="${RELEASE_OPS}"
readonly CANDIDATE_VERIFIER_TOOL="${REPOSITORY_ROOT}/scripts/release/verify_base_release_request.py"
readonly CANDIDATE_SIGNER_POLICY="${REPOSITORY_ROOT}/src/test-vectors/vnext/base-v1-release-signers-v1.json"

QUALIFICATION_MODE=""
RESOURCE_PROFILE=""
COMMAND=""
EVIDENCE_ROOT=""
RAW_EVIDENCE_ROOT=""
STAGED_CANDIDATE_REGISTRY_ROOT=""
STAGED_CANDIDATE_STAMP=""
STAGED_CANDIDATE_STATE=""
readonly REGISTRY_CLOSURE_DIGEST_FILE="registry-closure.blake3"

info() {
    printf '[onebrain-registry] %s\n' "$*"
}

die() {
    printf '[onebrain-registry] ERROR: %s\n' "$*" >&2
    exit 2
}

usage() {
    cat <<'EOF'
OneBrain Concept Registry qualification runner

Usage:
  onebrain-registry-runner.sh preflight --mode prequalification|release
  onebrain-registry-runner.sh closure --mode prequalification|release
  onebrain-registry-runner.sh build --mode prequalification|release
  onebrain-registry-runner.sh resource --mode prequalification|release --profile cold-cache|low-ram|ssd|hdd
  onebrain-registry-runner.sh kernel --mode prequalification|release
  onebrain-registry-runner.sh aggregate --mode prequalification|release

Fixed staging layout:
  target/base-v1/registry/previous/
  target/base-v1/registry/candidate/
  target/base-v1/registry/environment/

Measured outputs are written only below target/base-v1/evidence/<mode>/registry.
The fixture fallback is forbidden. Release identity is derived from the signed
request; this runner has no release-request/session/commit/tree override.
EOF
}

parse_arguments() {
    [[ $# -ge 1 ]] || { usage; exit 2; }
    COMMAND="$1"
    shift
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --mode)
                [[ $# -ge 2 ]] || die "--mode needs one value"
                QUALIFICATION_MODE="$2"
                shift 2
                ;;
            --profile)
                [[ $# -ge 2 ]] || die "--profile needs one value"
                RESOURCE_PROFILE="$2"
                shift 2
                ;;
            *) die "unknown argument: $1" ;;
        esac
    done
    [[ "$QUALIFICATION_MODE" == "prequalification" || "$QUALIFICATION_MODE" == "release" ]] ||
        die "closed qualification mode must be prequalification or release"
    case "$COMMAND" in
        preflight | closure | build | resource | kernel | aggregate) ;;
        *) die "unknown command: ${COMMAND}" ;;
    esac
    if [[ "$COMMAND" == "resource" ]]; then
        case "$RESOURCE_PROFILE" in
            cold-cache | low-ram | ssd | hdd) ;;
            *) die "resource requires a closed profile" ;;
        esac
    elif [[ -n "$RESOURCE_PROFILE" ]]; then
        die "--profile is valid only for resource"
    fi
    EVIDENCE_ROOT="${REPOSITORY_ROOT}/target/base-v1/evidence/${QUALIFICATION_MODE}/registry"
    RAW_EVIDENCE_ROOT="${EVIDENCE_ROOT}/raw"
}

require_command() {
    command -v "$1" >/dev/null 2>&1 || die "required command is unavailable: $1"
}

resolved_path() {
    python3 -c 'import os,sys; print(os.path.realpath(sys.argv[1]))' "$1"
}

require_external_secret_path() {
    local path="$1"
    local label="$2"
    [[ -f "$path" ]] || die "${label} is unavailable"
    local resolved
    resolved="$(resolved_path "$path")"
    case "$resolved" in
        "$REPOSITORY_ROOT" | "$REPOSITORY_ROOT"/*)
            die "${label} must remain outside the repository"
            ;;
    esac
}

require_fixed_host() {
    [[ "$(uname -s)" == "Linux" ]] || die "production-reference runner requires Linux"
    case "$(uname -m)" in
        x86_64 | amd64) ;;
        *) die "production-reference runner requires x86_64" ;;
    esac
    [[ "$(id -u)" -ne 0 ]] || die "qualification runner must not run as root"
    require_command python3
    require_command git
    require_command cargo
    require_command findmnt
    require_command sha256sum
    require_command /usr/bin/python3
    require_command /usr/bin/gpg
}

required_stage_files() {
    cat <<'EOF'
previous/input.jsonl
previous/concepts.obr
previous/concepts.obr.labels.idx
previous/concepts.obr.ccids.idx
previous/concepts.obr.manifest.json
previous/sbom.spdx.json
previous/release.stamp.json
previous/state.json
previous/sources.json
candidate/input.jsonl
candidate/concepts.obr
candidate/concepts.obr.labels.idx
candidate/concepts.obr.ccids.idx
candidate/concepts.obr.manifest.json
candidate/sbom.spdx.json
candidate/release.stamp.json
candidate/state.json
candidate/sources.json
environment/runner-image.json
environment/rust-toolchain.json
environment/registry_probe.sig
environment/registry-trust-policy.json
environment/release-public-key.hex
environment/query-label.txt
environment/candidate-semantic-evidence.json
environment/append-only-idl-history-root.txt
environment/host-environment-receipt.json
EOF
}

require_stage() {
    local relative
    while IFS= read -r relative; do
        [[ -f "${REGISTRY_STAGE_ROOT}/${relative}" ]] ||
            die "required Registry input is missing: ${relative}; fixture fallback is forbidden"
        [[ ! -L "${REGISTRY_STAGE_ROOT}/${relative}" ]] ||
            die "staged Registry input must not be a symlink: ${relative}"
    done < <(required_stage_files)

    if [[ "$QUALIFICATION_MODE" == "release" ]]; then
        for relative in environment/release-request.json environment/release-request.json.asc; do
            [[ -f "${REGISTRY_STAGE_ROOT}/${relative}" ]] ||
                die "release input is missing: ${relative}"
        done
    fi

    python3 - "$CANDIDATE_ROOT/concepts.obr" <<'PY'
import os
import sys

size = os.path.getsize(sys.argv[1])
if not 2_200_000_000 <= size <= 2_500_000_000:
    raise SystemExit(
        f"candidate concepts.obr is not production-size: {size} bytes"
    )
PY
    verify_staged_releases
}

verify_staged_releases() {
    python3 - \
        "$REGISTRY_STAGE_ROOT" \
        "$ENVIRONMENT_ROOT/registry-trust-policy.json" \
        "$ENVIRONMENT_ROOT/release-public-key.hex" \
        "$PRODUCTION_PROFILE" \
        "$REPOSITORY_ROOT/scripts/concept_registry" <<'PY'
from __future__ import annotations

import json
import sys
from pathlib import Path

import blake3
from cryptography.exceptions import InvalidSignature
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PublicKey

stage = Path(sys.argv[1])
policy = json.loads(Path(sys.argv[2]).read_text(encoding="utf-8"))
public_hex = Path(sys.argv[3]).read_text(encoding="ascii").strip()
profile = json.loads(Path(sys.argv[4]).read_text(encoding="utf-8"))
sys.path.insert(0, sys.argv[5])
from production_qualification import signer_fingerprint, trust_policy_digest
from release_cycle_qualification import (
    STAMP_ARTIFACT_FIELDS,
    STAMP_ARTIFACTS,
    STAMP_DISTRIBUTION,
    STAMP_FIELDS,
    STAMP_SIGNATURE_DOMAIN,
    STAMP_SOURCE_FIELDS,
    STAMP_SOURCES,
    STATE_FIELDS,
    _ordered_json,
    _pretty_ordered_json,
    _source_root,
)

frozen = profile["trust_policy"]
if trust_policy_digest(policy) != frozen["digest_hex"] or policy != frozen["policy"]:
    raise SystemExit("staged Registry trust policy differs from frozen profile")
allowed = {
    signer["public_key_hex"]: signer["fingerprint_hex"]
    for signer in policy["signers"]
}
if allowed.get(public_hex) != signer_fingerprint(bytes.fromhex(public_hex)):
    raise SystemExit("staged Registry release public key is not allowlisted")
public_key = Ed25519PublicKey.from_public_bytes(bytes.fromhex(public_hex))

def digest(path: Path) -> str:
    value = blake3.blake3()
    with path.open("rb") as handle:
        while chunk := handle.read(1024 * 1024):
            value.update(chunk)
    return value.hexdigest()

def frame(hasher: blake3.blake3, text: str) -> None:
    raw = text.encode("utf-8")
    hasher.update(len(raw).to_bytes(8, "big"))
    hasher.update(raw)

def artifact_root(rows: list[dict[str, object]]) -> str:
    hasher = blake3.blake3()
    hasher.update(b"onebrain:concept-registry-artifacts:1\0")
    for row in sorted(rows, key=lambda item: (str(item["role"]).encode(), str(item["relative_path"]).encode())):
        frame(hasher, str(row["role"]))
        frame(hasher, str(row["relative_path"]))
        hasher.update(int(row["length"]).to_bytes(8, "big"))
        frame(hasher, str(row["blake3"]))
    return hasher.hexdigest()

stamps: dict[str, dict[str, object]] = {}
states: dict[str, dict[str, object]] = {}
for lane in ("previous", "candidate"):
    root = stage / lane
    stamp_path = root / "release.stamp.json"
    state_path = root / "state.json"
    stamp_bytes = stamp_path.read_bytes()
    state_bytes = state_path.read_bytes()
    stamp = json.loads(stamp_bytes)
    state = json.loads(state_bytes)
    stamps[lane] = stamp
    states[lane] = state
    if not isinstance(stamp, dict) or set(stamp) != set(STAMP_FIELDS):
        raise SystemExit(f"{lane} release stamp fields are not closed")
    artifacts = stamp.get("artifacts")
    sources = stamp.get("sources")
    if (
        not isinstance(artifacts, list)
        or len(artifacts) != 5
        or any(not isinstance(row, dict) or set(row) != set(STAMP_ARTIFACT_FIELDS) for row in artifacts)
        or {(row["role"], row["relative_path"]) for row in artifacts} != STAMP_ARTIFACTS
    ):
        raise SystemExit(f"{lane} release artifact rows are not closed")
    if (
        not isinstance(sources, list)
        or len(sources) != 5
        or any(not isinstance(row, dict) or set(row) != set(STAMP_SOURCE_FIELDS) for row in sources)
        or {row["name"] for row in sources} != STAMP_SOURCES
    ):
        raise SystemExit(f"{lane} release source rows are not closed")
    canonical = {
        field: (
            [{nested: row[nested] for nested in STAMP_ARTIFACT_FIELDS} for row in artifacts]
            if field == "artifacts"
            else [{nested: row[nested] for nested in STAMP_SOURCE_FIELDS} for row in sources]
            if field == "sources"
            else stamp[field]
        )
        for field in STAMP_FIELDS
    }
    if stamp_bytes != _pretty_ordered_json(canonical, STAMP_FIELDS):
        raise SystemExit(f"{lane} release stamp is not canonical Rust JSON")
    for row in artifacts:
        artifact = root / str(row["relative_path"])
        if artifact.is_symlink() or not artifact.is_file():
            raise SystemExit(f"{lane} release artifact is not a regular file")
        if artifact.stat().st_size != row["length"] or digest(artifact) != row["blake3"]:
            raise SystemExit(f"{lane} release artifact differs from signed stamp")
    if artifact_root(artifacts) != stamp.get("artifact_root"):
        raise SystemExit(f"{lane} release aggregate root is invalid")
    source_value = json.loads((root / "sources.json").read_text(encoding="utf-8"))
    if source_value != sources or _source_root(sources) != stamp.get("source_root"):
        raise SystemExit(f"{lane} release sources differ from signed stamp")
    manifest = json.loads((root / "concepts.obr.manifest.json").read_text(encoding="utf-8"))
    manifest_sources = manifest.get("sources") if isinstance(manifest, dict) else None
    if (
        not isinstance(manifest_sources, dict)
        or stamp.get("builder_version") != manifest.get("builder_version")
        or stamp.get("dedup_policy_version") != manifest.get("dedup_policy_version")
        or len(manifest_sources) != len(sources)
        or any(
            not isinstance(manifest_sources.get(source["name"]), dict)
            or any(
                source[field] != manifest_sources[source["name"]].get(field)
                for field in ("snapshot_id", "source_uri", "license")
            )
            for source in sources
        )
    ):
        raise SystemExit(f"{lane} release stamp differs from packaged manifest")
    if stamp.get("distribution") != STAMP_DISTRIBUTION or stamp.get("signer_public_key") != public_hex:
        raise SystemExit(f"{lane} release policy or signer differs")
    unsigned = dict(canonical)
    unsigned["signature"] = ""
    try:
        public_key.verify(
            bytes.fromhex(str(stamp["signature"])),
            STAMP_SIGNATURE_DOMAIN + blake3.blake3(_ordered_json(unsigned, STAMP_FIELDS)).digest(),
        )
    except (ValueError, InvalidSignature) as error:
        raise SystemExit(f"{lane} release stamp signature is invalid") from error
    if not isinstance(state, dict) or set(state) != set(STATE_FIELDS):
        raise SystemExit(f"{lane} state fields are not closed")
    if state_bytes != _pretty_ordered_json(state, STATE_FIELDS):
        raise SystemExit(f"{lane} state is not canonical Rust JSON")
    generation = state.get("generation")
    if not isinstance(generation, int) or isinstance(generation, bool) or generation <= 0:
        raise SystemExit(f"{lane} state generation is invalid")
    state_view = {field: state[field] for field in STATE_FIELDS[:-1]}
    root_digest = blake3.blake3(
        b"onebrain:concept-registry-state:1\0" + _ordered_json(state_view, STATE_FIELDS[:-1])
    ).hexdigest()
    if (
        state.get("profile") != "onebrain/concept-registry-release-state/1"
        or state.get("active_release") != stamp.get("release_id")
        or state.get("state_root") != root_digest
    ):
        raise SystemExit(f"{lane} state does not bind the signed release")
if (
    stamps["previous"].get("release_id") == stamps["candidate"].get("release_id")
    or stamps["previous"].get("artifact_root") == stamps["candidate"].get("artifact_root")
    or states["candidate"].get("previous_release") != stamps["previous"].get("release_id")
    or int(states["candidate"]["generation"]) <= int(states["previous"]["generation"])
):
    raise SystemExit("candidate state does not advance the exact previous signed release")
PY
}

validate_runner_receipt() {
    local expected_label
    if [[ -n "${ONEBRAIN_REGISTRY_RUNNER_LABELS:-}" ]]; then
        expected_label="${ONEBRAIN_REGISTRY_RUNNER_LABELS##*,}"
    else
        expected_label="onebrain-registry-${RESOURCE_PROFILE:-controller}"
    fi
    python3 - \
        "$ENVIRONMENT_ROOT/host-environment-receipt.json" \
        "$ENVIRONMENT_ROOT/runner-image.json" \
        "$TARGET_TRIPLE" \
        "$expected_label" \
        "${ONEBRAIN_REGISTRY_RUNNER_LABELS:-}" <<'PY'
import json
import sys
from pathlib import Path

receipt_path, image_path, target, expected_profile_label, actual_labels = sys.argv[1:]
receipt = json.loads(Path(receipt_path).read_text(encoding="utf-8"))
expected = {
    "format",
    "immutable",
    "target_triple",
    "runner_image_blake3",
    "runner_labels",
}
if set(receipt) != expected:
    raise SystemExit("host environment receipt fields are not closed")
if receipt["format"] != "onebrain/registry-host-environment-receipt/1":
    raise SystemExit("host environment receipt format is invalid")
if receipt["immutable"] is not True or receipt["target_triple"] != target:
    raise SystemExit("host environment receipt is not immutable for the frozen target")
labels = receipt["runner_labels"]
required = {"self-hosted", "linux", "x64", "onebrain-registry-image-v1", expected_profile_label}
if not isinstance(labels, list) or not required.issubset(set(labels)):
    raise SystemExit("host environment receipt omits immutable runner labels")
if actual_labels and set(actual_labels.split(",")) != set(labels):
    raise SystemExit("workflow runner labels differ from immutable receipt")

import blake3
measured = blake3.blake3(Path(image_path).read_bytes()).hexdigest()
if receipt["runner_image_blake3"] != measured:
    raise SystemExit("runner image evidence differs from immutable host receipt")
PY
}

verify_release_request() {
    [[ "$QUALIFICATION_MODE" == "release" ]] || return 0
    local gpg_home="${ONEBRAIN_QUALIFICATION_GPG_HOME:?external qualification GPG home is required}"
    require_external_secret_path "$gpg_home/pubring.kbx" "qualification GPG keyring"
    mkdir -p "$EVIDENCE_ROOT"
    /usr/bin/python3 "$REPOSITORY_ROOT/scripts/release/verify_base_release_request.py" \
        --request "$ENVIRONMENT_ROOT/release-request.json" \
        --signature "$ENVIRONMENT_ROOT/release-request.json.asc" \
        --policy "$APPROVER_POLICY" \
        --gpg-home "$gpg_home" \
        >"$EVIDENCE_ROOT/verified-release-context.json"
    python3 - \
        "$EVIDENCE_ROOT/verified-release-context.json" \
        "$EVIDENCE_ROOT/release-run-context.json" <<'PY'
import json
import sys
from pathlib import Path

verified = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
context = verified.get("run_context")
required = {
    "format",
    "variant",
    "release_request_digest",
    "qualification_session_id",
    "candidate_commit",
    "candidate_tree",
}
if not isinstance(context, dict) or set(context) != required:
    raise SystemExit("verified release context is not closed")
if context.get("variant") != "Release":
    raise SystemExit("signed request did not derive Release context")
Path(sys.argv[2]).write_text(
    json.dumps(context, sort_keys=True, separators=(",", ":")) + "\n",
    encoding="utf-8",
)
PY
}

run_preflight() {
    require_fixed_host
    require_stage
    validate_runner_receipt
    verify_release_request
    info "preflight passed for ${QUALIFICATION_MODE}"
}

compute_closure() {
    run_preflight
    mkdir -p "$EVIDENCE_ROOT"
    python3 - \
        "$REPOSITORY_ROOT" \
        "$REGISTRY_STAGE_ROOT" \
        "$EVIDENCE_ROOT/registry-closure.json" \
        "$EVIDENCE_ROOT/$REGISTRY_CLOSURE_DIGEST_FILE" \
        "$QUALIFICATION_MODE" <<'PY'
from __future__ import annotations

import json
import os
import sys
from pathlib import Path

import blake3

repo = Path(sys.argv[1]).resolve()
stage = Path(sys.argv[2]).resolve()
manifest_output = Path(sys.argv[3])
digest_output = Path(sys.argv[4])
mode = sys.argv[5]

stage_paths = [
    "previous/input.jsonl",
    "previous/concepts.obr",
    "previous/concepts.obr.labels.idx",
    "previous/concepts.obr.ccids.idx",
    "previous/concepts.obr.manifest.json",
    "previous/sbom.spdx.json",
    "previous/release.stamp.json",
    "previous/state.json",
    "previous/sources.json",
    "candidate/input.jsonl",
    "candidate/concepts.obr",
    "candidate/concepts.obr.labels.idx",
    "candidate/concepts.obr.ccids.idx",
    "candidate/concepts.obr.manifest.json",
    "candidate/sbom.spdx.json",
    "candidate/release.stamp.json",
    "candidate/state.json",
    "candidate/sources.json",
    "environment/runner-image.json",
    "environment/rust-toolchain.json",
    "environment/registry_probe.sig",
    "environment/registry-trust-policy.json",
    "environment/release-public-key.hex",
    "environment/query-label.txt",
    "environment/candidate-semantic-evidence.json",
    "environment/append-only-idl-history-root.txt",
]
if mode == "release":
    stage_paths.extend(
        ["environment/release-request.json", "environment/release-request.json.asc"]
    )

repo_paths = [
    "src/Cargo.lock",
    "src/Cargo.toml",
    "src/ku-core/Cargo.toml",
    "src/onebrain-node/Cargo.toml",
    "src/ku-core/src/concept_registry_manifest.rs",
    "src/ku-core/src/concept_registry_release.rs",
    "src/ku-core/src/indexed_concept_registry.rs",
    "src/ku-core/src/qualification_request.rs",
    "src/ku-core/examples/registry_probe.rs",
    "src/ku-core/examples/concept_registry_release_ops.rs",
    "src/ku-core/examples/concept_registry_failure_qualification.rs",
    "src/onebrain-node/src/concept_registry_runtime.rs",
    "src/onebrain-node/examples/concept_registry_production_qualification.rs",
    "scripts/concept_registry/requirements.txt",
    "scripts/concept_registry/qualification-labels-v1.txt",
    "scripts/release/verify_base_release_request.py",
    "scripts/runner/onebrain-registry-runner.sh",
    "src/test-vectors/vnext/concept-registry-production-qualification-v1.json",
    "docs/specs/vnext/CONCEPT_REGISTRY_PRODUCTION_QUALIFICATION_PROFILE_V1.md",
    "src/test-vectors/vnext/base-v1-qualification-approver-policy-v1.json",
    "src/test-vectors/vnext/base-v1-runtime-interface-history-v1.json",
]
repo_paths.extend(
    path.relative_to(repo).as_posix()
    for path in sorted((repo / "scripts/concept_registry").glob("*.py"))
)
if mode == "release":
    repo_paths.extend(
        [
            "scripts/base/qualify_base.py",
            "scripts/release/create_base_release_request.py",
            "scripts/release/prepare_clean_candidate.py",
            "src/test-vectors/vnext/base-v1-release-signers-v1.json",
        ]
    )
probe = repo / "src/target/release/examples/registry_probe"
release_ops = repo / "src/target/release/examples/concept_registry_release_ops"
for executable in (probe, release_ops):
    if not executable.is_file():
        raise SystemExit(f"exact release tool has not been built: {executable.name}")

def digest(path: Path) -> str:
    value = blake3.blake3()
    with path.open("rb") as handle:
        while chunk := handle.read(1024 * 1024):
            value.update(chunk)
    return value.hexdigest()

rows = []
seen = set()
for logical, path in [
    *((f"stage/{name}", stage / name) for name in stage_paths),
    *((f"repo/{name}", repo / name) for name in repo_paths),
    ("repo/src/target/release/examples/registry_probe", probe),
    ("repo/src/target/release/examples/concept_registry_release_ops", release_ops),
]:
    if logical in seen:
        continue
    seen.add(logical)
    if not path.is_file() or path.is_symlink():
        raise SystemExit(f"closure input is not a regular no-follow file: {logical}")
    rows.append(
        {"path": logical, "length": path.stat().st_size, "blake3": digest(path)}
    )

def load(relative: str) -> dict[str, object]:
    value = json.loads((stage / relative).read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise SystemExit(f"closure metadata is not an object: {relative}")
    return value

previous_stamp = load("previous/release.stamp.json")
candidate_stamp = load("candidate/release.stamp.json")
previous_state = load("previous/state.json")
candidate_state = load("candidate/state.json")
identity = {
    "previous_release_root": previous_stamp.get("artifact_root"),
    "candidate_release_root": candidate_stamp.get("artifact_root"),
    "previous_generation": previous_state.get("generation"),
    "candidate_generation": candidate_state.get("generation"),
    "previous_release_id": previous_stamp.get("release_id"),
    "candidate_release_id": candidate_stamp.get("release_id"),
}
if any(value is None or isinstance(value, bool) for value in identity.values()):
    raise SystemExit("closure release roots/generations/identities are incomplete")

document = {
    "format": "onebrain/concept-registry-closure/1",
    "mode": mode,
    "target_triple": "x86_64-unknown-linux-gnu",
    "identity": identity,
    "rows": sorted(rows, key=lambda row: row["path"].encode("utf-8")),
}
canonical = json.dumps(
    document, ensure_ascii=False, sort_keys=True, separators=(",", ":")
).encode("utf-8")
closure = blake3.blake3(
    b"onebrain:concept-registry-closure:1\0" + canonical
).hexdigest()
document["registry_closure_digest"] = closure
manifest_output.write_text(
    json.dumps(document, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
    encoding="utf-8",
)
digest_output.write_text(closure + "\n", encoding="ascii")
print(closure)
PY
}

build_release_probe() {
    require_fixed_host
    cargo build --release --locked --manifest-path "$REPOSITORY_ROOT/src/Cargo.toml" \
        -p ku-core --example registry_probe --example concept_registry_release_ops
}

build_kernel_tools() {
    build_release_probe
    cargo build --release --locked --manifest-path "$REPOSITORY_ROOT/src/Cargo.toml" \
        -p ku-core --features concept-registry-failure-harness \
        --example concept_registry_failure_qualification
    cargo build --release --locked --manifest-path "$REPOSITORY_ROOT/src/Cargo.toml" \
        -p onebrain-node --example concept_registry_production_qualification
}

stage_candidate_context() {
    local release_id generation context_name
    release_id="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1], encoding="utf-8"))["release_id"])' "$CANDIDATE_ROOT/release.stamp.json")"
    generation="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1], encoding="utf-8"))["generation"])' "$CANDIDATE_ROOT/state.json")"
    [[ "$generation" =~ ^[1-9][0-9]*$ ]] || die "candidate generation is invalid"
    printf -v context_name 'staged-candidate-%s-%020d' \
        "${RESOURCE_PROFILE:-controller}-${GITHUB_RUN_ID:-manual}-${GITHUB_RUN_ATTEMPT:-1}" \
        "$generation"
    STAGED_CANDIDATE_REGISTRY_ROOT="${WORK_ROOT}/${context_name}"
    [[ ! -e "$STAGED_CANDIDATE_REGISTRY_ROOT" ]] ||
        die "candidate context already exists; preserve it and start a new attempt"
    local release_dir="${STAGED_CANDIDATE_REGISTRY_ROOT}/releases/${release_id}"
    local state_dir="${STAGED_CANDIDATE_REGISTRY_ROOT}/state"
    mkdir -p "$release_dir" "$state_dir"
    local name
    for name in \
        concepts.obr \
        concepts.obr.labels.idx \
        concepts.obr.ccids.idx \
        concepts.obr.manifest.json \
        sbom.spdx.json \
        release.stamp.json; do
        ln "$CANDIDATE_ROOT/$name" "$release_dir/$name" ||
            die "candidate context requires same-volume no-copy hard links"
    done
    printf -v STAGED_CANDIDATE_STATE '%s/state-%020d.json' "$state_dir" "$generation"
    ln "$CANDIDATE_ROOT/state.json" "$STAGED_CANDIDATE_STATE" ||
        die "candidate context state requires a same-volume hard link"
    STAGED_CANDIDATE_STAMP="$release_dir/release.stamp.json"
}

prepare_prequalification_binding() {
    local closure
    closure="$(tr -d '\r\n' <"$EVIDENCE_ROOT/$REGISTRY_CLOSURE_DIGEST_FILE")"
    python3 - \
        "$CANDIDATE_ROOT" \
        "$ENVIRONMENT_ROOT/registry-trust-policy.json" \
        "$PRODUCTION_PROFILE" \
        "$RELEASE_PROBE" \
        "$closure" \
        "$EVIDENCE_ROOT/prequalification-context.json" \
        "$EVIDENCE_ROOT/prequalification-binding.json" <<'PY'
import json
import sys
from pathlib import Path

import blake3

candidate, policy_path, profile_path, probe_path, closure, context_out, binding_out = sys.argv[1:]
candidate = Path(candidate)
policy_path = Path(policy_path)
profile_path = Path(profile_path)
probe_path = Path(probe_path)
stamp_path = candidate / "release.stamp.json"
state_path = candidate / "state.json"
stamp = json.loads(stamp_path.read_text(encoding="utf-8"))
state = json.loads(state_path.read_text(encoding="utf-8"))
profile = json.loads(Path(profile_path).read_text(encoding="utf-8"))

payloads = {
    f"{row['role']}:{row['relative_path']}": row["blake3"]
    for row in stamp["artifacts"]
}
expected = {
    "OBR:concepts.obr",
    "LABEL_INDEX:concepts.obr.labels.idx",
    "CCID_INDEX:concepts.obr.ccids.idx",
    "MANIFEST:concepts.obr.manifest.json",
    "SPDX_SBOM:sbom.spdx.json",
}
if set(payloads) != expected:
    raise SystemExit("candidate stamp payload set is not exact")
trust = profile["trust_policy"]
binding = {
    "release_aggregate_root": stamp["artifact_root"],
    "registry_generation": state["generation"],
    "production_profile_blake3": blake3.blake3(
        json.dumps(profile, sort_keys=True, separators=(",", ":")).encode("utf-8")
    ).hexdigest(),
    "trust_policy_digest": trust["digest_hex"],
    "signer_fingerprint": trust["policy"]["signers"][0]["fingerprint_hex"],
    "probe_blake3": blake3.blake3(probe_path.read_bytes()).hexdigest(),
    "executable_blake3": blake3.blake3(probe_path.read_bytes()).hexdigest(),
    "candidate_payload_artifacts_blake3": payloads,
    "release_stamp_blake3": blake3.blake3(stamp_path.read_bytes()).hexdigest(),
}
context = {
    "format": "onebrain/qualification-run-context/1",
    "variant": "Prequalification",
    "closure_digest": closure,
}
Path(context_out).write_text(json.dumps(context, sort_keys=True) + "\n", encoding="utf-8")
Path(binding_out).write_text(json.dumps(binding, sort_keys=True) + "\n", encoding="utf-8")
PY
}

run_resource() {
    compute_closure >/dev/null
    mkdir -p "$RAW_EVIDENCE_ROOT"
    local private_key="${ONEBRAIN_REGISTRY_PRIVATE_KEY_FILE:?external Registry signing key path is required}"
    require_external_secret_path "$private_key" "Registry signing key"
    local budget timeout
    case "$RESOURCE_PROFILE" in
        cold-cache) budget="cold-cache-production-v1"; timeout=240 ;;
        low-ram) budget="low-ram-production-v1"; timeout=360 ;;
        ssd) budget="ssd-production-v1"; timeout=180 ;;
        hdd) budget="hdd-production-v1"; timeout=360 ;;
    esac

    local output="$RAW_EVIDENCE_ROOT/resource-${RESOURCE_PROFILE}.json"
    if [[ "$QUALIFICATION_MODE" == "prequalification" ]]; then
        prepare_prequalification_binding
        python3 "$REPOSITORY_ROOT/scripts/concept_registry/resource_qualification.py" \
            --profile "$RESOURCE_PROFILE" \
            --probe "$RELEASE_PROBE" \
            --obr "$CANDIDATE_ROOT/concepts.obr" \
            --labels-file "$LABELS_FILE" \
            --output "$output" \
            --cache-strategy auto \
            --budget-profile "$budget" \
            --timeout-seconds "$timeout" \
            --run-context "$EVIDENCE_ROOT/prequalification-context.json" \
            --release-binding "$EVIDENCE_ROOT/prequalification-binding.json" \
            --trust-policy "$ENVIRONMENT_ROOT/registry-trust-policy.json" \
            --private-key "$private_key"
    else
        verify_release_request
        stage_candidate_context
        local gpg_home="${ONEBRAIN_QUALIFICATION_GPG_HOME:?external qualification GPG home is required}"
        local release_id
        release_id="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1], encoding="utf-8"))["release_id"])' "$CANDIDATE_ROOT/release.stamp.json")"
        python3 "$REPOSITORY_ROOT/scripts/concept_registry/resource_qualification.py" \
            --profile "$RESOURCE_PROFILE" \
            --probe "$RELEASE_PROBE" \
            --obr "$CANDIDATE_ROOT/concepts.obr" \
            --labels-file "$LABELS_FILE" \
            --output "$output" \
            --cache-strategy auto \
            --budget-profile "$budget" \
            --timeout-seconds "$timeout" \
            --release-request "$ENVIRONMENT_ROOT/release-request.json" \
            --release-request-signature "$ENVIRONMENT_ROOT/release-request.json.asc" \
            --qualification-approver-policy "$APPROVER_POLICY" \
            --gpg-home "$gpg_home" \
            --candidate-root "$REPOSITORY_ROOT" \
            --registry-root "$STAGED_CANDIDATE_REGISTRY_ROOT" \
            --release-id "$release_id" \
            --candidate-semantic-evidence "$ENVIRONMENT_ROOT/candidate-semantic-evidence.json" \
            --production-profile "$PRODUCTION_PROFILE" \
            --production-vector "$PRODUCTION_PROFILE" \
            --append-only-idl-history "$IDL_HISTORY" \
            --candidate-tool-qualifier "$CANDIDATE_QUALIFIER_TOOL" \
            --candidate-tool-request "$CANDIDATE_REQUEST_TOOL" \
            --candidate-tool-clean-worktree "$CANDIDATE_CLEAN_WORKTREE_TOOL" \
            --candidate-tool-release-wrapper "$CANDIDATE_RELEASE_WRAPPER_TOOL" \
            --candidate-tool-verifier "$CANDIDATE_VERIFIER_TOOL" \
            --candidate-tool-signer-policy "$CANDIDATE_SIGNER_POLICY" \
            --label-index "$CANDIDATE_ROOT/concepts.obr.labels.idx" \
            --ccid-index "$CANDIDATE_ROOT/concepts.obr.ccids.idx" \
            --manifest "$CANDIDATE_ROOT/concepts.obr.manifest.json" \
            --sbom "$CANDIDATE_ROOT/sbom.spdx.json" \
            --release-stamp "$CANDIDATE_ROOT/release.stamp.json" \
            --probe-signature "$ENVIRONMENT_ROOT/registry_probe.sig" \
            --executable "$RELEASE_PROBE" \
            --rust-toolchain-evidence "$ENVIRONMENT_ROOT/rust-toolchain.json" \
            --runner-image-evidence "$ENVIRONMENT_ROOT/runner-image.json" \
            --target-triple "$TARGET_TRIPLE" \
            --trust-policy "$ENVIRONMENT_ROOT/registry-trust-policy.json" \
            --private-key "$private_key"
    fi
}

run_kernel_prequalification() {
    prepare_prequalification_binding
    local private_key="${ONEBRAIN_REGISTRY_PRIVATE_KEY_FILE:?external Registry signing key path is required}"
    require_external_secret_path "$private_key" "Registry signing key"
    mkdir -p "$RAW_EVIDENCE_ROOT" "$WORK_ROOT"
    "$FAILURE_HARNESS" --prequalification \
        "$WORK_ROOT/failure" \
        "$CANDIDATE_ROOT/concepts.obr" \
        "$CANDIDATE_ROOT/sbom.spdx.json" \
        "$CANDIDATE_ROOT/sources.json" \
        "$private_key" \
        "$EVIDENCE_ROOT/prequalification-context.json" \
        "$EVIDENCE_ROOT/prequalification-binding.json" \
        "$ENVIRONMENT_ROOT/registry-trust-policy.json" \
        "$RAW_EVIDENCE_ROOT/failure-qualification.json"
    python3 "$REPOSITORY_ROOT/scripts/concept_registry/ccid_stability_diff.py" \
        --old-input "$PREVIOUS_ROOT/input.jsonl" \
        --old-obr "$PREVIOUS_ROOT/concepts.obr" \
        --old-manifest "$PREVIOUS_ROOT/concepts.obr.manifest.json" \
        --new-input "$CANDIDATE_ROOT/input.jsonl" \
        --new-obr "$CANDIDATE_ROOT/concepts.obr" \
        --new-manifest "$CANDIDATE_ROOT/concepts.obr.manifest.json" \
        --work-dir "$WORK_ROOT/ccid" \
        --output "$RAW_EVIDENCE_ROOT/ccid-stability.json"
    cargo test --locked --release --manifest-path "$REPOSITORY_ROOT/src/Cargo.toml" \
        -p onebrain-node concept_registry_runtime --lib -- --test-threads=1 \
        | tee "$RAW_EVIDENCE_ROOT/live-reader-process-kill.log"
}

run_kernel_release() {
    verify_release_request
    local private_key="${ONEBRAIN_REGISTRY_PRIVATE_KEY_FILE:?external Registry signing key path is required}"
    local gpg_home="${ONEBRAIN_QUALIFICATION_GPG_HOME:?external qualification GPG home is required}"
    require_external_secret_path "$private_key" "Registry signing key"
    local public_key old_release candidate_release query_label
    public_key="$(tr -d '\r\n' <"$ENVIRONMENT_ROOT/release-public-key.hex")"
    old_release="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1], encoding="utf-8"))["release_id"])' "$PREVIOUS_ROOT/release.stamp.json")"
    candidate_release="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1], encoding="utf-8"))["release_id"])' "$CANDIDATE_ROOT/release.stamp.json")"
    query_label="$(sed -n '1p' "$ENVIRONMENT_ROOT/query-label.txt")"
    [[ -n "$query_label" ]] || die "query label is empty"
    [[ "$old_release" != "$candidate_release" ]] || die "old and candidate release IDs must differ"
    mkdir -p "$RAW_EVIDENCE_ROOT" "$WORK_ROOT"
    stage_candidate_context
    local cycle_root="${WORK_ROOT}/release-cycle-${GITHUB_RUN_ID:-manual}-${GITHUB_RUN_ATTEMPT:-1}"
    local generation_root="${WORK_ROOT}/generation-${GITHUB_RUN_ID:-manual}-${GITHUB_RUN_ATTEMPT:-1}"
    [[ ! -e "$cycle_root" && ! -e "$generation_root" ]] ||
        die "release work root already exists; preserve it and start a new attempt"

    # The signed release-cycle and signed CCID producers have no caller command
    # plan. This fixed wrapper passes only reviewed paths; both producers verify
    # release-request bytes and candidate measurements before their first
    # operation.
    python3 - \
        "$REPOSITORY_ROOT" "$ENVIRONMENT_ROOT" "$PREVIOUS_ROOT" "$CANDIDATE_ROOT" \
        "$cycle_root" "$old_release" "$candidate_release" "$query_label" \
        "$private_key" "$gpg_home" "$APPROVER_POLICY" "$PRODUCTION_PROFILE" \
        "$IDL_HISTORY" "$RELEASE_PROBE" "$RELEASE_OPS" \
        "$CANDIDATE_QUALIFIER_TOOL" "$CANDIDATE_REQUEST_TOOL" \
        "$CANDIDATE_CLEAN_WORKTREE_TOOL" "$CANDIDATE_RELEASE_WRAPPER_TOOL" \
        "$CANDIDATE_VERIFIER_TOOL" "$CANDIDATE_SIGNER_POLICY" \
        "$STAGED_CANDIDATE_STAMP" "$STAGED_CANDIDATE_STATE" \
        "$RAW_EVIDENCE_ROOT/signed-release-cycle.json" \
        "$RAW_EVIDENCE_ROOT/ccid-stability.json" <<'PY'
from __future__ import annotations

import json
import os
import sys
import tempfile
from pathlib import Path

from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey

repo = Path(sys.argv[1])
environment = Path(sys.argv[2])
previous = Path(sys.argv[3])
candidate = Path(sys.argv[4])
cycle_root = Path(sys.argv[5])
old_release, candidate_release, query_label = sys.argv[6:9]
private_key_path = Path(sys.argv[9])
gpg_home = Path(sys.argv[10])
approver_policy = Path(sys.argv[11])
production_profile = Path(sys.argv[12])
idl_history = Path(sys.argv[13])
probe = Path(sys.argv[14])
release_ops = Path(sys.argv[15])
tool_values = [Path(value) for value in sys.argv[16:22]]
candidate_stamp = Path(sys.argv[22])
candidate_state = Path(sys.argv[23])
cycle_output = Path(sys.argv[24])
ccid_output = Path(sys.argv[25])

sys.path.insert(0, str(repo / "scripts/concept_registry"))
from ccid_stability_qualification import qualify_ccid_stability_from_signed_request
from release_cycle_qualification import run_release_cycle

request = environment / "release-request.json"
signature = environment / "release-request.json.asc"
policy = json.loads((environment / "registry-trust-policy.json").read_text(encoding="utf-8"))
key_bytes = bytes.fromhex(private_key_path.read_text(encoding="ascii").strip())
signing_key = Ed25519PrivateKey.from_private_bytes(key_bytes)
tool_names = (
    "qualifier",
    "request",
    "clean_worktree",
    "release_wrapper",
    "verifier",
    "signer_policy",
)
candidate_tooling = dict(zip(tool_names, tool_values, strict=True))

receipt = run_release_cycle(
    request,
    signature,
    approver_policy,
    gpg_home,
    candidate_release_stamp=candidate_stamp,
    candidate_state=candidate_state,
    candidate_semantic_evidence=environment / "candidate-semantic-evidence.json",
    production_profile=production_profile,
    production_vector=production_profile,
    append_only_idl_history=idl_history,
    candidate_tooling=candidate_tooling,
    probe=probe,
    probe_signature=environment / "registry_probe.sig",
    executable=probe,
    rust_toolchain_evidence=environment / "rust-toolchain.json",
    runner_image_evidence=environment / "runner-image.json",
    target_triple="x86_64-unknown-linux-gnu",
    registry_root=cycle_root,
    old_input=previous / "input.jsonl",
    old_obr=previous / "concepts.obr",
    old_manifest=previous / "concepts.obr.manifest.json",
    old_sbom=previous / "sbom.spdx.json",
    candidate_input=candidate / "input.jsonl",
    candidate_obr=candidate / "concepts.obr",
    candidate_manifest=candidate / "concepts.obr.manifest.json",
    candidate_sbom=candidate / "sbom.spdx.json",
    sources=candidate / "sources.json",
    old_release_id=old_release,
    candidate_release_id=candidate_release,
    query_label=query_label,
    release_private_key=private_key_path,
    release_public_key=(environment / "release-public-key.hex").read_text(encoding="ascii").strip(),
    signing_key=signing_key,
    receipt_policy=policy,
)

ccid = qualify_ccid_stability_from_signed_request(
    request,
    signature,
    approver_policy,
    gpg_home,
    previous / "input.jsonl",
    previous / "concepts.obr",
    previous / "concepts.obr.manifest.json",
    candidate / "input.jsonl",
    candidate / "concepts.obr",
    candidate / "concepts.obr.manifest.json",
    sample_limit=20,
    work_dir=cycle_root.parent / "ccid-release",
    signing_key=signing_key,
    receipt_policy=policy,
)

def write_atomic(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, name = tempfile.mkstemp(prefix=f".{path.name}.", suffix=".tmp", dir=path.parent)
    try:
        with os.fdopen(fd, "w", encoding="utf-8", newline="\n") as handle:
            json.dump(value, handle, indent=2, sort_keys=True)
            handle.write("\n")
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(name, path)
    finally:
        Path(name).unlink(missing_ok=True)

write_atomic(cycle_output, receipt)
write_atomic(ccid_output, ccid)
PY

    "$RELEASE_OPS" package "$generation_root" \
        "$PREVIOUS_ROOT/concepts.obr" "$PREVIOUS_ROOT/sbom.spdx.json" \
        "$PREVIOUS_ROOT/sources.json" "$old_release" "$private_key"
    "$RELEASE_OPS" package "$generation_root" \
        "$CANDIDATE_ROOT/concepts.obr" "$CANDIDATE_ROOT/sbom.spdx.json" \
        "$CANDIDATE_ROOT/sources.json" "$candidate_release" "$private_key"
    "$RELEASE_OPS" activate "$generation_root" "$old_release" "$public_key"

    "$GENERATION_HARNESS" --release \
        "$generation_root" "$public_key" "$old_release" "$candidate_release" "$query_label" \
        "$ENVIRONMENT_ROOT/release-request.json" \
        "$ENVIRONMENT_ROOT/release-request.json.asc" \
        "$APPROVER_POLICY" "$gpg_home" \
        "$REPOSITORY_ROOT" "$ENVIRONMENT_ROOT/candidate-semantic-evidence.json" \
        "$PRODUCTION_PROFILE" "$PRODUCTION_PROFILE" "$IDL_HISTORY" \
        "$RELEASE_PROBE" "$ENVIRONMENT_ROOT/registry_probe.sig" \
        "$ENVIRONMENT_ROOT/rust-toolchain.json" "$ENVIRONMENT_ROOT/runner-image.json" \
        "$TARGET_TRIPLE" "$ENVIRONMENT_ROOT/registry-trust-policy.json" \
        "$private_key" "$RAW_EVIDENCE_ROOT/generation-swap.json"

    "$FAILURE_HARNESS" --release \
        "$WORK_ROOT/failure-release-${GITHUB_RUN_ID:-manual}-${GITHUB_RUN_ATTEMPT:-1}" \
        "$CANDIDATE_ROOT/concepts.obr" "$CANDIDATE_ROOT/sbom.spdx.json" \
        "$CANDIDATE_ROOT/sources.json" "$private_key" \
        "$ENVIRONMENT_ROOT/release-request.json" \
        "$ENVIRONMENT_ROOT/release-request.json.asc" \
        "$APPROVER_POLICY" "$gpg_home" "$cycle_root" "$candidate_release" \
        "$REPOSITORY_ROOT" "$ENVIRONMENT_ROOT/candidate-semantic-evidence.json" \
        "$PRODUCTION_PROFILE" "$PRODUCTION_PROFILE" "$IDL_HISTORY" "$TARGET_TRIPLE" \
        "$RELEASE_PROBE" "$ENVIRONMENT_ROOT/registry_probe.sig" \
        "$ENVIRONMENT_ROOT/rust-toolchain.json" "$ENVIRONMENT_ROOT/runner-image.json" \
        "$ENVIRONMENT_ROOT/registry-trust-policy.json" \
        "$RAW_EVIDENCE_ROOT/failure-qualification.json"
}

run_kernel() {
    build_kernel_tools
    compute_closure >/dev/null
    if [[ "$QUALIFICATION_MODE" == "prequalification" ]]; then
        run_kernel_prequalification
    else
        run_kernel_release
    fi
}

write_prequalification_summary() {
    local closure
    closure="$(tr -d '\r\n' <"$EVIDENCE_ROOT/$REGISTRY_CLOSURE_DIGEST_FILE")"
    python3 - \
        "$RAW_EVIDENCE_ROOT" \
        "$EVIDENCE_ROOT/component-summary.json" \
        "$closure" \
        "$PRODUCTION_PROFILE" \
        "$REPOSITORY_ROOT/scripts/concept_registry" <<'PY'
from __future__ import annotations

import json
import sys
from pathlib import Path

import blake3

raw_root = Path(sys.argv[1])
output = Path(sys.argv[2])
closure = sys.argv[3]
profile = json.loads(Path(sys.argv[4]).read_text(encoding="utf-8"))
sys.path.insert(0, sys.argv[5])
from production_qualification import _verify_receipt

policy = profile["trust_policy"]["policy"]
required = [
    "resource-cold-cache.json",
    "resource-low-ram.json",
    "resource-ssd.json",
    "resource-hdd.json",
    "failure-qualification.json",
    "ccid-stability.json",
    "live-reader-process-kill.log",
]
missing = [name for name in required if not (raw_root / name).is_file()]
if missing:
    raise SystemExit(f"prequalification raw report missing: {missing[0]}")

digests = {
    name: blake3.blake3((raw_root / name).read_bytes()).hexdigest()
    for name in required
}
roots = set()
component_results = []
for name in required[:5]:
    report = json.loads((raw_root / name).read_text(encoding="utf-8"))
    kind, payload = _verify_receipt(report, profile, policy)
    expected_kind = (
        "resource-qualification"
        if name.startswith("resource-")
        else "failure-qualification"
    )
    if kind != expected_kind:
        raise SystemExit(f"{name} receipt kind mismatch")
    if payload.get("qualification_context_variant") != "Prequalification":
        raise SystemExit(f"{name} is not prequalification evidence")
    if payload.get("evidence_tier") != "prequalification":
        raise SystemExit(f"{name} evidence tier mismatch")
    if payload.get("closure_digest") != closure:
        raise SystemExit(f"{name} closure mismatch")
    if payload.get("base_candidate_bound") is not False:
        raise SystemExit(f"{name} candidate-binding claim is forbidden")
    roots.add(payload.get("release_aggregate_root"))
    component_results.append(payload.get("result") is True)
ccid = json.loads((raw_root / "ccid-stability.json").read_text(encoding="utf-8"))
component_results.append(ccid.get("qualified") is True)
component_results.append((raw_root / "live-reader-process-kill.log").stat().st_size > 0)
if len(roots) != 1 or None in roots:
    raise SystemExit("prequalification components do not bind one release root")
summary = {
    "format": "onebrain/concept-registry-component-prequalification-summary/1",
    "registry_closure_digest": closure,
    "release_aggregate_root": next(iter(roots)),
    "raw_report_blake3": digests,
    "component_mismatch_count": 0,
    "component_qualified": all(component_results),
    "base_candidate_bound": False,
    "registry_production_qualified": False,
    "limitations": [
        "Task 21 component prequalification only",
        "Not a Task 28 fresh exact-candidate production aggregate",
    ],
}
output.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8")
if not summary["component_qualified"]:
    raise SystemExit("one or more prequalification components failed")
PY
}

run_aggregate() {
    build_release_probe
    compute_closure >/dev/null
    mkdir -p "$RAW_EVIDENCE_ROOT"
    if [[ "$QUALIFICATION_MODE" == "prequalification" ]]; then
        write_prequalification_summary
        return
    fi
    verify_release_request
    local private_key="${ONEBRAIN_REGISTRY_PRIVATE_KEY_FILE:?external Registry signing key path is required}"
    require_external_secret_path "$private_key" "Registry signing key"
    mapfile -t receipts < <(find "$RAW_EVIDENCE_ROOT" -maxdepth 1 -type f -name '*.json' -print | sort)
    [[ ${#receipts[@]} -gt 0 ]] || die "no raw release receipts were supplied"
    local receipt_args=()
    local receipt
    for receipt in "${receipts[@]}"; do
        receipt_args+=(--receipt "$receipt")
    done
    python3 "$REPOSITORY_ROOT/scripts/concept_registry/production_qualification.py" \
        --profile "$PRODUCTION_PROFILE" \
        --run-context "$EVIDENCE_ROOT/release-run-context.json" \
        "${receipt_args[@]}" \
        --aggregate-private-key "$private_key" \
        --output "$EVIDENCE_ROOT/production-aggregate.json"
}

main() {
    parse_arguments "$@"
    case "$COMMAND" in
        preflight) run_preflight ;;
        closure) compute_closure ;;
        build) build_release_probe ;;
        resource) run_resource ;;
        kernel) run_kernel ;;
        aggregate) run_aggregate ;;
    esac
}

main "$@"
