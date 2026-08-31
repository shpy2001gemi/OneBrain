from __future__ import annotations

import copy
import tempfile
import unittest
from pathlib import Path

from scripts.base.generate_contract import (
    GENERATED_HEADER,
    GenerationError,
    render_all,
    validate_idl,
    write_outputs,
)


def tiny_idl() -> dict[str, object]:
    return {
        "format": "onebrain/base-v1-runtime-interface/1",
        "profile_id": "BASE_V1_RUNTIME_INTERFACE_V1",
        "profile_version": {"major": 1, "minor": 0},
        "scalar_types": [],
        "type_definitions": {
            "TinyEnum": {
                "kind": "enum",
                "repr": "u16",
                "closed": True,
                "variants": [
                    {"id": 2, "name": "Second"},
                    {"id": 1, "name": "First"},
                ],
            },
            "BaseOpaqueContinuation": {
                "kind": "newtype",
                "wire": "bounded_bytes",
                "max_bytes": 4,
                "ownership": "owned",
                "constructor": "private_checked",
            },
        },
        "requests": [],
        "responses": [],
        "errors": [],
        "command_kinds": [],
        "topic_kinds": [],
        "operations": [],
    }


EXPECTED_RUST = f"""{GENERATED_HEADER}
use crate::operation::BoundedBytes;

pub const BASE_RUNTIME_PROFILE_MAJOR: u16 = 1;
pub const BASE_RUNTIME_PROFILE_MINOR: u16 = 0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaseOpaqueContinuation(pub(crate) BoundedBytes<4>);

#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TinyEnum {{
    First = 1,
    Second = 2,
}}

impl TinyEnum {{
    pub const fn discriminator(self) -> u16 {{
        self as u16
    }}
}}
"""


EXPECTED_TYPESCRIPT = f"""{GENERATED_HEADER}
export const BASE_RUNTIME_PROFILE_MAJOR = 1 as const;
export const BASE_RUNTIME_PROFILE_MINOR = 0 as const;

export class BaseOpaqueContinuation {{
  private constructor(private readonly value: Uint8Array) {{}}

  static tryFromBytes(bytes: Uint8Array): BaseOpaqueContinuation {{
    if (bytes.length > 4) throw new RangeError("BaseOpaqueContinuation exceeds 4 bytes");
    return new BaseOpaqueContinuation(bytes.slice());
  }}

  asBytes(): Uint8Array {{
    return this.value.slice();
  }}
}}

export enum TinyEnum {{
  First = 1,
  Second = 2,
}}
"""


EXPECTED_DART = f"""{GENERATED_HEADER}
import 'dart:typed_data';

const int baseRuntimeProfileMajor = 1;
const int baseRuntimeProfileMinor = 0;

final class BaseOpaqueContinuation {{
  BaseOpaqueContinuation._(this._value);

  final Uint8List _value;

  factory BaseOpaqueContinuation.tryFromBytes(Uint8List bytes) {{
    if (bytes.length > 4) {{
      throw RangeError('BaseOpaqueContinuation exceeds 4 bytes');
    }}
    return BaseOpaqueContinuation._(Uint8List.fromList(bytes));
  }}

  Uint8List asBytes() => Uint8List.fromList(_value);
}}

enum TinyEnum {{
  first(1),
  second(2);

  const TinyEnum(this.discriminator);
  final int discriminator;
}}
"""


class GenerateContractTests(unittest.TestCase):
    def test_tiny_fixture_is_byte_exact_for_all_three_targets(self) -> None:
        outputs = render_all(tiny_idl())
        self.assertEqual(outputs["rust"], EXPECTED_RUST)
        self.assertEqual(outputs["typescript"], EXPECTED_TYPESCRIPT)
        self.assertEqual(outputs["dart"], EXPECTED_DART)

    def test_generated_header_and_sorted_discriminators_are_stable(self) -> None:
        outputs = render_all(tiny_idl())
        for output in outputs.values():
            self.assertTrue(output.startswith(GENERATED_HEADER + "\n"))
        self.assertLess(outputs["rust"].index("First = 1"), outputs["rust"].index("Second = 2"))
        self.assertLess(outputs["typescript"].index("First = 1"), outputs["typescript"].index("Second = 2"))
        self.assertLess(outputs["dart"].index("first(1)"), outputs["dart"].index("second(2)"))

    def test_continuation_construction_is_private_and_bounded(self) -> None:
        outputs = render_all(tiny_idl())
        self.assertIn("pub(crate) BoundedBytes<4>", outputs["rust"])
        self.assertIn("private constructor", outputs["typescript"])
        self.assertIn("BaseOpaqueContinuation._", outputs["dart"])

    def test_duplicate_discriminator_id_is_rejected(self) -> None:
        fixture = tiny_idl()
        fixture["type_definitions"]["TinyEnum"]["variants"][1]["id"] = 2
        with self.assertRaisesRegex(GenerationError, "duplicate discriminator"):
            validate_idl(fixture)

    def test_unsupported_type_kind_is_rejected(self) -> None:
        fixture = tiny_idl()
        fixture["type_definitions"]["Mystery"] = {"kind": "pointer"}
        with self.assertRaisesRegex(GenerationError, "unsupported type kind"):
            validate_idl(fixture)

    def test_unbounded_collection_is_rejected(self) -> None:
        fixture = tiny_idl()
        fixture["type_definitions"]["Unbounded"] = {
            "kind": "newtype",
            "wire": "bounded_bytes",
            "ownership": "owned",
        }
        with self.assertRaisesRegex(GenerationError, "finite byte bound"):
            validate_idl(fixture)

    def test_check_mode_does_not_modify_drifted_files(self) -> None:
        outputs = render_all(tiny_idl())
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            paths = {name: root / f"{name}.txt" for name in outputs}
            write_outputs(outputs, paths, check=False)
            paths["rust"].write_text("drift\n", encoding="utf-8")
            before = paths["rust"].read_bytes()
            with self.assertRaisesRegex(GenerationError, "generated output drift"):
                write_outputs(outputs, paths, check=True)
            self.assertEqual(paths["rust"].read_bytes(), before)

    def test_atomic_generation_replaces_every_complete_output(self) -> None:
        outputs = render_all(tiny_idl())
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            paths = {name: root / f"{name}.txt" for name in outputs}
            write_outputs(outputs, paths, check=False)
            for name, path in paths.items():
                self.assertEqual(path.read_text(encoding="utf-8"), outputs[name])
            self.assertEqual(list(root.glob("*.tmp")), [])


if __name__ == "__main__":
    unittest.main()
