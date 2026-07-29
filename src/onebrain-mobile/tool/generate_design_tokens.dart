import 'dart:convert';
import 'dart:io';

const _defaultInput =
    '../../docs/design/mobile/tokens/mobile_design_tokens_v1.json';
const _defaultOutput = 'lib/design/generated/mobile_design_tokens.g.dart';

void main(List<String> arguments) {
  final inputPath = arguments.isEmpty ? _defaultInput : arguments.first;
  final outputPath = arguments.length < 2 ? _defaultOutput : arguments[1];
  final source = File(inputPath);
  final root = jsonDecode(source.readAsStringSync()) as Map<String, Object?>;
  final output = File(outputPath);
  output.parent.createSync(recursive: true);
  output.writeAsStringSync(_render(root));
}

String _render(Map<String, Object?> root) {
  final color = _map(root, 'color');
  final semantic = _map(color, 'semantic');
  final status = _map(color, 'status');
  final gradient = _map(color, 'gradient');
  final typography = _map(root, 'typography');
  final style = _map(typography, 'style');
  final buffer = StringBuffer()
    ..writeln('// GENERATED CODE - DO NOT MODIFY BY HAND.')
    ..writeln('// Source: mobile_design_tokens_v1.json')
    ..writeln('// Run: dart run tool/generate_design_tokens.dart')
    ..writeln()
    ..writeln("import 'package:flutter/material.dart';")
    ..writeln()
    ..writeln('abstract final class ObmDesignTokens {')
    ..writeln("  static const sourceFormat = '${root['format']}';")
    ..writeln("  static const sourceVersion = '${root['version']}';");

  for (final appearance in <String>[
    'light',
    'dark',
    'highContrastLight',
    'highContrastDark',
  ]) {
    _writeColorMap(buffer, '${appearance}Colors', _map(semantic, appearance));
  }
  _writeNumberMap(buffer, 'spacing', _map(root, 'spacing'));
  _writeNumberMap(buffer, 'radius', _map(root, 'radius'));
  _writeNumberMap(buffer, 'stroke', _map(root, 'stroke'));
  _writeNumberMap(
    buffer,
    'motionDuration',
    _map(_map(root, 'motion'), 'duration'),
  );
  _writeNumberMap(
    buffer,
    'layoutBreakpoints',
    _map(_map(root, 'layout'), 'breakpoint'),
  );
  _writeNumberMap(
    buffer,
    'layoutMaxWidth',
    _map(_map(root, 'layout'), 'maxWidth'),
  );
  _writeNestedNumberMap(buffer, 'componentMetrics', _map(root, 'component'));
  _writeTextStyleMap(buffer, style);
  _writeGradientMap(buffer, gradient);
  for (final appearance in <String>['light', 'dark']) {
    final states = _map(status, appearance);
    _writeStatusColorMap(
      buffer,
      '${appearance}StatusContainer',
      states,
      'container',
    );
    _writeStatusColorMap(
      buffer,
      '${appearance}StatusContent',
      states,
      'content',
    );
  }
  buffer.writeln('}');
  return buffer.toString();
}

Map<String, Object?> _map(Map<String, Object?> parent, String key) =>
    (parent[key] as Map).cast<String, Object?>();

void _writeColorMap(
  StringBuffer buffer,
  String name,
  Map<String, Object?> values,
) {
  buffer
    ..writeln()
    ..writeln('  static const Map<String, Color> $name = <String, Color>{');
  for (final entry in values.entries) {
    buffer.writeln("    '${entry.key}': ${_color(entry.value as String)},");
  }
  buffer.writeln('  };');
}

void _writeNumberMap(
  StringBuffer buffer,
  String name,
  Map<String, Object?> values,
) {
  buffer
    ..writeln()
    ..writeln('  static const Map<String, double> $name = <String, double>{');
  for (final entry in values.entries) {
    buffer.writeln("    '${entry.key}': ${(entry.value as num).toDouble()},");
  }
  buffer.writeln('  };');
}

void _writeNestedNumberMap(
  StringBuffer buffer,
  String name,
  Map<String, Object?> groups,
) {
  buffer
    ..writeln()
    ..writeln(
      '  static const Map<String, Map<String, double>> $name = '
      '<String, Map<String, double>>{',
    );
  for (final entry in groups.entries) {
    final values = (entry.value as Map).cast<String, Object?>();
    buffer.writeln("    '${entry.key}': <String, double>{");
    for (final value in values.entries) {
      buffer.writeln(
        "      '${value.key}': ${(value.value as num).toDouble()},",
      );
    }
    buffer.writeln('    },');
  }
  buffer.writeln('  };');
}

void _writeTextStyleMap(StringBuffer buffer, Map<String, Object?> styles) {
  buffer
    ..writeln()
    ..writeln(
      '  static const Map<String, Map<String, double>> typography = '
      '<String, Map<String, double>>{',
    );
  for (final entry in styles.entries) {
    final value = (entry.value as Map).cast<String, Object?>();
    buffer
      ..writeln("    '${entry.key}': <String, double>{")
      ..writeln("      'size': ${(value['size'] as num).toDouble()},")
      ..writeln(
        "      'lineHeight': ${(value['lineHeight'] as num).toDouble()},",
      )
      ..writeln("      'weight': ${(value['weight'] as num).toDouble()},")
      ..writeln(
        "      'letterSpacing': ${(value['letterSpacing'] as num).toDouble()},",
      )
      ..writeln('    },');
  }
  buffer.writeln('  };');
}

void _writeGradientMap(StringBuffer buffer, Map<String, Object?> gradients) {
  buffer
    ..writeln()
    ..writeln('  static const Map<String, List<Color>> gradients = ')
    ..writeln('      <String, List<Color>>{');
  for (final entry in gradients.entries) {
    final value = (entry.value as Map).cast<String, Object?>();
    buffer.writeln(
      "    '${entry.key}': <Color>["
      "${_color(value['start'] as String)}, "
      "${_color(value['end'] as String)}],",
    );
  }
  buffer.writeln('  };');
}

void _writeStatusColorMap(
  StringBuffer buffer,
  String name,
  Map<String, Object?> states,
  String role,
) {
  buffer
    ..writeln()
    ..writeln('  static const Map<String, Color> $name = <String, Color>{');
  for (final entry in states.entries) {
    final value = (entry.value as Map).cast<String, Object?>();
    buffer.writeln("    '${entry.key}': ${_color(value[role] as String)},");
  }
  buffer.writeln('  };');
}

String _color(String value) {
  final hex = value.substring(1).toUpperCase();
  final argb = hex.length == 6
      ? 'FF$hex'
      : '${hex.substring(6, 8)}${hex.substring(0, 6)}';
  return 'Color(0x$argb)';
}
