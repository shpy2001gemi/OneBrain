import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../ui/screens/ai_model_screens.dart';
import '../ui/screens/assistant_screen.dart';
import '../ui/screens/capture_screen.dart';
import '../ui/screens/component_gallery_screen.dart';
import '../ui/screens/data_backup_screens.dart';
import '../ui/screens/entry_screen.dart';
import '../ui/screens/home_screen.dart';
import '../ui/screens/init_screen.dart';
import '../ui/screens/knowledge_library_screens.dart';
import '../ui/screens/library_screen.dart';
import '../ui/screens/media_activity_system_screens.dart';
import '../ui/screens/media_import_screen.dart';
import '../ui/screens/my_media_screen.dart';
import '../ui/screens/onboarding_screen.dart';
import '../ui/screens/registry_ops_screens.dart';
import '../ui/screens/settings_screen.dart';
import '../ui/screens/share_spools_screen.dart';
import '../ui/screens/text_capture_screen.dart';
import '../ui/screens/welcome_screen.dart';

/// Active consumer router.
///
/// Blocked/gated routes (`MOB-SCR-PUB-001..003`, `MOB-SCR-FID-002..010`,
/// `MOB-SCR-NET-001..008`, `MOB-SCR-MAT-001..005`, Wallet/M7, and Active M6 KQL)
/// are intentionally ABSENT from consumer navigation.
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
        path: '/onboarding/readiness',
        name: 'onboarding-readiness',
        builder: (context, state) => const ReadinessOnboardingScreen(),
      ),
      GoRoute(
        path: '/init',
        name: 'required-data-init',
        builder: (context, state) => const InitScreen(),
      ),
      GoRoute(
        path: '/init/registry/plan',
        name: 'registry-source-plan',
        builder: (context, state) => const RegistrySourcePlanScreen(),
      ),
      GoRoute(
        path: '/operations',
        name: 'operations-center',
        builder: (context, state) => const OperationsCenterScreen(),
      ),
      GoRoute(
        path: '/operations/:operationRef',
        name: 'operation-detail',
        builder: (context, state) => OperationDetailScreen(
          operationRef: state.pathParameters['operationRef']!,
        ),
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
        path: '/library/my-ku',
        name: 'library-my-ku',
        builder: (context, state) => const MyKuShelfScreen(),
      ),
      GoRoute(
        path: '/library/search',
        name: 'library-search',
        builder: (context, state) => const SemanticSearchScreen(),
      ),
      GoRoute(
        path: '/library/kql',
        name: 'library-local-kql',
        builder: (context, state) => const LocalKqlScreen(),
      ),
      GoRoute(
        path: '/library/graph/:localRef',
        name: 'library-neighborhood',
        builder: (context, state) => LocalNeighborhoodScreen(
          localRef: state.pathParameters['localRef']!,
        ),
      ),
      GoRoute(
        path: '/library/concepts',
        name: 'library-concepts',
        builder: (context, state) => const ConceptsBrowserScreen(),
      ),
      GoRoute(
        path: '/library/concepts/:ccid',
        name: 'library-concept-detail',
        builder: (context, state) =>
            ConceptDetailScreen(ccid: state.pathParameters['ccid']!),
      ),
      GoRoute(
        path: '/library/knowledge/:localRef',
        name: 'private-ku-detail',
        builder: (context, state) => PrivateKuDetailScreen(
          localRef: state.pathParameters['localRef']!,
        ),
      ),
      GoRoute(
        path: '/library/knowledge/:localRef/edit',
        name: 'private-ku-edit',
        builder: (context, state) => PrivateKuEditRevisionScreen(
          localRef: state.pathParameters['localRef']!,
        ),
      ),
      GoRoute(
        path: '/library/knowledge/:localRef/organize',
        name: 'private-ku-organize',
        builder: (context, state) => PrivateKuOrganizeScreen(
          localRef: state.pathParameters['localRef']!,
        ),
      ),
      GoRoute(
        path: '/library/knowledge/:localRef/workflow',
        name: 'private-ku-workflow',
        builder: (context, state) => PrivateKuWorkflowScreen(
          localRef: state.pathParameters['localRef']!,
        ),
      ),
      GoRoute(
        path: '/library/knowledge/:localRef/conflicts',
        name: 'private-ku-lineage',
        builder: (context, state) => PrivateKuLineageScreen(
          localRef: state.pathParameters['localRef']!,
        ),
      ),
      GoRoute(
        path: '/library/knowledge/:localRef/encoding',
        name: 'private-ku-alternates',
        builder: (context, state) => PrivateKuAlternatesScreen(
          localRef: state.pathParameters['localRef']!,
        ),
      ),
      GoRoute(
        path: '/library/knowledge/:localRef/fidelity',
        name: 'private-ku-fidelity',
        builder: (context, state) => PrivateKuFidelityScreen(
          localRef: state.pathParameters['localRef']!,
        ),
      ),
      GoRoute(
        path: '/library/my-media',
        name: 'library-my-media',
        builder: (context, state) => const MyMediaScreen(),
      ),
      GoRoute(
        path: '/library/media/:manifestRef',
        name: 'media-verified-viewer',
        builder: (context, state) => MediaVerifiedViewerScreen(
          manifestRef: state.pathParameters['manifestRef']!,
        ),
      ),
      GoRoute(
        path: '/library/media/:manifestRef/info',
        name: 'media-manifest-info',
        builder: (context, state) => MediaManifestInfoScreen(
          manifestRef: state.pathParameters['manifestRef']!,
        ),
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
        path: '/capture/import',
        name: 'capture-media-import',
        builder: (context, state) => const MediaImportScreen(),
      ),
      GoRoute(
        path: '/capture/operations/:operationRef/review',
        name: 'capture-review',
        builder: (context, state) => CaptureReviewScreen(
          operationRef: state.pathParameters['operationRef']!,
        ),
      ),
      GoRoute(
        path: '/capture/operations/:operationRef/result',
        name: 'capture-result',
        builder: (context, state) => CaptureResultScreen(
          operationRef: state.pathParameters['operationRef']!,
        ),
      ),
      GoRoute(
        path: '/capture/operations/:operationRef/encode',
        name: 'capture-ku-encode',
        builder: (context, state) => PrivateKuEncodeScreen(
          operationRef: state.pathParameters['operationRef']!,
        ),
      ),
      GoRoute(
        path: '/capture/operations/:operationRef/encoding-review',
        name: 'capture-ku-encoding-review',
        builder: (context, state) => PrivateKuEncodingReviewScreen(
          operationRef: state.pathParameters['operationRef']!,
        ),
      ),
      GoRoute(
        path: '/assistant',
        name: 'assistant',
        builder: (context, state) => const AssistantScreen(),
      ),
      GoRoute(
        path: '/assistant/thread/:threadRef',
        name: 'assistant-thread',
        builder: (context, state) => AssistantThreadScreen(
          threadRef: state.pathParameters['threadRef']!,
        ),
      ),
      GoRoute(
        path: '/assistant/context',
        name: 'assistant-context',
        builder: (context, state) => const AssistantContextScreen(),
      ),
      GoRoute(
        path: '/assistant/disclosure/:requestRef',
        name: 'assistant-disclosure',
        builder: (context, state) => AssistantDisclosureGateScreen(
          requestRef: state.pathParameters['requestRef']!,
        ),
      ),
      GoRoute(
        path: '/assistant/proposal/:proposalRef',
        name: 'assistant-proposal',
        builder: (context, state) => ToolProposalConsentScreen(
          proposalRef: state.pathParameters['proposalRef']!,
        ),
      ),
      GoRoute(
        path: '/assistant/tool/:operationRef',
        name: 'assistant-tool-receipt',
        builder: (context, state) => ToolExecutionReceiptScreen(
          operationRef: state.pathParameters['operationRef']!,
        ),
      ),
      GoRoute(
        path: '/assistant/provider',
        name: 'assistant-provider',
        builder: (context, state) => const AssistantProviderScreen(),
      ),
      GoRoute(
        path: '/activity',
        name: 'activity-inbox',
        builder: (context, state) => const ActivityInboxScreen(),
      ),
      GoRoute(
        path: '/activity/:intentRef',
        name: 'activity-detail',
        builder: (context, state) =>
            ActivityDetailScreen(intentRef: state.pathParameters['intentRef']!),
      ),
      GoRoute(
        path: '/settings',
        name: 'settings',
        builder: (context, state) => const SettingsScreen(),
      ),
      GoRoute(
        path: '/settings/registry',
        name: 'settings-registry',
        builder: (context, state) => const RegistrySettingsScreen(),
      ),
      GoRoute(
        path: '/settings/registry/update',
        name: 'settings-registry-update',
        builder: (context, state) => const RegistryUpdateScreen(),
      ),
      GoRoute(
        path: '/settings/storage',
        name: 'settings-storage',
        builder: (context, state) => const StorageBreakdownScreen(),
      ),
      GoRoute(
        path: '/settings/storage/cleanup',
        name: 'settings-storage-cleanup',
        builder: (context, state) => const StorageCleanupScreen(),
      ),
      GoRoute(
        path: '/settings/backup',
        name: 'settings-backup',
        builder: (context, state) => const BackupSettingsScreen(),
      ),
      GoRoute(
        path: '/settings/restore',
        name: 'settings-restore',
        builder: (context, state) => const RestoreBackupScreen(),
      ),
      GoRoute(
        path: '/settings/export',
        name: 'settings-export',
        builder: (context, state) => const ScopedExportScreen(),
      ),
      GoRoute(
        path: '/settings/ai',
        name: 'settings-ai',
        builder: (context, state) => const AiSettingsHubScreen(),
      ),
      GoRoute(
        path: '/settings/ai/models',
        name: 'settings-ai-models',
        builder: (context, state) => const SignedModelCatalogScreen(),
      ),
      GoRoute(
        path: '/settings/ai/models/:releaseRef',
        name: 'settings-ai-model-detail',
        builder: (context, state) => SignedModelDetailScreen(
          releaseRef: state.pathParameters['releaseRef']!,
        ),
      ),
      GoRoute(
        path: '/settings/notifications',
        name: 'settings-notifications',
        builder: (context, state) => const NotificationSettingsScreen(),
      ),
      GoRoute(
        path: '/settings/language',
        name: 'settings-language',
        builder: (context, state) => const LanguageSettingsScreen(),
      ),
      GoRoute(
        path: '/settings/accessibility',
        name: 'settings-accessibility',
        builder: (context, state) => const AccessibilitySettingsScreen(),
      ),
      GoRoute(
        path: '/settings/permissions',
        name: 'settings-permissions',
        builder: (context, state) => const PermissionsSettingsScreen(),
      ),
      GoRoute(
        path: '/settings/background',
        name: 'settings-background',
        builder: (context, state) => const BackgroundPolicyScreen(),
      ),
      GoRoute(
        path: '/settings/diagnostics',
        name: 'settings-diagnostics',
        builder: (context, state) => const DiagnosticsScreen(),
      ),
      GoRoute(
        path: '/settings/advanced',
        name: 'settings-advanced',
        builder: (context, state) => const AdvancedGatesScreen(),
      ),
      GoRoute(
        path: '/settings/about',
        name: 'settings-about',
        builder: (context, state) => const AboutNodeScreen(),
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
