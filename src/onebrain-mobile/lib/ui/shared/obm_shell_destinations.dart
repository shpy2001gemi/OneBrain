import '../../l10n/app_localizations.dart';
import 'obm_app_shell.dart';
import 'obm_icon.dart';

List<ObmShellDestination> obmShellDestinations(AppLocalizations strings) => [
  ObmShellDestination(
    label: strings.navHome,
    icon: ObmSymbol.home,
    location: '/home',
  ),
  ObmShellDestination(
    label: strings.navLibrary,
    icon: ObmSymbol.library,
    location: '/library',
  ),
  ObmShellDestination(
    label: strings.navCapture,
    icon: ObmSymbol.capture,
    location: '/capture',
  ),
  ObmShellDestination(
    label: strings.navAssistant,
    icon: ObmSymbol.assistant,
    location: '/assistant',
  ),
  ObmShellDestination(
    label: strings.navSettings,
    icon: ObmSymbol.settings,
    location: '/settings',
  ),
];
