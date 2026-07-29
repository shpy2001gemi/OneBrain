import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

void registerBundledFontLicenses() {
  LicenseRegistry.addLicense(() async* {
    yield LicenseEntryWithLineBreaks(const <String>[
      'Nunito Sans',
    ], await rootBundle.loadString('assets/licenses/NunitoSans-OFL.txt'));
    yield LicenseEntryWithLineBreaks(const <String>[
      'Roboto Mono',
    ], await rootBundle.loadString('assets/licenses/RobotoMono-OFL.txt'));
    yield LicenseEntryWithLineBreaks(
      const <String>['Material Symbols Rounded'],
      await rootBundle.loadString(
        'assets/licenses/MaterialSymbolsRounded-Apache-2.0.txt',
      ),
    );
  });
}
