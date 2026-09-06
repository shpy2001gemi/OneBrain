"""KU-ENC-003 read-only inventory. Never runs inference or grants qualification.

Only /api/version and /api/show on literal loopback are queried. Local model
blobs are streamed through SHA-256; filenames/tags alone are not evidence.
Detailed output is private, outside the repository, and cannot be overwritten.
"""
from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import urllib.request

ROOT = Path(__file__).resolve().parents[2]
BUNDLE = Path("docs/specs/vnext/ku-encoder-v1")


def digest(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def strict_json(raw: bytes):
    def pairs(items):
        result = {}
        for key, value in items:
            if key in result:
                raise ValueError("duplicate_json_key")
            result[key] = value
        return result

    def invalid_constant(_):
        raise ValueError("nonfinite_json")

    return json.loads(raw, object_pairs_hook=pairs, parse_constant=invalid_constant)


def bundle_pins(root: Path) -> dict:
    manifest = strict_json((root / BUNDLE / "bundle.manifest.json").read_bytes())
    pins = {}
    for name, expected in manifest["artifacts"].items():
        path = (root / name).resolve()
        if not path.is_relative_to(root.resolve()):
            raise ValueError("bundle_path_escape")
        observed = digest(path)
        if observed != expected:
            raise ValueError("bundle_drift")
        pins[name] = observed
    pins[(BUNDLE / "bundle.manifest.json").as_posix()] = digest(
        root / BUNDLE / "bundle.manifest.json"
    )
    return pins


def verify_layer(home: Path, layer: dict) -> dict:
    expected = layer["digest"]
    if not re.fullmatch(r"sha256:[0-9a-f]{64}", expected):
        raise ValueError("invalid_blob_digest")
    path = (home / "blobs" / expected.replace(":", "-")).resolve()
    if not path.is_relative_to(home.resolve()):
        raise ValueError("blob_path_escape")
    size = path.stat().st_size
    if type(layer["size"]) is not int or size != layer["size"]:
        raise ValueError("blob_size_mismatch")
    observed = digest(path)
    if observed != expected[7:]:
        raise ValueError("blob_digest_mismatch")
    return {"media_type": layer["mediaType"], "bytes": size, "sha256": observed}


def local_model(home: Path, tag: str) -> dict:
    # Deliberately supports only explicit library name:tag, never URLs or latest.
    if not re.fullmatch(r"[a-zA-Z0-9_.-]+:[a-zA-Z0-9_.-]+", tag):
        raise ValueError("explicit_library_tag_required")
    name, revision = tag.split(":")
    path = (home / "manifests/registry.ollama.ai/library" / name / revision).resolve()
    if not path.is_relative_to(home.resolve()):
        raise ValueError("manifest_path_escape")
    raw = path.read_bytes()
    if len(raw) > 1_048_576:
        raise ValueError("manifest_too_large")
    manifest = strict_json(raw)
    layers = [verify_layer(home, item) for item in [manifest["config"], *manifest["layers"]]]
    if not any(item["media_type"] == "application/vnd.ollama.image.model" for item in layers):
        raise ValueError("model_layer_missing")
    if path.read_bytes() != raw:
        raise ValueError("model_manifest_changed")
    return {"tag": tag, "manifest_sha256": hashlib.sha256(raw).hexdigest(), "layers": layers}


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        raise ValueError("redirect_forbidden")


def metadata(route: str, tag: str | None = None):
    if (route, tag is not None) not in {("version", False), ("show", True)}:
        raise ValueError("metadata_only")
    body = json.dumps({"model": tag}).encode() if tag is not None else None
    request = urllib.request.Request(
        "http://127.0.0.1:11434/api/" + route, data=body,
        headers={"Content-Type": "application/json"},
    )
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect())
    with opener.open(request, timeout=10) as response:
        raw = response.read(8_388_609)
        if len(raw) > 8_388_608:
            raise ValueError("metadata_too_large")
    return strict_json(raw)


def collect(home: Path, tags: list[str]) -> dict:
    pins = bundle_pins(ROOT)
    native = [*sorted((ROOT / "src/ku-encoder/src/extraction").glob("*.rs")),
              ROOT / "src/ku-ai/src/backend/ollama.rs", ROOT / "src/onebrain-node/src/ku_extraction.rs",
              ROOT / "src/Cargo.lock", Path(__file__)]
    for path in native:
        pins[path.relative_to(ROOT).as_posix()] = digest(path)
    profile = strict_json((ROOT / BUNDLE / "profile.json").read_bytes())
    executable = shutil.which("ollama")
    models = []
    for tag in tags:
        entry = local_model(home, tag)
        info = metadata("show", tag)
        # /show is metadata only; no tokenizer execution, load or inference.
        entry.update({"details": info.get("details"),
                      "chat_template_sha256": hashlib.sha256(info.get("template", "").encode()).hexdigest(),
                      "chat_template_present": bool(info.get("template")),
                      "metadata_sha256": hashlib.sha256(json.dumps(info, sort_keys=True).encode()).hexdigest(),
                      "model_info": info.get("model_info"),
                      "parameters": info.get("parameters"),
                      "qualified": False})
        models.append(entry)
    return {
        "format": "ku-enc-003-preflight/1", "created_utc": dt.datetime.now(dt.timezone.utc).isoformat(),
        "git_head": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "evidence_class": "local_artifact_inventory_not_inference", "qualified": False,
        "holdout_executed": False, "inference_calls": 0, "qualified_tuples": [],
        "host": {"os": platform.platform(), "machine": platform.machine(),
                 "processor": platform.processor(), "python": platform.python_version()},
        "backend": {"reported": metadata("version"),
                    "client_binary_sha256": digest(Path(executable)) if executable else None,
                    "worker_build_verified": False},
        "artifact_sha256": pins, "thresholds": profile["qualification"],
        "resource_profiles": profile["resource_profiles"], "models": models,
        "missing_evidence": ["locked_independent_holdout_and_evaluator_provenance",
                             "authenticated_source_and_registry_run_bindings",
                             "exact_executable_tokenizer_and_chat_wrapper_accounting",
                             "backend_worker_build_and_managed_cancellation",
                             "repeated_real_model_quality_and_agreement",
                             "worker_plus_host_peak_memory_kv_and_oom",
                             "cold_warm_latency_and_process_death",
                             "isolated_legacy_ablations", "physical_mobile_MOB_06_07"],
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--models-home", type=Path,
                        default=Path(os.environ.get("OLLAMA_MODELS", Path.home() / ".ollama/models")))
    parser.add_argument("--model", action="append", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    target = args.output.resolve()
    if target.is_relative_to(ROOT):
        parser.error("Detailed inventory must be stored outside the repository")
    if target.exists():
        parser.error("Refusing to overwrite an evidence file")
    try:
        report = collect(args.models_home, args.model)
        target.parent.mkdir(parents=True, exist_ok=True)
        with target.open("x", encoding="utf-8", newline="\n") as stream:
            json.dump(report, stream, ensure_ascii=False, indent=2)
            stream.write("\n")
    except (OSError, ValueError, KeyError, TypeError):
        print("Preflight failed; no qualification granted. Check local artifacts and metadata service.")
        return 1
    print("Inventory recorded; 0 inference calls; 0 qualified tuples. Qualification remains blocked.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
