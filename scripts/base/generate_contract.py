#!/usr/bin/env python3
"""Generate the Base v1 Rust, TypeScript, and Dart contract projections."""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
import tempfile
from pathlib import Path
from typing import Mapping


ROOT = Path(__file__).resolve().parents[2]
IDL_PATH = ROOT / "src/test-vectors/vnext/base-v1-runtime-interface-v1.json"
HISTORY_PATH = (
    ROOT / "src/test-vectors/vnext/base-v1-runtime-interface-history-v1.json"
)
OUTPUT_PATHS = {
    "rust": ROOT / "src/onebrain-base-contract/src/generated.rs",
    "typescript": (
        ROOT / "src/onebrain-base-contract/generated/typescript/base_v1.ts"
    ),
    "dart": ROOT / "src/onebrain-base-contract/generated/dart/base_v1.dart",
}
GENERATED_HEADER = (
    "// Generated from src/test-vectors/vnext/"
    "base-v1-runtime-interface-v1.json; DO NOT EDIT."
)


class GenerationError(RuntimeError):
    """Raised when the IDL or a generated projection is invalid."""


def _rows_by_id(rows: object, context: str, *, allow_empty: bool = False) -> list[dict[str, object]]:
    if not isinstance(rows, list) or (not rows and not allow_empty):
        raise GenerationError(f"{context} must be a {'possibly empty' if allow_empty else 'non-empty'} list")
    result: list[dict[str, object]] = []
    ids: set[int] = set()
    names: set[str] = set()
    for row in rows:
        if not isinstance(row, dict):
            raise GenerationError(f"invalid discriminator row in {context}")
        identifier = row.get("id")
        name = row.get("name")
        if not isinstance(identifier, int) or identifier <= 0:
            raise GenerationError(f"invalid discriminator ID in {context}")
        if not isinstance(name, str) or not name:
            raise GenerationError(f"invalid discriminator name in {context}")
        if identifier in ids or name in names:
            raise GenerationError(f"duplicate discriminator ID/name in {context}")
        ids.add(identifier)
        names.add(name)
        result.append(row)
    return sorted(result, key=lambda item: (int(item["id"]), str(item["name"])))


def _positive_bound(row: Mapping[str, object], *names: str) -> int | None:
    for name in names:
        value = row.get(name)
        if isinstance(value, int) and value > 0:
            return value
    return None


def validate_idl(idl: dict[str, object]) -> None:
    if idl.get("format") != "onebrain/base-v1-runtime-interface/1":
        raise GenerationError("unexpected Base runtime IDL format")
    if idl.get("profile_id") != "BASE_V1_RUNTIME_INTERFACE_V1":
        raise GenerationError("unexpected Base runtime profile ID")
    version = idl.get("profile_version")
    if (
        not isinstance(version, dict)
        or version.get("major") != 1
        or not isinstance(version.get("minor"), int)
        or not 0 <= version["minor"] <= 65535
    ):
        raise GenerationError("invalid Base runtime profile version")

    scalars = idl.get("scalar_types")
    definitions = idl.get("type_definitions")
    if not isinstance(scalars, list):
        raise GenerationError("scalar_types must be a list")
    if not isinstance(definitions, dict) or not definitions:
        raise GenerationError("type_definitions must be a non-empty object")

    supported_scalar_wires = {
        "u8",
        "u16",
        "u32",
        "u64",
        "opaque_bytes",
        "blake3_digest",
        "ascii_token",
        "opaque_handle",
        "bounded_set",
        "bounded_bytes",
    }
    scalar_names: set[str] = set()
    for scalar in scalars:
        if not isinstance(scalar, dict):
            raise GenerationError("invalid scalar type")
        name = scalar.get("name")
        wire = scalar.get("wire")
        if not isinstance(name, str) or not name or name in scalar_names:
            raise GenerationError("duplicate or invalid scalar type name")
        if wire not in supported_scalar_wires:
            raise GenerationError(f"unsupported scalar wire type: {wire}")
        scalar_names.add(name)
        if wire in {"opaque_bytes", "opaque_handle", "blake3_digest"}:
            if _positive_bound(scalar, "exact_bytes", "max_bytes") is None:
                raise GenerationError(f"{name} lacks a finite byte bound")
        if wire in {"ascii_token", "bounded_bytes"} and _positive_bound(
            scalar, "max_bytes"
        ) is None:
            raise GenerationError(f"{name} lacks a finite byte bound")
        if wire == "bounded_set" and _positive_bound(scalar, "max_items") is None:
            raise GenerationError(f"{name} lacks a finite collection bound")

    definition_names = set(definitions)
    if scalar_names & definition_names:
        raise GenerationError("scalar and type-definition names must be disjoint")
    references = definition_names | scalar_names | {
        "u8",
        "u16",
        "u32",
        "u64",
        "bool",
        "SecretBytes",
    }
    allowed_ownership = {
        "value",
        "owned",
        "service_handle",
        "management_handle",
        "host_principal",
        "zeroizing_one_way_ingress",
    }

    root_sources = {
        "requests": idl.get("requests", []),
        "responses": idl.get("responses", []),
        "errors": idl.get("errors", []),
        "command_kinds": idl.get("command_kinds", []),
        "topic_kinds": idl.get("topic_kinds", []),
        "operations": idl.get("operations", []),
    }
    for source_name, rows in root_sources.items():
        _rows_by_id(rows, source_name, allow_empty=True)

    for name, definition in definitions.items():
        if not isinstance(name, str) or not name or not isinstance(definition, dict):
            raise GenerationError("invalid type definition")
        kind = definition.get("kind")
        if kind not in {"newtype", "opaque_registry_id", "struct", "enum"}:
            raise GenerationError(f"unsupported type kind: {kind}")
        if kind in {"newtype", "opaque_registry_id"}:
            if kind == "newtype" and definition.get("wire") not in {
                "fixed_bytes",
                "bounded_bytes",
            }:
                raise GenerationError(f"unsupported newtype wire for {name}")
            if _positive_bound(definition, "exact_bytes", "max_bytes") is None:
                raise GenerationError(f"{name} lacks a finite byte bound")
            if definition.get("ownership") not in allowed_ownership:
                raise GenerationError(f"invalid ownership for {name}")
        elif kind == "struct":
            fields = _rows_by_id(definition.get("fields"), f"{name}.fields")
            for field in fields:
                field_type = field.get("type")
                if field_type not in references:
                    raise GenerationError(f"unsupported type reference {field_type} in {name}")
                if field.get("required") not in {True, False}:
                    raise GenerationError(f"field optionality is absent in {name}")
                if field.get("ownership") not in allowed_ownership:
                    raise GenerationError(f"invalid field ownership in {name}")
                if field_type == "SecretBytes" and _positive_bound(
                    field, "max_bytes"
                ) is None:
                    raise GenerationError(f"secret field in {name} lacks a finite byte bound")
        else:
            if definition.get("closed") is not True:
                raise GenerationError(f"enum {name} must be closed")
            if definition.get("repr") not in {"u8", "u16", "u32"}:
                raise GenerationError(f"unsupported enum representation in {name}")
            variants_from = definition.get("variants_from")
            if variants_from is not None:
                if variants_from not in root_sources:
                    raise GenerationError(f"unknown variants_from source in {name}")
                variants = _rows_by_id(
                    root_sources[str(variants_from)],
                    f"{name}.variants_from",
                    allow_empty=True,
                )
            else:
                variants = _rows_by_id(definition.get("variants"), f"{name}.variants")
            for variant in variants:
                payload = variant.get("payload")
                if payload is not None and payload not in references:
                    raise GenerationError(f"unsupported payload type {payload} in {name}")


def _variants(idl: dict[str, object], definition: dict[str, object], name: str) -> list[dict[str, object]]:
    source = definition.get("variants_from")
    if source is not None:
        return _rows_by_id(idl[str(source)], f"{name}.variants_from", allow_empty=True)
    return _rows_by_id(definition["variants"], f"{name}.variants")


def _rust_field_type(field: Mapping[str, object]) -> str:
    field_type = str(field["type"])
    if field_type == "SecretBytes":
        field_type = f"SecretBytes<{int(field['max_bytes'])}>"
    if field.get("required") is False:
        return f"Option<{field_type}>"
    return field_type


def _rust_newtype(name: str, definition: Mapping[str, object]) -> list[str]:
    ownership = definition.get("ownership")
    derives = (
        "#[derive(Clone, Copy, PartialEq, Eq)]"
        if definition.get("exact_bytes") is not None
        and ownership in {"owned", "value"}
        else "#[derive(Clone, PartialEq, Eq)]"
        if ownership == "owned"
        else ""
    )
    lines = [derives] if derives else []
    if definition.get("kind") == "opaque_registry_id":
        rust_type = f"[u8; {int(definition['exact_bytes'])}]"
        visibility = "pub(crate)"
    elif definition.get("wire") == "fixed_bytes":
        rust_type = f"[u8; {int(definition['exact_bytes'])}]"
        visibility = "pub" if ownership in {"owned", "value"} else "pub(crate)"
    else:
        rust_type = f"BoundedBytes<{int(definition['max_bytes'])}>"
        visibility = "pub(crate)"
    lines.append(f"pub struct {name}({visibility} {rust_type});")
    return lines


def _rust_scalar(scalar: Mapping[str, object]) -> list[str]:
    name = str(scalar["name"])
    wire = scalar["wire"]
    ownership = scalar.get("ownership")
    if wire in {"u8", "u16", "u32", "u64"}:
        return ["#[derive(Clone, Copy, PartialEq, Eq)]", f"pub struct {name}(pub {wire});"]
    derives = (
        "#[derive(Clone, Copy, PartialEq, Eq)]"
        if scalar.get("exact_bytes") is not None and ownership in {"owned", "value"}
        else "#[derive(Clone, PartialEq, Eq)]"
        if ownership == "owned"
        else ""
    )
    if wire == "ascii_token":
        rust_type = f"BoundedAscii<{int(scalar['max_bytes'])}>"
        visibility = "pub(crate)"
    elif wire == "bounded_set":
        rust_type = f"BoundedVec<u16, {int(scalar['max_items'])}>"
        visibility = "pub(crate)"
    elif scalar.get("exact_bytes") is not None:
        rust_type = f"[u8; {int(scalar['exact_bytes'])}]"
        visibility = "pub" if ownership in {"owned", "value"} else "pub(crate)"
    else:
        rust_type = f"BoundedBytes<{int(scalar['max_bytes'])}>"
        visibility = "pub(crate)"
    lines = [derives] if derives else []
    lines.append(f"pub struct {name}({visibility} {rust_type});")
    return lines


def _rust_enum(idl: dict[str, object], name: str, definition: dict[str, object]) -> list[str]:
    variants = _variants(idl, definition, name)
    has_payload = any(row.get("payload") is not None for row in variants)
    lines: list[str] = []
    if not has_payload:
        lines.extend(
            [
                f"#[repr({definition['repr']})]",
                "#[derive(Clone, Copy, Debug, PartialEq, Eq)]",
                f"pub enum {name} {{",
            ]
        )
        for row in variants:
            lines.append(f"    {row['name']} = {row['id']},")
        lines.extend(
            [
                "}",
                "",
                f"impl {name} {{",
                f"    pub const fn discriminator(self) -> {definition['repr']} {{",
                f"        self as {definition['repr']}",
                "    }",
                "}",
            ]
        )
        return lines

    lines.append(f"pub enum {name} {{")
    for row in variants:
        payload = row.get("payload")
        suffix = f"({payload})" if payload is not None else ""
        lines.append(f"    {row['name']}{suffix},")
    lines.extend(["}", "", f"impl {name} {{", f"    pub const fn discriminator(&self) -> {definition['repr']} {{", "        match self {"])
    for row in variants:
        pattern = f"Self::{row['name']}(..)" if row.get("payload") is not None else f"Self::{row['name']}"
        lines.append(f"            {pattern} => {row['id']},")
    lines.extend(["        }", "    }", "}"])
    return lines


def _rust_struct(name: str, definition: Mapping[str, object]) -> list[str]:
    lines = [f"pub struct {name} {{"]
    for field in _rows_by_id(definition["fields"], f"{name}.fields"):
        visibility = "pub(crate)" if name == "BoundedSecretIngressV1" and field["name"] == "bytes" else "pub"
        lines.append(f"    {visibility} {field['name']}: {_rust_field_type(field)},")
    lines.append("}")
    return lines


def _rust_inventory(idl: dict[str, object]) -> list[str]:
    lines: list[str] = []
    inventories = (
        ("BASE_REQUEST_DISCRIMINATORS", "requests"),
        ("BASE_RESPONSE_DISCRIMINATORS", "responses"),
        ("BASE_ERROR_DISCRIMINATORS", "errors"),
        ("BASE_COMMAND_DISCRIMINATORS", "command_kinds"),
        ("BASE_TOPIC_DISCRIMINATORS", "topic_kinds"),
        ("BASE_OPERATION_DISCRIMINATORS", "operations"),
    )
    for constant, source in inventories:
        rows = _rows_by_id(idl.get(source, []), source, allow_empty=True)
        if not rows:
            continue
        lines.append(f"pub const {constant}: &[(&str, u16)] = &[")
        for row in rows:
            lines.append(f"    (\"{row['name']}\", {row['id']}),")
        lines.extend(["];"])
    return lines


def render_rust(idl: dict[str, object]) -> str:
    validate_idl(idl)
    version = idl["profile_version"]
    definitions = idl["type_definitions"]
    needs: set[str] = set()
    for scalar in idl["scalar_types"]:
        if scalar["wire"] in {"opaque_bytes", "bounded_bytes"} and scalar.get("max_bytes") is not None:
            needs.add("BoundedBytes")
        if scalar["wire"] == "ascii_token":
            needs.add("BoundedAscii")
        if scalar["wire"] == "bounded_set":
            needs.add("BoundedVec")
    for definition in definitions.values():
        if definition.get("kind") == "newtype" and definition.get("wire") == "bounded_bytes":
            needs.add("BoundedBytes")
        if definition.get("kind") == "struct":
            for field in definition["fields"]:
                if field.get("type") == "SecretBytes":
                    needs.add("SecretBytes")

    lines = [GENERATED_HEADER]
    if needs:
        lines.extend([f"use crate::operation::{{{', '.join(sorted(needs))}}};" if len(needs) > 1 else f"use crate::operation::{next(iter(needs))};", ""])
    lines.extend(
        [
            f"pub const BASE_RUNTIME_PROFILE_MAJOR: u16 = {version['major']};",
            f"pub const BASE_RUNTIME_PROFILE_MINOR: u16 = {version['minor']};",
            "",
        ]
    )
    for scalar in sorted(idl["scalar_types"], key=lambda item: item["name"]):
        lines.extend(_rust_scalar(scalar))
        lines.append("")
    for name in sorted(definitions):
        definition = definitions[name]
        if definition["kind"] in {"newtype", "opaque_registry_id"}:
            rendered = _rust_newtype(name, definition)
        elif definition["kind"] == "struct":
            rendered = _rust_struct(name, definition)
        else:
            rendered = _rust_enum(idl, name, definition)
        lines.extend(rendered)
        lines.append("")
    inventory = _rust_inventory(idl)
    if inventory:
        lines.extend(inventory)
        lines.append("")
    return "\n".join(lines)


def _ts_type(field: Mapping[str, object]) -> str:
    field_type = str(field["type"])
    primitive = {"u8": "number", "u16": "number", "u32": "number", "u64": "bigint", "bool": "boolean", "SecretBytes": "Uint8Array"}
    return primitive.get(field_type, field_type)


def _ts_bounded_class(name: str, maximum: int) -> list[str]:
    return [
        f"export class {name} {{",
        "  private constructor(private readonly value: Uint8Array) {}",
        "",
        f"  static tryFromBytes(bytes: Uint8Array): {name} {{",
        f"    if (bytes.length > {maximum}) throw new RangeError(\"{name} exceeds {maximum} bytes\");",
        f"    return new {name}(bytes.slice());",
        "  }",
        "",
        "  asBytes(): Uint8Array {",
        "    return this.value.slice();",
        "  }",
        "}",
    ]


def _ts_definition(idl: dict[str, object], name: str, definition: dict[str, object]) -> list[str]:
    kind = definition["kind"]
    if kind in {"newtype", "opaque_registry_id"}:
        if definition.get("wire") == "bounded_bytes":
            return _ts_bounded_class(name, int(definition["max_bytes"]))
        return [f"export type {name} = Uint8Array & {{ readonly __brand: \"{name}\" }};"]
    if kind == "struct":
        lines = [f"export interface {name} {{"]
        for field in _rows_by_id(definition["fields"], f"{name}.fields"):
            optional = "?" if field.get("required") is False else ""
            lines.append(f"  readonly {field['name']}{optional}: {_ts_type(field)};")
        lines.append("}")
        return lines
    variants = _variants(idl, definition, name)
    if not any(row.get("payload") is not None for row in variants):
        lines = [f"export enum {name} {{"]
        lines.extend(f"  {row['name']} = {row['id']}," for row in variants)
        lines.append("}")
        return lines
    lines = [f"export type {name} ="]
    for index, row in enumerate(variants):
        prefix = "  |"
        payload = f"; readonly payload: {row['payload']}" if row.get("payload") is not None else ""
        terminator = ";" if index == len(variants) - 1 else ""
        lines.append(
            f"{prefix} {{ readonly kind: {row['id']}; readonly name: \"{row['name']}\"{payload} }}{terminator}"
        )
    return lines


def _ts_scalar(scalar: Mapping[str, object]) -> list[str]:
    name = str(scalar["name"])
    wire = scalar["wire"]
    if wire in {"u8", "u16", "u32", "u64"}:
        base = "bigint" if wire == "u64" else "number"
        return [f"export type {name} = {base} & {{ readonly __brand: \"{name}\" }};"]
    if wire in {"bounded_bytes", "opaque_bytes"} and scalar.get("max_bytes") is not None:
        return _ts_bounded_class(name, int(scalar["max_bytes"]))
    if wire == "ascii_token":
        return [f"export type {name} = string & {{ readonly __maxBytes: {scalar['max_bytes']} }};"]
    if wire == "bounded_set":
        return [f"export type {name} = ReadonlyArray<number> & {{ readonly __maxItems: {scalar['max_items']} }};"]
    return [f"export type {name} = Uint8Array & {{ readonly __brand: \"{name}\" }};"]


def render_typescript(idl: dict[str, object]) -> str:
    validate_idl(idl)
    version = idl["profile_version"]
    lines = [
        GENERATED_HEADER,
        f"export const BASE_RUNTIME_PROFILE_MAJOR = {version['major']} as const;",
        f"export const BASE_RUNTIME_PROFILE_MINOR = {version['minor']} as const;",
        "",
    ]
    for scalar in sorted(idl["scalar_types"], key=lambda item: item["name"]):
        lines.extend(_ts_scalar(scalar))
        lines.append("")
    for name in sorted(idl["type_definitions"]):
        lines.extend(_ts_definition(idl, name, idl["type_definitions"][name]))
        lines.append("")
    return "\n".join(lines)


def _lower_camel(name: str) -> str:
    return name[:1].lower() + name[1:]


def _dart_type(field: Mapping[str, object]) -> str:
    field_type = str(field["type"])
    primitive = {"u8": "int", "u16": "int", "u32": "int", "u64": "int", "bool": "bool", "SecretBytes": "Uint8List"}
    result = primitive.get(field_type, field_type)
    return f"{result}?" if field.get("required") is False else result


def _dart_bounded_class(name: str, maximum: int) -> list[str]:
    return [
        f"final class {name} {{",
        f"  {name}._(this._value);",
        "",
        "  final Uint8List _value;",
        "",
        f"  factory {name}.tryFromBytes(Uint8List bytes) {{",
        f"    if (bytes.length > {maximum}) {{",
        f"      throw RangeError('{name} exceeds {maximum} bytes');",
        "    }",
        f"    return {name}._(Uint8List.fromList(bytes));",
        "  }",
        "",
        "  Uint8List asBytes() => Uint8List.fromList(_value);",
        "}",
    ]


def _dart_definition(idl: dict[str, object], name: str, definition: dict[str, object]) -> list[str]:
    kind = definition["kind"]
    if kind in {"newtype", "opaque_registry_id"}:
        if definition.get("wire") == "bounded_bytes":
            return _dart_bounded_class(name, int(definition["max_bytes"]))
        return [
            f"final class {name} {{",
            f"  {name}(Uint8List value) : value = Uint8List.fromList(value);",
            "  final Uint8List value;",
            "}",
        ]
    if kind == "struct":
        fields = _rows_by_id(definition["fields"], f"{name}.fields")
        lines = [f"final class {name} {{", f"  const {name}({{"]
        for field in fields:
            requirement = "required " if field.get("required") is True else ""
            lines.append(f"    {requirement}this.{_lower_camel(str(field['name']))},")
        lines.extend(["  });", ""])
        for field in fields:
            lines.append(f"  final {_dart_type(field)} {_lower_camel(str(field['name']))};")
        lines.append("}")
        return lines
    variants = _variants(idl, definition, name)
    if not any(row.get("payload") is not None for row in variants):
        lines = [f"enum {name} {{"]
        lines.extend(f"  {_lower_camel(str(row['name']))}({row['id']})," for row in variants[:-1])
        last = variants[-1]
        lines.append(f"  {_lower_camel(str(last['name']))}({last['id']});")
        lines.extend(["", f"  const {name}(this.discriminator);", "  final int discriminator;", "}"])
        return lines
    lines = [f"sealed class {name} {{", f"  const {name}(this.discriminator);", "  final int discriminator;", "}"]
    for row in variants:
        variant_name = f"{name}{row['name']}"
        payload = row.get("payload")
        lines.append("")
        lines.append(f"final class {variant_name} extends {name} {{")
        if payload is None:
            lines.append(f"  const {variant_name}() : super({row['id']});")
        else:
            lines.append(f"  const {variant_name}(this.payload) : super({row['id']});")
            lines.append(f"  final {payload} payload;")
        lines.append("}")
    return lines


def _dart_scalar(scalar: Mapping[str, object]) -> list[str]:
    name = str(scalar["name"])
    wire = scalar["wire"]
    if wire in {"u8", "u16", "u32", "u64"}:
        return [f"extension type const {name}(int value) {{}}"]
    if wire in {"bounded_bytes", "opaque_bytes"} and scalar.get("max_bytes") is not None:
        return _dart_bounded_class(name, int(scalar["max_bytes"]))
    if wire == "ascii_token":
        return [f"extension type const {name}(String value) {{}}"]
    if wire == "bounded_set":
        return [f"extension type const {name}(List<int> value) {{}}"]
    return [
        f"final class {name} {{",
        f"  {name}(Uint8List value) : value = Uint8List.fromList(value);",
        "  final Uint8List value;",
        "}",
    ]


def render_dart(idl: dict[str, object]) -> str:
    validate_idl(idl)
    version = idl["profile_version"]
    lines = [
        GENERATED_HEADER,
        "import 'dart:typed_data';",
        "",
        f"const int baseRuntimeProfileMajor = {version['major']};",
        f"const int baseRuntimeProfileMinor = {version['minor']};",
        "",
    ]
    for scalar in sorted(idl["scalar_types"], key=lambda item: item["name"]):
        lines.extend(_dart_scalar(scalar))
        lines.append("")
    for name in sorted(idl["type_definitions"]):
        lines.extend(_dart_definition(idl, name, idl["type_definitions"][name]))
        lines.append("")
    return "\n".join(lines)


def render_all(idl: dict[str, object]) -> dict[str, str]:
    validate_idl(idl)
    return {
        "rust": render_rust(idl),
        "typescript": render_typescript(idl),
        "dart": render_dart(idl),
    }


def write_outputs(
    outputs: Mapping[str, str],
    paths: Mapping[str, Path],
    *,
    check: bool,
) -> None:
    if set(outputs) != set(paths):
        raise GenerationError("generated output/path inventory mismatch")
    if check:
        drift: list[str] = []
        for name in sorted(outputs):
            path = paths[name]
            try:
                actual = path.read_text(encoding="utf-8")
            except OSError:
                drift.append(str(path.relative_to(ROOT) if path.is_relative_to(ROOT) else path))
                continue
            if actual != outputs[name]:
                drift.append(str(path.relative_to(ROOT) if path.is_relative_to(ROOT) else path))
        if drift:
            raise GenerationError(f"generated output drift: {', '.join(drift)}")
        return

    temporary: dict[str, Path] = {}
    try:
        for name in sorted(outputs):
            path = paths[name]
            path.parent.mkdir(parents=True, exist_ok=True)
            with tempfile.NamedTemporaryFile(
                mode="w",
                encoding="utf-8",
                newline="\n",
                prefix=f".{path.name}.",
                suffix=".tmp",
                dir=path.parent,
                delete=False,
            ) as handle:
                handle.write(outputs[name])
                handle.flush()
                os.fsync(handle.fileno())
                temporary[name] = Path(handle.name)
        for name in sorted(outputs):
            os.replace(temporary[name], paths[name])
            temporary.pop(name, None)
    finally:
        for path in temporary.values():
            try:
                path.unlink()
            except FileNotFoundError:
                pass


def _load_json(path: Path, description: str) -> dict[str, object]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise GenerationError(f"cannot load {description}: {error}") from error
    if not isinstance(value, dict):
        raise GenerationError(f"{description} must be a JSON object")
    return value


def verify_task14_baseline(receipt_path: Path, idl: dict[str, object]) -> None:
    if str(ROOT) not in sys.path:
        sys.path.insert(0, str(ROOT))
    from scripts.ci.validate_vnext_contracts import (  # pylint: disable=import-outside-toplevel
        ContractError,
        load_base_v1_runtime_baseline,
        validate_base_v1_runtime_interface,
    )

    if not receipt_path.is_file():
        raise GenerationError(f"Task 14 baseline receipt is missing: {receipt_path}")
    try:
        baseline_profile, baseline_history = load_base_v1_runtime_baseline(
            receipt_path
        )
        validate_base_v1_runtime_interface(
            idl,
            _load_json(HISTORY_PATH, "Base runtime discriminator history"),
            baseline_profile=baseline_profile,
            baseline_history=baseline_history,
        )
    except ContractError as error:
        raise GenerationError(f"Task 14 baseline verification failed: {error}") from error


def _default_receipt() -> Path:
    configured = os.environ.get("BASE_V1_IDL_BASELINE_RECEIPT")
    return Path(configured) if configured else ROOT / ".git/base-v1-idl-baseline-receipt.json"


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="fail on generated drift without writing")
    parser.add_argument(
        "--baseline-receipt",
        type=Path,
        default=_default_receipt(),
        help="immutable Task 14 baseline receipt",
    )
    arguments = parser.parse_args(argv)
    try:
        idl = _load_json(IDL_PATH, "Base runtime machine IDL")
        verify_task14_baseline(arguments.baseline_receipt.resolve(), idl)
        outputs = render_all(idl)
        write_outputs(outputs, OUTPUT_PATHS, check=arguments.check)
    except GenerationError as error:
        print(f"Base contract generation failed: {error}", file=sys.stderr)
        return 1
    action = "verified" if arguments.check else "generated"
    print(
        f"Base contract projections {action}: "
        + ", ".join(str(OUTPUT_PATHS[name].relative_to(ROOT)) for name in sorted(OUTPUT_PATHS))
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
