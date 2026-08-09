#!/usr/bin/env python3
"""Fail-closed Base v1 IDL/Rust/header/toolchain conformance gate."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
IDL = Path("src/test-vectors/vnext/base-v1-runtime-interface-v1.json")
RUST = Path("src/onebrain-base-abi/src/lib.rs")
HEADER = Path("src/onebrain-base-abi/include/onebrain_base_v1.h")
CONFIG = Path("src/onebrain-base-abi/cbindgen.toml")
LOCK = Path("scripts/toolchains/base-v1-tools.lock.json")

TASK18_ALIASES = {
    "ob_base_capabilities_v1",
    "ob_base_archive_source_push_v1",
    "ob_base_archive_sink_read_v1",
    "ob_base_complete_reprovision_v1",
    "ob_base_buffer_free_v1",
}


class ValidationError(RuntimeError):
    pass


def load_json(path: Path) -> dict:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise ValidationError(f"cannot read canonical JSON {path}: {exc}") from exc


def idl_descriptor(document: dict) -> dict:
    projection = document.get("projection_rules", {})
    mappings = projection.get("operation_mapping", [])
    operations = document.get("operations", [])
    errors = document.get("errors", [])
    if projection.get("source") != "machine_idl_only":
        raise ValidationError("machine IDL is not the sole projection source")
    if projection.get("abi_struct_rule") != "every_input_output_and_error_struct_starts_with_u32_struct_size":
        raise ValidationError("machine IDL no longer freezes the C struct_size prefix")
    by_name = {item["name"]: item["id"] for item in operations}
    mapped = {item["operation"]: item["c_abi"] for item in mappings}
    if set(by_name) != set(mapped):
        raise ValidationError("operation-to-C mapping is incomplete or contains extras")
    limits = document.get("limits", {})
    return {
        "operations": [
            {"id": by_name[name], "name": name, "symbol": mapped[name]}
            for name in sorted(by_name, key=lambda name: by_name[name])
        ],
        "errors": [
            {
                "id": item["id"],
                "name": item["name"],
                "retryable": bool(item["retryable"]),
                "reconcile_before_retry": bool(item["reconcile_before_retry"]),
            }
            for item in sorted(errors, key=lambda item: item["id"])
        ],
        "limits": {
            key: limits[key]
            for key in (
                "max_payload_bytes",
                "max_continuation_bytes",
                "max_archive_chunk_bytes",
                "max_management_scopes",
                "max_active_operations",
            )
        },
        "ownership": projection.get("forbidden_exposures", []),
    }


def canonical_machine_descriptor(document: dict) -> dict:
    """Independently retain every cross-projection semantic ABI input.

    This deliberately excludes release/baseline metadata while retaining all
    field widths, bounds, ownership labels, discriminators, operations,
    errors, and lifecycle rules that can change a generated projection.
    """

    required = (
        "wire",
        "limits",
        "scalar_types",
        "common_cross_projection_fields",
        "requests",
        "responses",
        "errors",
        "command_kinds",
        "topic_kinds",
        "type_definitions",
        "operations",
        "runtime_lifecycle",
        "projection_rules",
    )
    missing = [key for key in required if key not in document]
    if missing:
        raise ValidationError(f"machine IDL descriptor is missing {missing}")
    return {key: document[key] for key in required}


def machine_descriptor_sha256(document: dict) -> str:
    canonical = json.dumps(
        canonical_machine_descriptor(document),
        sort_keys=True,
        separators=(",", ":"),
        ensure_ascii=False,
    ).encode("utf-8")
    return hashlib.sha256(canonical).hexdigest()


def required_symbols(document: dict) -> set[str]:
    descriptor = idl_descriptor(document)
    return {item["symbol"] for item in descriptor["operations"]} | TASK18_ALIASES


def rust_symbols(text: str) -> set[str]:
    direct = set(re.findall(r"(?:fn|ordinary_symbol!\(|management_symbol!\()\s*(ob_base_[a-z0-9_]+_v1)", text))
    return direct


def header_symbols(text: str) -> set[str]:
    return set(re.findall(r"\b(ob_base_[a-z0-9_]+_v1)\s*\(", text))


def validate_public_structs(header: str) -> None:
    structs = re.findall(r"typedef struct (ObBase\w+V1)\s*\{(.*?)\}\s*\1;", header, re.S)
    if not structs:
        raise ValidationError("header contains no public Base v1 structs")
    required_prefix = [
        r"uint32_t\s+struct_size\s*;",
        r"uint16_t\s+abi_major\s*;",
        r"uint16_t\s+abi_minor\s*;",
    ]
    for name, body in structs:
        cursor = 0
        for pattern in required_prefix:
            match = re.search(pattern, body[cursor:])
            if not match or body[cursor : cursor + match.start()].strip():
                raise ValidationError(f"{name} does not begin with the frozen size/ABI prefix")
            cursor += match.end()
    forbidden = ("std::", "Vec<", "String", "Path", "dyn ", "private_key", "runtime_reference")
    for token in forbidden:
        if token in header:
            raise ValidationError(f"forbidden C ABI exposure: {token}")

    expected_fields = {
        "ObBaseOpenRequestV1": [
            "uint32_t struct_size;",
            "uint16_t abi_major;",
            "uint16_t abi_minor;",
            "uint8_t registration_token[32];",
            "uint8_t host_trust_digest[32];",
        ],
        "ObBaseCallV1": [
            "uint32_t struct_size;",
            "uint16_t abi_major;",
            "uint16_t abi_minor;",
            "uint8_t process_generation[32];",
            "uint8_t dataset_generation[32];",
            "uint8_t request_id[32];",
            "uint8_t operation_id[32];",
            "uint8_t auxiliary_id[32];",
            "uint16_t discriminator;",
            "uint16_t flags;",
            "uint64_t value0;",
            "uint64_t value1;",
            "const uint8_t *payload_ptr;",
            "size_t payload_len;",
        ],
        "ObBaseOutputV1": [
            "uint32_t struct_size;",
            "uint16_t abi_major;",
            "uint16_t abi_minor;",
            "uint8_t process_generation[32];",
            "uint8_t dataset_generation[32];",
            "uint16_t response_discriminator;",
            "uint16_t reserved;",
            "uint8_t operation_id[32];",
            "uint8_t *buffer_ptr;",
            "size_t buffer_capacity;",
            "size_t required_len;",
            "size_t written_len;",
        ],
        "ObBaseErrorV1": [
            "uint32_t struct_size;",
            "uint16_t abi_major;",
            "uint16_t abi_minor;",
            "uint16_t code;",
            "uint8_t retryable;",
            "uint8_t reconcile_before_retry;",
            "uint16_t reserved;",
            "const uint8_t *message_ptr;",
            "size_t message_len;",
            "uint64_t allocation_tag;",
        ],
        "ObBaseOwnedBufferV1": [
            "uint32_t struct_size;",
            "uint16_t abi_major;",
            "uint16_t abi_minor;",
            "const uint8_t *ptr;",
            "size_t len;",
            "uint64_t allocation_tag;",
        ],
    }
    actual = {
        name: [re.sub(r"\s+", " ", line.strip()) for line in body.splitlines() if line.strip()]
        for name, body in structs
    }
    if actual != expected_fields:
        raise ValidationError(
            f"public C field width/order drift: expected={expected_fields}, actual={actual}"
        )


def validate_descriptor_binding(document: dict, rust: str, header: str) -> None:
    expected = machine_descriptor_sha256(document)
    rust_match = re.search(
        r"OB_BASE_IDL_DESCRIPTOR_SHA256_V1:\s*\[u8;\s*32\]\s*=\s*\[(.*?)\];",
        rust,
        re.S,
    )
    header_match = re.search(
        r"#define\s+OB_BASE_IDL_DESCRIPTOR_SHA256_V1\s+\{(.*?)\}",
        header,
        re.S,
    )
    rust_digest = None
    header_digest = None
    if rust_match:
        try:
            rust_digest = bytes(
                int(value.strip(), 0)
                for value in rust_match.group(1).split(",")
                if value.strip()
            ).hex()
        except ValueError:
            pass
    if header_match:
        try:
            header_digest = bytes(
                int(value.strip(), 10)
                for value in header_match.group(1).split(",")
                if value.strip()
            ).hex()
        except ValueError:
            pass
    if rust_digest != expected:
        raise ValidationError("Rust ABI is not bound to the complete machine descriptor")
    if header_digest != expected:
        raise ValidationError("C header is not bound to the complete machine descriptor")


def host_key() -> str:
    system = platform.system().lower()
    machine = platform.machine().lower()
    aliases = {"amd64": "x86_64", "x64": "x86_64", "aarch64": "arm64"}
    return f"{system}-{aliases.get(machine, machine)}"


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def resolve_pinned_cbindgen(root: Path, lock: dict) -> Path:
    configured = os.environ.get("ONEBRAIN_BASE_CBINDGEN")
    cbindgen = configured or shutil.which("cbindgen")
    if not cbindgen:
        raise ValidationError("pinned cbindgen executable is unavailable")
    path = Path(cbindgen).resolve()
    version = subprocess.run(
        [str(path), "--version"], capture_output=True, text=True, check=True
    ).stdout.strip().split()[-1]
    expected = lock.get("cbindgen", {})
    if version != expected.get("version"):
        raise ValidationError(f"cbindgen version mismatch: expected {expected.get('version')}, got {version}")
    host = expected.get("hosts", {}).get(host_key())
    if not host:
        raise ValidationError(f"no pinned cbindgen executable for host {host_key()}")
    actual_hash = sha256(path)
    if actual_hash != host.get("sha256"):
        raise ValidationError(
            f"cbindgen executable hash mismatch for {host_key()}: expected {host.get('sha256')}, got {actual_hash}"
        )
    for field in ("distribution", "distribution_sha256", "install"):
        if not host.get(field):
            raise ValidationError(f"missing {field} for {host_key()}")
    return path


def validate(root: Path = ROOT, verify_tool: bool = True) -> None:
    document = load_json(root / IDL)
    descriptor = idl_descriptor(document)
    rust = (root / RUST).read_text(encoding="utf-8")
    header = (root / HEADER).read_text(encoding="utf-8")
    lock = load_json(root / LOCK)
    expected = required_symbols(document)
    rust_found = rust_symbols(rust)
    header_found = header_symbols(header)
    if rust_found != expected:
        raise ValidationError(
            f"Rust ABI symbol drift: missing={sorted(expected-rust_found)}, extra={sorted(rust_found-expected)}"
        )
    if header_found != expected:
        raise ValidationError(
            f"header symbol drift: missing={sorted(expected-header_found)}, extra={sorted(header_found-expected)}"
        )
    validate_public_structs(header)
    validate_descriptor_binding(document, rust, header)
    if "MAX_C_PAYLOAD: usize = 1_048_576" not in rust:
        raise ValidationError("Rust ABI payload bound does not match the machine IDL")
    if descriptor["limits"]["max_payload_bytes"] != 1_048_576:
        raise ValidationError("machine IDL payload bound changed without an ABI revision")
    if "OB_BASE_ABI_MAJOR_V1 1" not in header or "OB_BASE_ABI_MINOR_V1 0" not in header:
        raise ValidationError("header ABI version constants drifted")
    if not verify_tool:
        return
    cbindgen = resolve_pinned_cbindgen(root, lock)
    with tempfile.TemporaryDirectory(prefix="onebrain-base-abi-") as directory:
        generated = Path(directory) / "onebrain_base_v1.h"
        subprocess.run(
            [
                str(cbindgen),
                "--config",
                "onebrain-base-abi/cbindgen.toml",
                "--crate",
                "onebrain-base-abi",
                "--output",
                str(generated),
            ],
            cwd=root / "src",
            check=True,
        )
        if generated.read_bytes() != (root / HEADER).read_bytes():
            raise ValidationError("checked-in header differs from pinned cbindgen output")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--skip-tool-verification", action="store_true")
    args = parser.parse_args()
    try:
        validate(args.root.resolve(), verify_tool=not args.skip_tool_verification)
    except (ValidationError, OSError, subprocess.CalledProcessError) as exc:
        print(f"Base ABI validation failed: {exc}", file=sys.stderr)
        return 1
    print("Base ABI header, IDL descriptor, symbols, and pinned toolchain are valid.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
