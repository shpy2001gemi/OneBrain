"""Private, preparation-only export of pinned workbook source cells.

This is not SourceArtifact capture, a source grant, a run lock or a provider.
No labels are exported. The decoded cell string is encoded as strict UTF-8;
no trimming, Unicode normalization, newline conversion or BOM is added.
Workbook/decoder/cell bindings describe the transformation, not upstream origin.
"""
from __future__ import annotations

import argparse
from collections import Counter
import io
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
import unicodedata
import zipfile

from scripts.encoder.qualification_package_audit import (
    ROOT, allowed_path, audit, is_digest, local_path, require, sha256,
    snapshot_json, strict_json, verify_pin,
)

MAX_BOOK_BYTES = 32 * 1024 * 1024
MAX_XML_BYTES = 64 * 1024 * 1024
MAX_ROWS = 4096
MAX_COLUMNS = 128
MAX_SOURCE_BYTES = 786432


def pin(path):
    raw = path.read_bytes()
    return {"path": str(path), "bytes": len(raw), "sha256": sha256(raw)}


def write_new(path, raw):
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, "wb") as stream:
        stream.write(raw)
        stream.flush()
        os.fsync(stream.fileno())


def json_bytes(value):
    return (json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2) + "\n").encode("utf-8")


def private_directory(path):
    """Create a fresh owner-only directory before writing any private bytes."""
    path.mkdir(mode=0o700, parents=False, exist_ok=False)
    restrict_directory(path)


def restrict_directory(path):
    """Replace the DACL, including explicit defaults added by Windows mkdir."""
    if os.name == "nt":
        # Fixed code, path passed as data through the child environment, not
        # interpolated shell text. Do not preserve implicit Administrators ACEs.
        script = r"""
$ErrorActionPreference = 'Stop'
$target = $env:KU_PREPARATION_PRIVATE_DIRECTORY
$ownerSid = [System.Security.Principal.WindowsIdentity]::GetCurrent().User
$systemSid = [System.Security.Principal.SecurityIdentifier]::new('S-1-5-18')
$acl = [System.Security.AccessControl.DirectorySecurity]::new()
$acl.SetAccessRuleProtection($true, $false)
foreach ($sid in @($ownerSid, $systemSid)) {
    $rule = [System.Security.AccessControl.FileSystemAccessRule]::new(
        $sid, 'FullControl', 'ContainerInherit,ObjectInherit', 'None', 'Allow')
    $acl.AddAccessRule($rule)
}
[System.IO.Directory]::SetAccessControl($target, $acl)
$actual = [System.IO.Directory]::GetAccessControl($target)
$allowed = @($ownerSid.Value, $systemSid.Value)
if (-not $actual.AreAccessRulesProtected -or $actual.Access.Count -ne 2) { throw 'private_acl_failed' }
foreach ($rule in $actual.Access) {
    if ($rule.IdentityReference.Translate([System.Security.Principal.SecurityIdentifier]).Value -notin $allowed) {
        throw 'private_acl_failed'
    }
}
"""
        environment = dict(os.environ, KU_PREPARATION_PRIVATE_DIRECTORY=str(path))
        subprocess.run(["powershell.exe", "-NoProfile", "-NonInteractive", "-Command", script],
                       env=environment, capture_output=True, check=True,
                       creationflags=subprocess.CREATE_NO_WINDOW)
    else:
        os.chmod(path, 0o700)


def workbook_snapshot(book_pin, roots):
    require(type(book_pin.get("bytes")) is int and 0 < book_pin["bytes"] <= MAX_BOOK_BYTES,
            "workbook_size_limit")
    path = allowed_path(book_pin["path"], roots)
    with path.open("rb") as stream:
        raw = stream.read(MAX_BOOK_BYTES + 1)
    require(len(raw) == book_pin["bytes"] and sha256(raw) == book_pin["sha256"],
            "workbook_binding_mismatch")
    # Bound decompression before the spreadsheet library allocates shared strings.
    with zipfile.ZipFile(io.BytesIO(raw)) as archive:
        members = archive.infolist()
        require(len(members) <= 4096 and len({m.filename for m in members}) == len(members),
                "invalid_workbook_archive")
        require(sum(m.file_size for m in members) <= MAX_XML_BYTES,
                "workbook_expansion_limit")
        require(not any(m.flag_bits & 1 for m in members), "encrypted_workbook")
    return raw


def source_cells(raw):
    # Read-only extraction; never evaluate formulas or open the annotation sheet.
    import openpyxl
    book = openpyxl.load_workbook(io.BytesIO(raw), read_only=True,
                                 data_only=False, keep_links=False)
    try:
        require("Nguon" in book.sheetnames, "source_sheet_missing")
        sheet = book["Nguon"]
        require(sheet.max_row is not None and sheet.max_row <= MAX_ROWS and
                sheet.max_column is not None and sheet.max_column <= MAX_COLUMNS,
                "sheet_dimensions_out_of_bounds")
        header = list(sheet.iter_rows(min_row=5, max_row=5))[0]
        names = [str(c.value or "").split("\n")[0].strip() for c in header]
        required = ("source_id", "source_text", "language")
        require(all(names.count(n) == 1 for n in required), "source_headers_invalid")
        indices = {n: names.index(n) for n in required}
        for row in sheet.iter_rows(min_row=6):
            selected = {n: row[i] for n, i in indices.items()}
            if all(c.value is None for c in selected.values()):
                continue
            require(all(c.data_type != "f" for c in selected.values()), "formula_source_cell")
            require(all(isinstance(c.value, str) and c.value for c in selected.values()),
                    "source_cell_must_be_nonempty_text")
            require(selected["language"].value in ("vi", "en"), "invalid_language")
            text = selected["source_text"].value
            encoded = text.encode("utf-8", errors="strict")
            require(0 < len(encoded) <= MAX_SOURCE_BYTES, "source_byte_limit")
            yield {
                "source_id_sha256": sha256(selected["source_id"].value.encode("utf-8")),
                "language": selected["language"].value,
                "cell": selected["source_text"].coordinate,
                "id_cell": selected["source_id"].coordinate,
                "nfc_comparison_sha256": sha256(unicodedata.normalize("NFC", text).encode("utf-8")),
            }, encoded
    finally:
        book.close()


def export_sources(draft_path, input_roots, destination, root=ROOT):
    import openpyxl
    root = local_path(root)
    roots = [root, *(local_path(p) for p in input_roots)]
    draft_path = allowed_path(draft_path, roots)
    destination = local_path(destination)
    require(not destination.is_relative_to(root), "private_output_required")
    require(destination.parent.is_dir(), "output_parent_missing")
    require(not destination.exists(), "output_exists")
    draft_raw = draft_path.read_bytes()
    draft = strict_json(draft_raw)
    preparation = audit(draft_path, input_roots, root)
    require(draft_path.read_bytes() == draft_raw, "draft_changed")
    intake = snapshot_json(draft["intake"], roots)
    pending = []
    seen = set()
    books = []
    for book in intake["workbooks"]:
        snapshot = workbook_snapshot(book, roots)
        expected = {r["source_id_sha256"]: r["source_text_sha256"] for r in book["source_commitments"]}
        extracted = list(source_cells(snapshot))
        require(len(extracted) == book["source_count"], "export_row_count_mismatch")
        require(Counter(r["language"] for r, _ in extracted) == book["language_counts"],
                "export_language_count_mismatch")
        found = set()
        for row, encoded in extracted:
            identity = row["source_id_sha256"]
            require(identity not in seen, "duplicate_export_source_id")
            require(identity in expected and expected[identity] == row["nfc_comparison_sha256"],
                    "intake_source_binding_mismatch")
            seen.add(identity)
            found.add(identity)
            row.update({"workbook_sha256": book["sha256"], "sheet": "Nguon",
                        "relative_path": f"sources/{identity}.utf8",
                        "bytes": len(encoded), "utf8_sha256": sha256(encoded)})
            pending.append((row, encoded))
        require(found == set(expected), "export_id_set_mismatch")
        books.append({k: book[k] for k in ("path", "bytes", "sha256")})
    # Validate everything before creating output; failures after creation retain
    # the incomplete directory. Only a final completion.json marks a complete export.
    private_directory(destination)
    private_directory(destination / "sources")
    for row, encoded in pending:
        write_new(destination / row["relative_path"], encoded)
    transformation = {
        "kind": "ku-enc-003-source-export-transformation/1",
        "input": "OOXML Nguon row-5 headers; source_text text cells only",
        "decoder": {"library": "openpyxl", "version": openpyxl.__version__,
                    "python": platform.python_version(), "unicode": unicodedata.unidata_version},
        "operation": "OOXML-decoded cell string -> strict UTF-8 bytes",
        "normalization": "none", "trim": False, "newline_conversion": False,
        "added_bom": False, "formula_evaluation": False, "labels_exported": False,
        "original_scope": "Exact decoded workbook cell, not a claim about pre-workbook upstream bytes",
        "nfc_hash_purpose": "Comparison with old intake only; never the exact-byte binding",
        "exporter": pin(Path(__file__).resolve()),
        "authority": "Preparation only; no SourceArtifact or processing grant created",
    }
    write_new(destination / "transformation.json", json_bytes(transformation))
    manifest = {
        "kind": "ku-enc-003-source-export/1", "execution_admitted": False,
        "model_qualified": False, "locked": False, "inference_calls": 0,
        "draft_sha256": sha256(draft_raw), "intake": draft["intake"],
        "workbooks": books, "sources": [r for r, _ in pending],
        "transformation": pin(destination / "transformation.json"),
        "preparation_audit": preparation,
    }
    write_new(destination / "source-manifest.json", json_bytes(manifest))
    custody = {
        "kind": "ku-enc-003-custody-request/1", "status": "unresolved",
        "source_manifest": pin(destination / "source-manifest.json"),
        "custodian": None, "processing_permission_evidence": None,
        "exposure_history": None, "provenance_and_grouping_review": None,
        "authorized_principal": None, "purpose": None, "dataset_generation": None,
        "process_generation": None, "retention_policy": None, "revocation_policy": None,
        "authorization_assessment": None, "source_artifacts": [], "issued_grants": [],
        "note": "No native custody installation until actual governance assessment is Authorized",
    }
    write_new(destination / "custody-request.json", json_bytes(custody))
    # Rehash original input files after extraction, without modifying old evidence.
    for book in books:
        verify_pin(book, roots)
    completion = {
        "kind": "ku-enc-003-source-export-completion/1", "execution_admitted": False,
        "manifest": pin(destination / "source-manifest.json"),
        "custody_request": pin(destination / "custody-request.json"),
        "source_count": len(pending), "issued_grants": 0,
    }
    verify_export(completion["manifest"], roots + [destination], root)
    write_new(destination / "completion.json", json_bytes(completion))
    return completion


def verify_export(manifest_pin, roots, root=ROOT):
    """Verify exact bytes and old NFC bindings; return counts, never source text."""
    roots = [local_path(p) for p in roots]
    path = allowed_path(manifest_pin["path"], roots)
    require(not path.is_relative_to(local_path(root)), "private_manifest_required")
    manifest = snapshot_json(manifest_pin, roots)
    require(manifest.get("kind") == "ku-enc-003-source-export/1" and
            manifest.get("locked") is False and manifest.get("model_qualified") is False and
            manifest.get("execution_admitted") is False, "invalid_export_status")
    transformation = snapshot_json(manifest["transformation"], [path.parent])
    require(transformation.get("operation") == "OOXML-decoded cell string -> strict UTF-8 bytes" and
            transformation.get("normalization") == "none" and
            all(transformation.get(k) is False for k in
                ("trim", "newline_conversion", "added_bom", "formula_evaluation", "labels_exported")),
            "invalid_transformation")
    intake = snapshot_json(manifest["intake"], roots)
    expected = {}
    for book in intake["workbooks"]:
        cells = {r["source_id_sha256"]: (r, raw) for r, raw in
                 source_cells(workbook_snapshot(book, roots))}
        require(len(cells) == len(book["source_commitments"]), "workbook_id_count_mismatch")
        for row in book["source_commitments"]:
            key = row["source_id_sha256"]
            require(key not in expected and key in cells, "duplicate_or_missing_intake_id")
            cell, raw = cells[key]
            require(cell["nfc_comparison_sha256"] == row["source_text_sha256"], "intake_cell_mismatch")
            expected[key] = {**cell, "workbook_sha256": book["sha256"],
                             "utf8_sha256": sha256(raw), "bytes": len(raw), "sheet": "Nguon"}
    seen = set()
    counts = Counter()
    for row in manifest["sources"]:
        identity = row["source_id_sha256"]
        require(is_digest(identity) and identity not in seen, "duplicate_or_invalid_export_id")
        require(identity in expected and all(row.get(k) == v for k, v in expected[identity].items()),
                "export_workbook_mismatch")
        require(row["relative_path"] == f"sources/{identity}.utf8", "source_path_invalid")
        require(row["language"] in ("vi", "en") and type(row["bytes"]) is int and
                0 < row["bytes"] <= MAX_SOURCE_BYTES, "invalid_source_metadata")
        source_pin = {"path": str(path.parent / row["relative_path"]),
                      "bytes": row["bytes"], "sha256": row["utf8_sha256"]}
        source = verify_pin(source_pin, [path.parent / "sources"])
        raw = source.read_bytes()
        require(sha256(raw) == row["utf8_sha256"], "source_changed")
        text = raw.decode("utf-8", errors="strict")
        require(sha256(unicodedata.normalize("NFC", text).encode("utf-8")) == row["nfc_comparison_sha256"],
                "nfc_comparison_mismatch")
        seen.add(identity)
        counts[row["language"]] += 1
    require(seen == set(expected), "export_sources_missing")
    actual = {p.relative_to(path.parent).as_posix() for p in (path.parent / "sources").iterdir()}
    require(actual == {r["relative_path"] for r in manifest["sources"]}, "extra_source_files")
    return {"verified_sources": len(seen), "languages": dict(counts), "execution_admitted": False}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--draft", type=Path, required=True)
    parser.add_argument("--input-root", type=Path, action="append", required=True)
    parser.add_argument("--output-directory", type=Path, required=True)
    args = parser.parse_args()
    try:
        result = export_sources(args.draft, args.input_root, args.output_directory)
    except Exception:
        # Library and filesystem exceptions can contain source values or paths.
        print("Source export failed; execution blocked; incomplete output retained if created.")
        return 1
    print(f"Private source export complete: {result['source_count']} sources; 0 grants; execution BLOCKED.")
    return 2


if __name__ == "__main__":
    sys.exit(main())
