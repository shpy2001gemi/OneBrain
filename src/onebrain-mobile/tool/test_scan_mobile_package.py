from __future__ import annotations

import tempfile
import unittest
import zipfile
from pathlib import Path

from tool.scan_mobile_package import scan_package


class MobilePackageScannerTests(unittest.TestCase):
    def _package(self, bridge_abis: set[str]) -> Path:
        directory = Path(self._temporary_directory.name)
        package = directory / "fixture.apk"
        with zipfile.ZipFile(package, "w") as archive:
            for abi in {"armeabi-v7a", "arm64-v8a", "x86_64"}:
                archive.writestr(f"lib/{abi}/libflutter.so", b"flutter")
                archive.writestr(f"lib/{abi}/libapp.so", b"app")
            for abi in bridge_abis:
                archive.writestr(
                    f"lib/{abi}/libonebrain_mobile_bridge.so",
                    b"rust",
                )
        return package

    def setUp(self) -> None:
        self._temporary_directory = tempfile.TemporaryDirectory()
        self.addCleanup(self._temporary_directory.cleanup)

    def test_rejects_flutter_abi_without_rust_bridge(self) -> None:
        report = scan_package(self._package({"arm64-v8a", "x86_64"}))
        self.assertIn(
            "MISSING_RUST_BRIDGE_FOR_ANDROID_ABI:armeabi-v7a",
            report["violations"],
        )

    def test_accepts_exact_runtime_and_bridge_abi_set(self) -> None:
        report = scan_package(
            self._package({"armeabi-v7a", "arm64-v8a", "x86_64"})
        )
        self.assertEqual(report["violations"], [])


if __name__ == "__main__":
    unittest.main()
