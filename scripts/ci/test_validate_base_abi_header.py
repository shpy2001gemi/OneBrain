from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from scripts.ci.validate_base_abi_header import (
    HEADER,
    RUST,
    ROOT,
    ValidationError,
    validate,
)


class BaseAbiHeaderValidatorTests(unittest.TestCase):
    def fixture(self) -> Path:
        directory = Path(tempfile.mkdtemp(prefix="base-abi-validator-"))
        for relative in (
            "src/test-vectors/vnext/base-v1-runtime-interface-v1.json",
            "src/onebrain-base-abi/src/lib.rs",
            "src/onebrain-base-abi/include/onebrain_base_v1.h",
            "scripts/toolchains/base-v1-tools.lock.json",
        ):
            target = directory / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes((ROOT / relative).read_bytes())
        return directory

    def test_checked_in_contract_is_valid_without_executing_tool(self) -> None:
        validate(ROOT, verify_tool=False)

    def test_missing_header_symbol_fails_closed(self) -> None:
        root = self.fixture()
        path = root / HEADER
        path.write_text(
            path.read_text(encoding="utf-8").replace("ob_base_query_v1", "removed_query"),
            encoding="utf-8",
        )
        with self.assertRaisesRegex(ValidationError, "header symbol drift"):
            validate(root, verify_tool=False)

    def test_underspecified_public_struct_fails_closed(self) -> None:
        root = self.fixture()
        path = root / HEADER
        path.write_text(
            path.read_text(encoding="utf-8").replace("  uint32_t struct_size;\n", "", 1),
            encoding="utf-8",
        )
        with self.assertRaisesRegex(ValidationError, "frozen size/ABI prefix"):
            validate(root, verify_tool=False)

    def test_extra_rust_symbol_fails_closed(self) -> None:
        root = self.fixture()
        path = root / RUST
        path.write_text(
            path.read_text(encoding="utf-8")
            + '\n#[no_mangle]\npub extern "C" fn ob_base_unreviewed_v1() {}\n',
            encoding="utf-8",
        )
        with self.assertRaisesRegex(ValidationError, "Rust ABI symbol drift"):
            validate(root, verify_tool=False)

    def test_machine_field_bound_drift_requires_a_new_descriptor_binding(self) -> None:
        root = self.fixture()
        path = root / "src/test-vectors/vnext/base-v1-runtime-interface-v1.json"
        path.write_text(
            path.read_text(encoding="utf-8").replace(
                '"max_archive_chunk_bytes": 1048576',
                '"max_archive_chunk_bytes": 1048575',
                1,
            ),
            encoding="utf-8",
        )
        with self.assertRaisesRegex(ValidationError, "complete machine descriptor"):
            validate(root, verify_tool=False)

    def test_public_c_field_width_drift_fails_closed(self) -> None:
        root = self.fixture()
        path = root / HEADER
        path.write_text(
            path.read_text(encoding="utf-8").replace(
                "  uint16_t discriminator;", "  uint32_t discriminator;", 1
            ),
            encoding="utf-8",
        )
        with self.assertRaisesRegex(ValidationError, "field width/order drift"):
            validate(root, verify_tool=False)


if __name__ == "__main__":
    unittest.main()
