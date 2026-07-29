import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../ui/screens/component_gallery_screen.dart';
import '../ui/screens/assistant_screen.dart';
import '../ui/screens/capture_screen.dart';
import '../ui/screens/entry_screen.dart';
import '../ui/screens/home_screen.dart';
import '../ui/screens/init_screen.dart';
import '../ui/screens/library_screen.dart';
import '../ui/screens/onboarding_screen.dart';
import '../ui/screens/settings_screen.dart';
import '../ui/screens/share_spools_screen.dart';
import '../ui/screens/text_capture_screen.dart';
import '../ui/screens/welcome_screen.dart';

final appRouterProvider = Provider<GoRouter>((ref) {
  final router = GoRouter(
    initialLocation: '/entry',
    routes: [
      GoRoute(
        path: '/entry',
        name: 'entry',
        builder: (context, state) => const EntryScreen(),
      ),
      GoRoute(
        path: '/onboarding/welcome',
        name: 'onboarding-welcome',
        builder: (context, state) => const WelcomeScreen(),
      ),
      GoRoute(
        path: '/onboarding/preflight',
        name: 'onboarding-preflight',
        builder: (context, state) =>
            const OnboardingScreen(step: OnboardingStep.preflight),
      ),
      GoRoute(
        path: '/onboarding/identity',
        name: 'onboarding-identity',
        builder: (context, state) =>
            const OnboardingScreen(step: OnboardingStep.identity),
      ),
      GoRoute(
        path: '/onboarding/security',
        name: 'onboarding-security',
        builder: (context, state) =>
            const OnboardingScreen(step: OnboardingStep.security),
      ),
      GoRoute(
        path: '/onboarding/init-handoff',
        name: 'onboarding-init-handoff',
        builder: (context, state) =>
            const OnboardingScreen(step: OnboardingStep.initHandoff),
      ),
      GoRoute(
        path: '/init',
        name: 'required-data-init',
        builder: (context, state) => const InitScreen(),
      ),
      GoRoute(
        path: '/home',
        name: 'home',
        builder: (context, state) => const HomeScreen(),
      ),
      GoRoute(
        path: '/library',
        name: 'library',
        builder: (context, state) => const LibraryScreen(),
      ),
      GoRoute(
        path: '/capture',
        name: 'capture',
        builder: (context, state) => const CaptureScreen(),
      ),
      GoRoute(
        path: '/capture/text',
        name: 'capture-text',
        builder: (context, state) => const TextCaptureScreen(),
      ),
      GoRoute(
        path: '/capture/spools',
        name: 'capture-share-spools',
        builder: (context, state) => const ShareSpoolsScreen(),
      ),
      GoRoute(
        path: '/assistant',
        name: 'assistant',
        builder: (context, state) => const AssistantScreen(),
      ),
      GoRoute(
        path: '/settings',
        name: 'settings',
        builder: (context, state) => const SettingsScreen(),
      ),
      GoRoute(
        path: '/debug/components',
        name: 'component-gallery',
        builder: (context, state) => const ComponentGalleryScreen(),
      ),
    ],
  );
  ref.onDispose(router.dispose);
  return router;
});
