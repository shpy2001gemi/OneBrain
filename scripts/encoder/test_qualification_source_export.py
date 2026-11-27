"""Synthetic OOXML only; no private holdout values or model execution."""
import contextlib
import io
import json
import os
from pathlib import Path
import unittest
from unittest.mock import patch
import unicodedata
from xml.sax.saxutils import escape
import zipfile

from scripts.encoder import qualification_source_export as export
from scripts.encoder import test_qualification_package_audit as fixtures


def workbook(rows, formula=False, headers=None):
    """Minimal synthetic OOXML fixture, not an authored user workbook."""
    namespace = "http://schemas.openxmlformats.org/spreadsheetml/2006/main"
    rels = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
    headers = headers or ["source_id", "source_text", "language"]
    xml_rows = []
    for index, row in enumerate([headers, *rows], 5):
        cells = []
        for col, value in zip("ABC", row):
            if value is None:
                continue
            value = escape(value).replace("\r", "&#13;")
            content = f'<is><t xml:space="preserve">{value}</t></is>'
            if formula and col == "B" and index == 6:
                content = '<f>"DO_NOT_PRINT_FORMULA"</f><v>0</v>'
            cells.append(f'<c r="{col}{index}" t="inlineStr">{content}</c>')
        xml_rows.append(f'<row r="{index}">{"".join(cells)}</row>')
    sink = io.BytesIO()
    with zipfile.ZipFile(sink, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("[Content_Types].xml", '''<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>
<Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>
</Types>''')
        z.writestr("xl/workbook.xml", f'<workbook xmlns="{namespace}" xmlns:r="{rels}"><sheets><sheet name="Nguon" sheetId="1" r:id="rId1"/></sheets></workbook>')
        z.writestr("xl/_rels/workbook.xml.rels", f'<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{rels}/worksheet" Target="worksheets/sheet1.xml"/></Relationships>')
        z.writestr("xl/worksheets/sheet1.xml", f'<worksheet xmlns="{namespace}"><dimension ref="A5:C{5+len(rows)}"/><sheetData>{"".join(xml_rows)}</sheetData></worksheet>')
    return sink.getvalue()


class SourceExportTests(unittest.TestCase):
    def setUp(self):
        self.f = fixtures.PreparationAuditTests()
        self.f.setUp()
        self.addCleanup(self.f.doCleanups)
        self.destination = self.f.private / "export"
        self.originals = {}
        for book in self.f.intake["workbooks"]:
            language = next(iter(book["language_counts"]))
            rows = []
            commitments = []
            for i in range(100):
                identity = f"{language}-{i}"
                value = f"  {language} {i}: e\u0301\r\n\t😀\n "
                rows.append([identity, value, language])
                key = export.sha256(identity.encode())
                self.originals[key] = value.encode("utf-8")
                commitments.append({"source_id_sha256": key, "source_text_sha256":
                                    export.sha256(unicodedata.normalize("NFC", value).encode())})
            path = Path(book["path"])
            path.write_bytes(workbook(rows))
            book.update(self.f.pin(path))
            book["source_commitments"] = commitments
        self.f.save()

    def run_export(self):
        # Actual owner-only directory creation is tested by the real private run;
        # tests mock OS ACL setup to avoid spawning whoami/icacls 200 times.
        with patch.object(export, "private_directory", side_effect=lambda p: p.mkdir(mode=0o700)):
            return export.export_sources(self.f.draft_path, [self.f.private], self.destination, self.f.root)

    def verify(self, completion):
        return export.verify_export(completion["manifest"], [self.f.private], self.f.root)

    def test_exact_decomposed_unicode_crlf_whitespace_and_no_added_bom(self):
        with patch("socket.socket", side_effect=AssertionError("network forbidden")), \
                patch("subprocess.Popen", side_effect=AssertionError("process forbidden")):
            result = self.run_export()
        self.assertEqual(result["source_count"], 200)
        self.assertEqual(self.verify(result)["languages"], {"vi": 100, "en": 100})
        for key, value in self.originals.items():
            self.assertEqual((self.destination / "sources" / f"{key}.utf8").read_bytes(), value)

    @unittest.skipUnless(os.name == "nt", "Windows DACL regression")
    def test_native_directory_acl_replaces_explicit_python_defaults(self):
        # The helper independently reads back the DACL and requires precisely
        # the current user and SYSTEM, both before any private bytes are written.
        path = self.f.private / "acl-regression"
        export.private_directory(path)
        self.assertTrue(path.is_dir())

    def test_manifest_has_exact_and_separate_nfc_bindings(self):
        result = self.run_export()
        manifest = json.loads(Path(result["manifest"]["path"]).read_bytes())
        self.assertTrue(all(r["utf8_sha256"] != r["nfc_comparison_sha256"] for r in manifest["sources"]))
        self.assertTrue(all(r["cell"].startswith("B") for r in manifest["sources"]))
        for raw in self.originals.values():
            self.assertNotIn(raw.decode(), json.dumps(manifest))

    def test_custody_cannot_issue_authority_or_lock(self):
        result = self.run_export()
        custody = json.loads(Path(result["custody_request"]["path"]).read_bytes())
        self.assertEqual(custody["issued_grants"], [])
        self.assertIsNone(custody["authorization_assessment"])
        self.assertEqual(custody["status"], "unresolved")
        self.assertFalse(result["execution_admitted"])

    def test_original_inputs_unchanged(self):
        before = {p: p.read_bytes() for p in self.f.private.iterdir() if p.is_file()}
        self.run_export()
        self.assertTrue(all(p.read_bytes() == value for p, value in before.items()))

    def test_formula_rejected_without_evaluation(self):
        with self.assertRaisesRegex(ValueError, "formula_source_cell"):
            list(export.source_cells(workbook([["id", "text", "en"]], formula=True)))

    def test_missing_id_not_silently_filtered(self):
        with self.assertRaisesRegex(ValueError, "nonempty_text"):
            list(export.source_cells(workbook([[None, "text", "en"]])))

    def test_duplicate_header_rejected(self):
        with self.assertRaisesRegex(ValueError, "source_headers_invalid"):
            list(export.source_cells(workbook([], headers=["source_id", "source_text", "source_text"])))

    def test_source_size_limit(self):
        with patch.object(export, "MAX_SOURCE_BYTES", 3):
            with self.assertRaisesRegex(ValueError, "source_byte_limit"):
                list(export.source_cells(workbook([["id", "😀", "en"]])))

    def test_archive_expansion_limit(self):
        with patch.object(export, "MAX_XML_BYTES", 3):
            with self.assertRaisesRegex(ValueError, "workbook_expansion_limit"):
                self.run_export()
        self.assertFalse(self.destination.exists())

    def test_stale_workbook_rejected_before_output(self):
        Path(self.f.intake["workbooks"][0]["path"]).write_bytes(b"changed")
        with self.assertRaises(ValueError):
            self.run_export()
        self.assertFalse(self.destination.exists())

    def test_intake_text_mismatch_rejected(self):
        self.f.intake["workbooks"][0]["source_commitments"][0]["source_text_sha256"] = "0" * 64
        self.f.save()
        with self.assertRaisesRegex(ValueError, "intake_source_binding_mismatch"):
            self.run_export()

    def test_existing_export_is_preserved(self):
        result = self.run_export()
        with self.assertRaisesRegex(ValueError, "output_exists"):
            self.run_export()
        self.assertEqual(self.verify(result)["verified_sources"], 200)

    def test_repository_output_rejected(self):
        self.destination = self.f.root / "private"
        with self.assertRaisesRegex(ValueError, "private_output_required"):
            self.run_export()

    def test_same_size_source_tamper_rejected(self):
        result = self.run_export()
        path = next((self.destination / "sources").iterdir())
        path.write_bytes(b"X" * path.stat().st_size)
        with self.assertRaisesRegex(ValueError, "artifact_digest_mismatch"):
            self.verify(result)

    def test_manifest_drift_rejected(self):
        result = self.run_export()
        Path(result["manifest"]["path"]).write_bytes(b"{}")
        with self.assertRaises(ValueError):
            self.verify(result)

    def test_rehashed_normalized_export_cannot_replace_original_cell(self):
        result = self.run_export()
        path = Path(result["manifest"]["path"])
        manifest = json.loads(path.read_bytes())
        row = manifest["sources"][0]
        source = self.destination / row["relative_path"]
        raw = unicodedata.normalize("NFC", source.read_bytes().decode()).encode()
        source.write_bytes(raw)
        row["utf8_sha256"], row["bytes"] = export.sha256(raw), len(raw)
        path.write_bytes(export.json_bytes(manifest))
        result["manifest"] = export.pin(path)
        with self.assertRaisesRegex(ValueError, "export_workbook_mismatch"):
            self.verify(result)

    def test_rehashed_wrong_cell_coordinate_rejected(self):
        result = self.run_export()
        path = Path(result["manifest"]["path"])
        manifest = json.loads(path.read_bytes())
        manifest["sources"][0]["cell"] = "B99"
        path.write_bytes(export.json_bytes(manifest))
        result["manifest"] = export.pin(path)
        with self.assertRaisesRegex(ValueError, "export_workbook_mismatch"):
            self.verify(result)

    def test_extra_source_file_rejected(self):
        result = self.run_export()
        (self.destination / "sources" / "labels.json").write_text("not a source")
        with self.assertRaisesRegex(ValueError, "extra_source_files"):
            self.verify(result)

    def test_incomplete_write_has_no_completion_marker(self):
        original = export.write_new
        def fail(path, raw):
            if path.name == "source-manifest.json":
                raise OSError("simulated disk failure")
            original(path, raw)
        with patch.object(export, "write_new", side_effect=fail):
            with self.assertRaises(OSError):
                self.run_export()
        self.assertTrue(self.destination.exists())
        self.assertFalse((self.destination / "completion.json").exists())

    def test_cli_redacts_library_exception(self):
        output = io.StringIO()
        args = ["export", "--draft", "ignored", "--input-root", "ignored", "--output-directory", "ignored"]
        with patch("sys.argv", args), patch.object(export, "export_sources", side_effect=ValueError("PRIVATE_SENTINEL")), contextlib.redirect_stdout(output):
            self.assertEqual(export.main(), 1)
        self.assertNotIn("PRIVATE_SENTINEL", output.getvalue())


if __name__ == "__main__":
    unittest.main()
