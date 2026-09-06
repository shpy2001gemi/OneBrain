"""Offline validator for the explicitly closed JSON Schema subset in KU-ENC-001.

This is a contract/corpus tool, not a production extraction or acceptance port.
Unknown keywords and remote references fail closed. No network resolution.
"""
import json
import re


class ContractError(ValueError):
    pass


def require(condition, reason):
    if not condition:
        raise ContractError(reason)


KEYWORDS = {'$schema', 'title', '$defs', '$ref', 'type', 'const', 'enum', 'oneOf',
            'properties', 'required', 'additionalProperties', 'items', 'minItems',
            'maxItems', 'minLength', 'maxLength', 'pattern', 'minimum', 'maximum'}


def check_schema(node, definitions):
    require(isinstance(node, dict) and not (set(node) - KEYWORDS), 'unsupported_schema_keyword')
    if '$ref' in node:
        require(set(node) == {'$ref'} or '$defs' in node, 'ref_siblings')
        require(node['$ref'].startswith('#/$defs/') and node['$ref'][8:] in definitions, 'schema_reference')
    if 'type' in node:
        require(node['type'] in ('object', 'array', 'string', 'integer', 'boolean'), 'schema_type')
        if node['type'] == 'object':
            require(node.get('additionalProperties') is False, 'open_object_schema')
            require(set(node.get('required', [])) <= set(node.get('properties', {})), 'required_field')
        if node['type'] == 'array':
            require('maxItems' in node and 'items' in node, 'unbounded_array')
        if node['type'] == 'string' and 'enum' not in node:
            require('maxLength' in node, 'unbounded_string')
        if node['type'] == 'integer':
            require('minimum' in node and 'maximum' in node, 'unbounded_integer')
    for value in node.get('properties', {}).values():
        check_schema(value, definitions)
    if 'items' in node:
        check_schema(node['items'], definitions)
    for value in node.get('oneOf', []):
        check_schema(value, definitions)


def validate(value, schema, definitions, depth=0):
    require(depth <= 32, 'schema_depth')
    if '$ref' in schema:
        return validate(value, definitions[schema['$ref'][8:]], definitions, depth + 1)
    if 'oneOf' in schema:
        passed = 0
        for branch in schema['oneOf']:
            try:
                validate(value, branch, definitions, depth + 1)
                passed += 1
            except ContractError:
                pass
        require(passed == 1, 'oneof')
        return
    if 'const' in schema:
        require(type(value) is type(schema['const']) and value == schema['const'], 'const')
    if 'enum' in schema:
        require(value in schema['enum'], 'enum')
    kind = schema.get('type')
    if kind == 'object':
        require(type(value) is dict, 'object_type')
        require(set(schema['required']) <= set(value), 'missing_field')
        require(set(value) <= set(schema['properties']), 'unknown_field')
        for key, item in value.items():
            validate(item, schema['properties'][key], definitions, depth + 1)
    elif kind == 'array':
        require(type(value) is list, 'array_type')
        require(schema.get('minItems', 0) <= len(value) <= schema['maxItems'], 'array_bound')
        for item in value:
            validate(item, schema['items'], definitions, depth + 1)
    elif kind == 'string':
        require(type(value) is str, 'string_type')
        try:
            value.encode('utf-8')
        except UnicodeError as error:
            raise ContractError('invalid_unicode') from error
        require(schema.get('minLength', 0) <= len(value) <= schema.get('maxLength', 2**31), 'string_bound')
        if 'pattern' in schema:
            require(re.fullmatch(schema['pattern'], value) is not None, 'string_pattern')
    elif kind == 'integer':
        require(type(value) is int, 'integer_type')
        require(schema['minimum'] <= value <= schema['maximum'], 'integer_bound')
    elif kind == 'boolean':
        require(type(value) is bool, 'boolean_type')


def strict_loads(raw, limit=1_048_576):
    if isinstance(raw, str):
        try:
            raw = raw.encode('utf-8')
        except UnicodeError as error:
            raise ContractError('invalid_json') from error
    require(len(raw) <= limit, 'payload_bytes')
    # Bound nesting before constructing Python containers. Brackets in strings
    # are ignored, including escaped quote/backslash sequences.
    depth = 0
    quoted = escaped = False
    try:
        decoded = raw.decode('utf-8')
    except UnicodeError as error:
        raise ContractError('invalid_json') from error
    for char in decoded:
        if quoted:
            if escaped:
                escaped = False
            elif char == '\\':
                escaped = True
            elif char == '"':
                quoted = False
        elif char == '"':
            quoted = True
        elif char in '[{':
            depth += 1
            require(depth <= 32, 'json_depth')
        elif char in ']}':
            depth -= 1
    def pairs(items):
        result = {}
        for key, value in items:
            require(key not in result, 'duplicate_key')
            result[key] = value
        return result
    def invalid_number(_):
        raise ContractError('non_integer_json_number')
    try:
        return json.loads(raw, object_pairs_hook=pairs, parse_float=invalid_number, parse_constant=invalid_number)
    except (UnicodeError, json.JSONDecodeError, RecursionError) as error:
        raise ContractError('invalid_json') from error


def stable_bytes(value):
    """Finite sorted JSON artifact binding; explicitly not canonical KU encoding."""
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(',', ':'), allow_nan=False).encode('utf-8')
