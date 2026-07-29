import 'package:flutter/material.dart';

import '../../design/generated/mobile_design_tokens.g.dart';

enum ObmSymbol {
  arrowBack('arrow_back'),
  buildCircle('build_circle'),
  cancel('cancel'),
  checkCircle('check_circle'),
  cloudDownload('cloud_download'),
  cloudOff('cloud_off'),
  hub('hub'),
  info('info'),
  memory('memory'),
  phoneAndroid('phone_android'),
  phonelinkErase('phonelink_erase'),
  schedule('schedule'),
  shield('shield'),
  wifiOff('wifi_off');

  const ObmSymbol(this.glyph);

  final String glyph;
}

enum ObmIconSize {
  inline('inline'),
  control('control'),
  navigation('navigation'),
  hero('hero');

  const ObmIconSize(this.token);

  final String token;
}

class ObmIcon extends StatelessWidget {
  const ObmIcon(
    this.symbol, {
    this.size = ObmIconSize.control,
    this.color,
    this.semanticLabel,
    super.key,
  });

  final ObmSymbol symbol;
  final ObmIconSize size;
  final Color? color;
  final String? semanticLabel;

  @override
  Widget build(BuildContext context) {
    final logicalSize = ObmDesignTokens.iconSize[size.token]!;
    final glyph = Text(
      symbol.glyph,
      maxLines: 1,
      overflow: TextOverflow.clip,
      textScaler: TextScaler.noScaling,
      style: TextStyle(
        color: color ?? IconTheme.of(context).color,
        fontFamily: ObmDesignTokens.iconFamily,
        fontSize: logicalSize,
        height: 1,
        fontVariations: <FontVariation>[
          const FontVariation('FILL', 0),
          const FontVariation('GRAD', 0),
          FontVariation('opsz', logicalSize),
          const FontVariation('wght', 400),
        ],
      ),
    );
    if (semanticLabel == null) {
      return ExcludeSemantics(child: glyph);
    }
    return Semantics(
      image: true,
      label: semanticLabel,
      child: ExcludeSemantics(child: glyph),
    );
  }
}
