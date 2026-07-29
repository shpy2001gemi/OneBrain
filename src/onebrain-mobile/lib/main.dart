import 'package:flutter/widgets.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'app/onebrain_app.dart';
import 'design/font_license_registry.dart';

void main() {
  WidgetsFlutterBinding.ensureInitialized();
  registerBundledFontLicenses();
  runApp(const ProviderScope(child: OneBrainApp()));
}
