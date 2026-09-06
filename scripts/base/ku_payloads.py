"""Generate the registered KU payload projections from the Base machine IDL."""
from __future__ import annotations

import json
import re
import subprocess


def variant(name: str) -> str:
    return ''.join(part[:1].upper() + part[1:] for part in name.split('_'))


def rust_type(name: str, types: dict) -> str:
    if name not in types:
        return name
    spec = types[name]
    return {
        'string': 'String', 'integer': 'u64', 'base64': 'String',
        'boolean': 'bool', 'literal': 'bool',
        'array': f"Vec<{spec.get('items')}>",
    }.get(spec['kind'], name)


def check(name: str, expr: str, types: dict, depth: int = 0) -> list[str]:
    if name not in types:
        return [f'{expr}.validate()?;']
    s = types[name]
    k = s['kind']
    if k == 'string':
        lines = [f'ensure({expr}.len() <= {s["max_bytes"]})?;']
        if name == 'Continuation':
            lines += [f'crate::ku_payload::validate_continuation({expr})?;']
        return lines
    if k == 'integer':
        lo, hi = s['min'], s['max']
        return ([f'ensure(*{expr} >= {lo})?;'] if lo else []) + ([f'ensure(*{expr} <= {hi})?;'] if hi < 2**64-1 else [])
    if k == 'literal':
        return [f'ensure(!*{expr})?;']
    if k == 'base64':
        return [f'validate_base64({expr}, {s["max_decoded_bytes"]})?;']
    if k == 'array':
        item = f'item_{depth}'
        inner = check(s['items'], item, types, depth+1)
        return [f'ensure({expr}.len() <= {s["max_items"]})?;'] + ([f'for {item} in {expr} {{'] + inner + ['}'] if inner else [])
    return []


def render_rust(section: dict) -> str:
    types, dtos = section['types'], section['dtos']
    lines = ['pub mod ku {', '#![allow(non_camel_case_types)]',
             'use serde::{Serialize, Deserialize};', 'use crate::ku_payload::{ensure, validate_base64, KuPayloadError, KuPayload};']
    lines += [f'pub const MINIMUM_BASE_MINOR: u16 = {section["minimum_base_minor"]};']
    for name, s in types.items():
        if s['kind'] == 'hex':
            n = s['bytes']
            lines += [
                '#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]',
                '#[serde(try_from = "String", into = "String")]',
                f'pub struct {name}(pub [u8; {n}]);',
                f'impl TryFrom<String> for {name} {{ type Error = KuPayloadError;',
                f'fn try_from(value: String) -> Result<Self, Self::Error> {{ Ok(Self(crate::ku_payload::decode_hex::<{n}>(&value)?)) }} }}',
                f'impl From<{name}> for String {{ fn from(value: {name}) -> Self {{ crate::ku_payload::hex(&value.0) }} }}',
                f'impl std::fmt::Debug for {name} {{ fn fmt(&self, f: &mut std::fmt::Formatter<\'_>) -> std::fmt::Result {{ f.write_str("{name}([private])") }} }}',
            ]
        elif s['kind'] == 'enum':
            lines += ['#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]', f'pub enum {name} {{']
            for v in s['values']:
                lines += [f'#[serde(rename = "{v}")] {variant(v)},']
            lines += ['}']
        else:
            lines += [f'pub type {name} = {rust_type(name, types)};']
    for name, dto in dtos.items():
        lines += ['#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]', '#[serde(deny_unknown_fields)]', f'pub struct {name} {{']
        for required in (True, False):
            for field, typ in dto['required' if required else 'optional'].items():
                if not required:
                    lines += ['#[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "crate::ku_payload::deserialize_present")]']
                lines += [f'pub {field}: {typ if required else f"Option<{typ}>"},']
        lines += ['}', f'impl KuPayload for {name} {{', f'const DTO_ID: u16 = {section["dto_ids"][name]};', 'fn validate(&self) -> Result<(), KuPayloadError> {']
        for field, typ in dto['required'].items():
            lines += check(typ, f'(&self.{field})', types)
        for field, typ in dto['optional'].items():
            inner = check(typ, 'value', types)
            if inner:
                lines += [f'if let Some(value) = &self.{field} {{'] + inner + ['}']
        rules = {
            'KuPrepareV1': ['ensure(self.semantic_profile == "ku-semantic-content/1.0" && !self.source_refs.is_empty())?;', 'ensure((self.input_mode == InputMode::ResolvedSemanticDraft) == self.draft_ref.is_some())?;'],
            'KuPreparedV1': ['ensure(self.semantic_profile == "ku-semantic-content/1.0")?;', 'if self.validity == Validity::Ready { ensure(!self.object_cids.is_empty() && self.object_cids.len() == self.artifacts.len())?;', 'let mut ids = std::collections::BTreeSet::new(); for (id, artifact) in self.object_cids.iter().zip(&self.artifacts) { ensure(*id == artifact.object_cid && ids.insert(*id))?; }', '} else { ensure(self.artifacts.is_empty() && self.object_cids.is_empty())?; }'],
            'KuSaveV1': ['ensure(!self.object_cids.is_empty())?;'],
            'KuExportV1': ['ensure(!self.object_cids.is_empty())?;'],
            'KuExportViewV1': ['let archive = self.mode == ExportMode::EncryptedBaseArchive;', 'ensure(self.requires_base_management == archive && self.archive_operation_id.is_some() == archive && self.public_records.is_some() != archive)?;'],
            'KuViewV1': ['ensure(self.fidelity_policy_cid.is_some() == self.fidelity_frontier.is_some())?;', 'ensure(self.artifact_validity != ArtifactValidity::AcceptedOpaque || self.semantic_content_cid.is_none())?;'],
            'KuSummaryV1': ['ensure(self.fidelity_policy_cid.is_some() == self.fidelity_frontier.is_some())?;', 'ensure(self.artifact_validity != ArtifactValidity::AcceptedOpaque || self.semantic_content_cid.is_none())?;'],
        }
        lines += rules.get(name, [])
        if name == 'KuFailureV1':
            lines += ['let (retryable, reconcile) = match self.code {']
            for error in section['errors']:
                lines += [f'BaseError::{variant(error["name"])} => ({str(error["retryable"]).lower()}, {str(error["reconcile_before_retry"]).lower()}),']
            lines += ['};', 'ensure(self.retryable == retryable && self.reconcile_before_retry == reconcile)?;']
        lines += ['Ok(())', '}', '}']
    for direction in ('request', 'response'):
        name = 'KuRequestV1' if direction == 'request' else 'KuResponseV1'
        lines += ['#[derive(Clone, Debug, PartialEq, Eq)]', f'pub enum {name} {{']
        for op in section['operations']:
            lines += [f'{variant(op["name"])}({op[direction]}),']
        lines += ['}', f'impl {name} {{', 'pub fn discriminator(&self) -> u16 { match self {']
        for op in section['operations']:
            lines += [f'Self::{variant(op["name"])}(_) => {op["wire_id"]},']
        lines += ['} }', 'pub fn validate(&self) -> Result<(), KuPayloadError> { match self {']
        for op in section['operations']:
            lines += [f'Self::{variant(op["name"])}(value) => value.validate(),']
        lines += ['} }', 'pub fn payload_bytes(&self) -> Result<Vec<u8>, KuPayloadError> { match self {']
        for op in section['operations']:
            lines += [f'Self::{variant(op["name"])}(value) => value.encode(),']
        lines += ['} }']
        if direction == 'request':
            ids = ' | '.join(str(op['wire_id']) for op in section['operations'])
            lines += [f'pub const fn is_registered_kind(kind: u16) -> bool {{ matches!(kind, {ids}) }}',
                      'pub fn decode_for_base_minor(kind: u16, bytes: &[u8], minor: u16) -> Result<Self, KuPayloadError> { ensure(minor >= MINIMUM_BASE_MINOR)?; Self::decode(kind, bytes) }']
            lines += ['pub fn decode(kind: u16, bytes: &[u8]) -> Result<Self, KuPayloadError> { match kind {']
            for op in section['operations']:
                lines += [f'{op["wire_id"]} => Ok(Self::{variant(op["name"])}({op[direction]}::decode(bytes)?)),']
            lines += ['_ => Err(KuPayloadError),', '} }']
        lines += ['}']
    lines += ['}']
    source = '\n'.join(lines) + '\n'
    # Parentheses are useful around length expressions, but not arguments/loops.
    source = re.sub(r'for (\w+) in \(&self\.(\w+)\)', r'for \1 in &self.\2', source)
    source = re.sub(r'validate_base64\(\(&self\.(\w+)\),', r'validate_base64(&self.\1,', source)
    return subprocess.run(['rustfmt', '--edition', '2021', '--emit', 'stdout'], input=source, text=True, capture_output=True, check=True).stdout


def render_typescript(section: dict) -> str:
    lines = ['// Registered KU local payloads; Base session and budget fences apply.']
    for name, s in section['types'].items():
        k = s['kind']
        typ = {'integer': 'bigint' if s.get('max',0) > 2**53-1 else 'number', 'boolean': 'boolean', 'literal': 'false', 'string': 'string', 'base64': 'string'}.get(k)
        if k == 'hex':
            typ = f'string & {{ readonly __role: "Ku{name}" }}'
        elif k == 'enum':
            typ = ' | '.join(json.dumps(v) for v in s['values'])
        elif k == 'array':
            typ = f'ReadonlyArray<KuPayload{s["items"]}>'
        lines += [f'export type KuPayload{name} = {typ};']
    for name, dto in section['dtos'].items():
        lines += [f'export interface KuPayload{name} {{']
        for required in (True, False):
            for field, typ in dto['required' if required else 'optional'].items():
                lines += [f'  readonly {field}{"" if required else "?"}: KuPayload{typ};']
        lines += ['}']
    lines += ['export const KU_OPERATION_IDS = {']
    lines += [f'  {o["name"]}: {o["wire_id"]},' for o in section['operations']]
    lines += ['} as const;']
    lines += ['export const KU_DTO_IDS = ' + json.dumps(section['dto_ids'], sort_keys=True) + ' as const;']
    lines += ['export const KU_PAYLOAD_SCHEMA = ' + json.dumps(section, separators=(',',':')) + ' as const;']
    return '\n'.join(lines) + '\n'


def render_dart(section: dict) -> str:
    lines = ['// Registered KU local payload declarations.']
    for name, s in section['types'].items():
        k = s['kind']
        typ = {'integer': 'int', 'boolean': 'bool', 'literal': 'bool', 'string': 'String', 'hex': 'String', 'base64': 'String'}.get(k)
        if k == 'array':
            typ = f'List<KuPayload{s["items"]}>'
        if k == 'enum':
            lines += [f'enum KuPayload{name} {{ ' + ', '.join('v' + variant(v) + '(' + json.dumps(v) + ')' for v in s['values']) + ';',
                      f'const KuPayload{name}(this.wireValue); final String wireValue;', '}']
        else:
            lines += [f'typedef KuPayload{name} = {typ};']
    for name, dto in section['dtos'].items():
        lines += [f'final class KuPayload{name} {{', f'const KuPayload{name}({{']
        for required in (True, False):
            lines += [f'{"required " if required else ""}this.{field},' for field in dto['required' if required else 'optional']]
        lines += ['});']
        for required in (True, False):
            for field, typ in dto['required' if required else 'optional'].items():
                lines += [f'final KuPayload{typ}{"" if required else "?"} {field};']
        lines += ['}']
    lines += ['const kuOperationIds = <String, int>{']
    lines += [f"'{o['name']}': {o['wire_id']}," for o in section['operations']]
    lines += ['};']
    lines += ['const kuDtoIds = <String, int>{' + ', '.join(json.dumps(k)+': '+str(v) for k,v in section['dto_ids'].items()) + '};']
    return '\n'.join(lines) + '\n'
