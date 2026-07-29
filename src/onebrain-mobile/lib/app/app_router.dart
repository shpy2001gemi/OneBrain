import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../ui/screens/component_gallery_screen.dart';
import '../ui/screens/entry_screen.dart';
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
        path: '/debug/components',
        name: 'component-gallery',
        builder: (context, state) => const ComponentGalleryScreen(),
      ),
    ],
  );
  ref.onDispose(router.dispose);
  return router;
});
