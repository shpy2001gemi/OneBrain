import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../design/onebrain_theme_extensions.dart';
import '../../domain/wave7_workspace_controller.dart';
import '../../l10n/app_localizations.dart';
import '../shared/obm_action_card.dart';
import '../shared/obm_button.dart';
import '../shared/obm_icon.dart';
import '../shared/obm_registry_plan.dart';
import '../shared/obm_scope_banner.dart';
import '../shared/obm_screen_frame.dart';
import '../shared/obm_wave7_components.dart';

/// `MOB-SCR-CAP-007` — capture review before optional deterministic encoding.
class CaptureReviewScreen extends ConsumerWidget {
  const CaptureReviewScreen({required this.operationRef, super.key});

  final String operationRef;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final strings = AppLocalizations.of(context);
    final w7 = ref.watch(wave7WorkspaceProvider);
    return ObmScreenFrame(
      title: 'Capture review',
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          ObmScopeBanner(
            title: strings.statusPrivate,
            body:
                'Opaque draft reference $operationRef is sealed in PrivateLocal storage.',
            tone: ObmStatusTone.pausedPrivate,
            icon: ObmSymbol.lock,
          ),
          SizedBox(height: context.spacing.lg),
          ObmButton(
            label: strings.wave7EncodeFromDraftAction,
            onPressed: () =>
                context.push('/capture/operations/$operationRef/encode'),
          ),
          if (!w7.isReadyOffline) ...[
            SizedBox(height: context.spacing.sm),
            Text(strings.registryRequiredReason),
          ],
        ],
      ),
    );
  }
}

/// `MOB-SCR-CAP-008` — capture completion receipt.
class CaptureResultScreen extends StatelessWidget {
  const CaptureResultScreen({required this.operationRef, super.key});

  final String operationRef;

  @override
  Widget build(BuildContext context) {
    final strings = AppLocalizations.of(context);
    return ObmScreenFrame(
      title: strings.draftSavedTitle,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          ObmVerificationReceipt(
            title: strings.draftSavedTitle,
            statusLabel: 'LOCAL_ONLY',
            tone: ObmStatusTone.ready,
            facts: [
              ObmPlanFact(label: 'Reference', value: operationRef),
              const ObmPlanFact(label: 'Disclosure', value: 'LOCAL_ONLY'),
              const ObmPlanFact(label: 'Author', value: 'Unresolved'),
              const ObmPlanFact(
                label: 'Fidelity',
                value: 'SELF_ATTESTED (local publisher attempt)',
              ),
            ],
          ),
          SizedBox(height: context.spacing.lg),
          ObmButton(
            label: strings.wave7MyKuTitle,
            onPressed: () => context.go('/library/my-ku'),
          ),
        ],
      ),
    );
  }
}

/// `MOB-SCR-ENC-001` — canonical KU composer gated on `ReadyOffline`.
class PrivateKuEncodeScreen extends ConsumerStatefulWidget {
  const PrivateKuEncodeScreen({required this.operationRef, super.key});

  final String operationRef;

  @override
  ConsumerState<PrivateKuEncodeScreen> createState() =>
      _PrivateKuEncodeScreenState();
}

class _PrivateKuEncodeScreenState extends ConsumerState<PrivateKuEncodeScreen> {
  final _titleController = TextEditingController(text: 'Copper conductivity');
  final _subjectController = TextEditingController(text: 'cu_legacy');
  final _predicateController = TextEditingController(text: 'conducts');
  final _objectController = TextEditingController(text: 'electricity');

  @override
  void dispose() {
    _titleController.dispose();
    _subjectController.dispose();
    _predicateController.dispose();
    _objectController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final strings = AppLocalizations.of(context);
    final w7 = ref.watch(wave7WorkspaceProvider);
    if (!w7.isReadyOffline) {
      return ObmScreenFrame(
        title: strings.wave7EncodeTitle,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            ObmScopeBanner(
              title: strings.limitedTitle,
              body: strings.wave7EncodeGateBody,
              tone: ObmStatusTone.waiting,
              icon: ObmSymbol.database,
              statusLabel: w7.registryReadiness,
            ),
            SizedBox(height: context.spacing.lg),
            ObmButton(
              label: strings.openInitAction,
              onPressed: () => context.push('/init'),
            ),
          ],
        ),
      );
    }

    return ObmScreenFrame(
      title: strings.wave7EncodeTitle,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          const ObmPrivacyScopePill(scopeLabel: 'LOCAL_ONLY · PrivateVault'),
          SizedBox(height: context.spacing.md),
          TextField(
            key: const Key('ku_encode_title_field'),
            controller: _titleController,
            decoration: const InputDecoration(labelText: 'KU title'),
          ),
          SizedBox(height: context.spacing.sm),
          TextField(
            key: const Key('ku_encode_subject_field'),
            controller: _subjectController,
            decoration: const InputDecoration(labelText: 'Subject concept'),
          ),
          SizedBox(height: context.spacing.sm),
          TextField(
            key: const Key('ku_encode_predicate_field'),
            controller: _predicateController,
            decoration: const InputDecoration(labelText: 'Predicate concept'),
          ),
          SizedBox(height: context.spacing.sm),
          TextField(
            key: const Key('ku_encode_object_field'),
            controller: _objectController,
            decoration: const InputDecoration(labelText: 'Object concept'),
          ),
          SizedBox(height: context.spacing.lg),
          ObmButton(
            key: const Key('ku_encode_preview_button'),
            label: strings.wave7EncodePreviewTitle,
            onPressed: () {
              ref
                  .read(wave7WorkspaceProvider.notifier)
                  .preparePrivateKu(
                    sourceDraftRef: widget.operationRef,
                    title: _titleController.text,
                    subjectLabel: _subjectController.text,
                    predicateLabel: _predicateController.text,
                    objectLabel: _objectController.text,
                  );
              context.push(
                '/capture/operations/${widget.operationRef}/encoding-review',
              );
            },
          ),
        ],
      ),
    );
  }
}

/// `MOB-SCR-ENC-002` — exact canonical KU preview & immutable `LOCAL_ONLY` Save.
class PrivateKuEncodingReviewScreen extends ConsumerWidget {
  const PrivateKuEncodingReviewScreen({required this.operationRef, super.key});

  final String operationRef;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final strings = AppLocalizations.of(context);
    final w7 = ref.watch(wave7WorkspaceProvider);
    final preview = w7.preparedKuByRef[operationRef];
    if (preview == null) {
      return ObmScreenFrame(
        title: strings.wave7EncodePreviewTitle,
        child: Text(strings.unavailableTitle),
      );
    }
    return ObmScreenFrame(
      title: strings.wave7EncodePreviewTitle,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          ObmVerificationReceipt(
            title: preview.title,
            statusLabel: 'CANDIDATE · LOCAL_ONLY',
            tone: ObmStatusTone.ready,
            facts: [
              ObmPlanFact(label: 'Candidate CID', value: preview.candidateCidHex),
              ObmPlanFact(
                label: 'Canonical SFS triple',
                value:
                    '(${preview.subjectLabel}, ${preview.predicateLabel}, ${preview.objectLabel})',
              ),
              ObmPlanFact(
                label: 'Canonical preview (Base64)',
                value: preview.canonicalPreviewBase64,
              ),
              for (final redirect in preview.redirectNotices)
                ObmPlanFact(label: 'Lifecycle rewrite', value: redirect),
            ],
          ),
          SizedBox(height: context.spacing.lg),
          ObmButton(
            key: const Key('ku_save_private_button'),
            label: strings.wave7SavePrivateKuAction,
            leadingIcon: ObmSymbol.lock,
            onPressed: () {
              final saved = ref
                  .read(wave7WorkspaceProvider.notifier)
                  .savePreparedKu(operationRef);
              context.go('/library/knowledge/${saved.objectCidHex}');
            },
          ),
        ],
      ),
    );
  }
}

/// `MOB-SCR-LIB-007` — cursor-paginated My Private KU shelf.
class MyKuShelfScreen extends ConsumerStatefulWidget {
  const MyKuShelfScreen({super.key});

  @override
  ConsumerState<MyKuShelfScreen> createState() => _MyKuShelfScreenState();
}

class _MyKuShelfScreenState extends ConsumerState<MyKuShelfScreen> {
  int _limit = 10;

  @override
  Widget build(BuildContext context) {
    final strings = AppLocalizations.of(context);
    final w7 = ref.watch(wave7WorkspaceProvider);
    final page = w7.savedKus.take(_limit).toList(growable: false);
    return ObmScreenFrame(
      title: strings.wave7MyKuTitle,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          const ObmPrivacyScopePill(scopeLabel: 'LOCAL_ONLY · Private shelf'),
          SizedBox(height: context.spacing.md),
          if (page.isEmpty)
            ObmActionCard(
              title: strings.wave7MyKuTitle,
              body: 'No saved LOCAL_ONLY Knowledge Units yet.',
              icon: ObmSymbol.library,
              tone: ObmStatusTone.information,
            )
          else
            for (final ku in page) ...[
              ObmActionCard(
                title: ku.title,
                body:
                    '${ku.objectCidHex} · (${ku.subjectLabel} ${ku.predicateLabel} ${ku.objectLabel}) · Fidelity: ${ku.fidelityStatus}',
                icon: ObmSymbol.library,
                tone: ObmStatusTone.pausedPrivate,
                statusLabel: '${ku.disclosure} · rev ${ku.revisionNumber}',
                actionLabel: strings.wave7KuDetailTitle,
                onPressed: () =>
                    context.push('/library/knowledge/${ku.objectCidHex}'),
              ),
              SizedBox(height: context.spacing.sm),
            ],
          if (w7.savedKus.length > _limit)
            ObmButton(
              label: 'Load next cursor page',
              variant: ObmButtonVariant.outline,
              onPressed: () => setState(() => _limit += 10),
            ),
        ],
      ),
    );
  }
}

/// `MOB-SCR-LIB-002` — structured local semantic search.
class SemanticSearchScreen extends ConsumerStatefulWidget {
  const SemanticSearchScreen({super.key});

  @override
  ConsumerState<SemanticSearchScreen> createState() =>
      _SemanticSearchScreenState();
}

class _SemanticSearchScreenState extends ConsumerState<SemanticSearchScreen> {
  String _query = '';

  @override
  Widget build(BuildContext context) {
    final strings = AppLocalizations.of(context);
    ref.watch(wave7WorkspaceProvider);
    final matches = ref
        .read(wave7WorkspaceProvider.notifier)
        .searchSemantic(_query);
    return ObmScreenFrame(
      title: strings.wave7SearchTitle,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          TextField(
            key: const Key('semantic_search_field'),
            decoration: InputDecoration(
              labelText: strings.wave7SearchTitle,
              hintText: 'copper, electricity…',
            ),
            onChanged: (v) => setState(() => _query = v),
          ),
          SizedBox(height: context.spacing.lg),
          for (final ku in matches) ...[
            ObmActionCard(
              title: ku.title,
              body:
                  '(${ku.subjectLabel}, ${ku.predicateLabel}, ${ku.objectLabel})',
              icon: ObmSymbol.search,
              tone: ObmStatusTone.ready,
              statusLabel: ku.disclosure,
              actionLabel: strings.wave7KuDetailTitle,
              onPressed: () =>
                  context.push('/library/knowledge/${ku.objectCidHex}'),
            ),
            SizedBox(height: context.spacing.sm),
          ],
        ],
      ),
    );
  }
}

/// `MOB-SCR-LIB-003` — read-only local KQL (`ku_kql::read_only`, rejecting writes/WATCH).
class LocalKqlScreen extends ConsumerStatefulWidget {
  const LocalKqlScreen({super.key});

  @override
  ConsumerState<LocalKqlScreen> createState() => _LocalKqlScreenState();
}

class _LocalKqlScreenState extends ConsumerState<LocalKqlScreen> {
  final _controller = TextEditingController(
    text: 'FIND (k:KU) WHERE gene_type = "Fact"',
  );

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final strings = AppLocalizations.of(context);
    final w7 = ref.watch(wave7WorkspaceProvider);
    final result = w7.lastKqlResult;
    return ObmScreenFrame(
      title: strings.wave7KqlTitle,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          ObmScopeBanner(
            title: strings.wave7KqlTitle,
            body:
                'Strictly read-only over local verified KUs. CREATE, UPDATE, DELETE, DEPRECATE and WATCH are rejected.',
            tone: ObmStatusTone.information,
            icon: ObmSymbol.database,
          ),
          SizedBox(height: context.spacing.md),
          TextField(
            key: const Key('local_kql_input_field'),
            controller: _controller,
            minLines: 2,
            maxLines: 4,
            decoration: InputDecoration(
              labelText: strings.wave7KqlTitle,
              hintText: strings.wave7KqlHint,
            ),
          ),
          SizedBox(height: context.spacing.md),
          ObmButton(
            key: const Key('local_kql_run_button'),
            label: strings.wave7RunKqlAction,
            onPressed: () => ref
                .read(wave7WorkspaceProvider.notifier)
                .runReadOnlyLocalKql(_controller.text),
          ),
          if (result != null) ...[
            SizedBox(height: context.spacing.lg),
            if (result.rejectedWriteReason != null)
              ObmScopeBanner(
                title: 'Read-only boundary enforced',
                body: result.rejectedWriteReason!,
                tone: ObmStatusTone.failed,
                icon: ObmSymbol.cancel,
              )
            else
              ObmVerificationReceipt(
                title: 'Local KQL results (${result.matchedRows.length})',
                statusLabel: 'READ_ONLY_LOCAL',
                tone: ObmStatusTone.ready,
                facts: [
                  ObmPlanFact(label: 'Query', value: result.query),
                  for (var i = 0; i < result.matchedRows.length; i++)
                    ObmPlanFact(
                      label: 'Row ${i + 1}',
                      value: result.matchedRows[i],
                    ),
                ],
              ),
          ],
        ],
      ),
    );
  }
}

/// `MOB-SCR-LIB-004` — bounded 2D local neighborhood view.
class LocalNeighborhoodScreen extends ConsumerWidget {
  const LocalNeighborhoodScreen({required this.localRef, super.key});

  final String localRef;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final strings = AppLocalizations.of(context);
    final w7 = ref.watch(wave7WorkspaceProvider);
    final ku = w7.savedKus.where((k) => k.objectCidHex == localRef).firstOrNull;
    return ObmScreenFrame(
      title: strings.wave7NeighborhoodTitle,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          ObmVerificationReceipt(
            title: ku?.title ?? localRef,
            statusLabel: '2D BOUNDED LOCAL GRAPH',
            tone: ObmStatusTone.information,
            facts: [
              ObmPlanFact(label: 'Center KU (0, 0)', value: localRef),
              ObmPlanFact(
                label: 'Subject node (100, 0)',
                value: ku?.subjectLabel ?? 'copper',
              ),
              ObmPlanFact(
                label: 'Predicate edge',
                value: ku?.predicateLabel ?? 'conducts',
              ),
              ObmPlanFact(
                label: 'Object node (0, 100)',
                value: ku?.objectLabel ?? 'electricity',
              ),
            ],
          ),
        ],
      ),
    );
  }
}

/// `MOB-SCR-LIB-005` — Concept Registry demand-paged browser.
class ConceptsBrowserScreen extends ConsumerWidget {
  const ConceptsBrowserScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final strings = AppLocalizations.of(context);
    final w7 = ref.watch(wave7WorkspaceProvider);
    return ObmScreenFrame(
      title: strings.wave7ConceptsBrowserTitle,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          for (final concept in w7.concepts) ...[
            ObmActionCard(
              title: concept.canonicalName,
              body:
                  'CCID ${concept.ccidHex} · Q${concept.qid} · ${concept.category}',
              icon: ObmSymbol.hub,
              tone: concept.lifecycleState == 'Canonical'
                  ? ObmStatusTone.ready
                  : ObmStatusTone.waiting,
              statusLabel: concept.lifecycleState,
              actionLabel: 'Inspect concept',
              onPressed: () =>
                  context.push('/library/concepts/${concept.ccidHex}'),
            ),
            SizedBox(height: context.spacing.sm),
          ],
        ],
      ),
    );
  }
}

/// `MOB-SCR-LIB-006` — Concept detail & lifecycle card.
class ConceptDetailScreen extends ConsumerWidget {
  const ConceptDetailScreen({required this.ccid, super.key});

  final String ccid;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final w7 = ref.watch(wave7WorkspaceProvider);
    final concept = w7.concepts.firstWhere(
      (c) => c.ccidHex == ccid,
      orElse: () => w7.concepts.first,
    );
    return ObmScreenFrame(
      title: concept.canonicalName,
      child: ObmVerificationReceipt(
        title: concept.canonicalName,
        statusLabel: concept.lifecycleState,
        tone: ObmStatusTone.ready,
        facts: [
          ObmPlanFact(label: 'CCID', value: concept.ccidHex),
          ObmPlanFact(label: 'Category', value: concept.category),
          ObmPlanFact(label: 'Wikidata QID', value: 'Q${concept.qid}'),
          ObmPlanFact(label: 'Lifecycle', value: concept.lifecycleState),
        ],
      ),
    );
  }
}

/// `MOB-SCR-KNO-001` — Private `LOCAL_ONLY` KU detail (no public publish routes!).
class PrivateKuDetailScreen extends ConsumerWidget {
  const PrivateKuDetailScreen({required this.localRef, super.key});

  final String localRef;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final strings = AppLocalizations.of(context);
    final w7 = ref.watch(wave7WorkspaceProvider);
    final ku = w7.savedKus.where((k) => k.objectCidHex == localRef).firstOrNull;
    if (ku == null) {
      return ObmScreenFrame(
        title: strings.wave7KuDetailTitle,
        child: Text(strings.unavailableTitle),
      );
    }
    return ObmScreenFrame(
      title: ku.title,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          ObmVerificationReceipt(
            title: ku.title,
            statusLabel: ku.disclosure,
            tone: ObmStatusTone.pausedPrivate,
            facts: [
              ObmPlanFact(label: 'Object CID', value: ku.objectCidHex),
              ObmPlanFact(label: 'Disclosure', value: ku.disclosure),
              ObmPlanFact(label: 'Author', value: ku.authorStatus),
              ObmPlanFact(label: 'Fidelity', value: ku.fidelityStatus),
              ObmPlanFact(
                label: 'Statement',
                value:
                    '(${ku.subjectLabel}, ${ku.predicateLabel}, ${ku.objectLabel})',
              ),
              ObmPlanFact(label: 'Revision', value: '${ku.revisionNumber}'),
              ObmPlanFact(label: 'Tags', value: ku.tags.join(', ')),
              ObmPlanFact(
                label: 'Collections',
                value: ku.collections.join(', '),
              ),
            ],
          ),
          SizedBox(height: context.spacing.lg),
          ObmButton(
            label: strings.wave7KuFidelityTitle,
            variant: ObmButtonVariant.tonal,
            onPressed: () =>
                context.push('/library/knowledge/${ku.objectCidHex}/fidelity'),
          ),
          SizedBox(height: context.spacing.sm),
          ObmButton(
            label: strings.wave7KuRevisionAction,
            variant: ObmButtonVariant.outline,
            onPressed: () =>
                context.push('/library/knowledge/${ku.objectCidHex}/edit'),
          ),
          SizedBox(height: context.spacing.sm),
          ObmButton(
            label: strings.wave7KuOrganizeAction,
            variant: ObmButtonVariant.outline,
            onPressed: () =>
                context.push('/library/knowledge/${ku.objectCidHex}/organize'),
          ),
          SizedBox(height: context.spacing.sm),
          ObmButton(
            label: strings.wave7KuWorkflowAction,
            variant: ObmButtonVariant.outline,
            onPressed: () =>
                context.push('/library/knowledge/${ku.objectCidHex}/workflow'),
          ),
          SizedBox(height: context.spacing.sm),
          ObmButton(
            label: strings.wave7KuLineageAction,
            variant: ObmButtonVariant.outline,
            onPressed: () =>
                context.push('/library/knowledge/${ku.objectCidHex}/conflicts'),
          ),
          SizedBox(height: context.spacing.sm),
          ObmButton(
            label: strings.wave7KuAlternateAction,
            variant: ObmButtonVariant.outline,
            onPressed: () =>
                context.push('/library/knowledge/${ku.objectCidHex}/encoding'),
          ),
          SizedBox(height: context.spacing.sm),
          ObmButton(
            label: strings.wave7NeighborhoodTitle,
            variant: ObmButtonVariant.text,
            onPressed: () => context.push('/library/graph/${ku.objectCidHex}'),
          ),
        ],
      ),
    );
  }
}

/// `MOB-SCR-KNO-002` — create new immutable `LOCAL_ONLY` KU revision.
class PrivateKuEditRevisionScreen extends ConsumerStatefulWidget {
  const PrivateKuEditRevisionScreen({required this.localRef, super.key});

  final String localRef;

  @override
  ConsumerState<PrivateKuEditRevisionScreen> createState() =>
      _PrivateKuEditRevisionScreenState();
}

class _PrivateKuEditRevisionScreenState
    extends ConsumerState<PrivateKuEditRevisionScreen> {
  final _titleController = TextEditingController(
    text: 'Copper conductivity (rev 2)',
  );
  final _objectController = TextEditingController(text: 'electricity');

  @override
  void dispose() {
    _titleController.dispose();
    _objectController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final strings = AppLocalizations.of(context);
    return ObmScreenFrame(
      title: strings.wave7KuRevisionAction,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          TextField(
            controller: _titleController,
            decoration: const InputDecoration(labelText: 'New revision title'),
          ),
          SizedBox(height: context.spacing.sm),
          TextField(
            controller: _objectController,
            decoration: const InputDecoration(labelText: 'Object concept'),
          ),
          SizedBox(height: context.spacing.lg),
          ObmButton(
            key: const Key('ku_commit_revision_button'),
            label: strings.wave7KuRevisionAction,
            onPressed: () {
              final next = ref
                  .read(wave7WorkspaceProvider.notifier)
                  .createImmutableKuRevision(
                    predecessorCidHex: widget.localRef,
                    newTitle: _titleController.text,
                    newObjectLabel: _objectController.text,
                  );
              context.go('/library/knowledge/${next.objectCidHex}');
            },
          ),
        ],
      ),
    );
  }
}

/// `MOB-SCR-KNO-003` — private local tags & collections.
class PrivateKuOrganizeScreen extends ConsumerStatefulWidget {
  const PrivateKuOrganizeScreen({required this.localRef, super.key});

  final String localRef;

  @override
  ConsumerState<PrivateKuOrganizeScreen> createState() =>
      _PrivateKuOrganizeScreenState();
}

class _PrivateKuOrganizeScreenState
    extends ConsumerState<PrivateKuOrganizeScreen> {
  final _tagsController = TextEditingController(text: 'materials, physics');
  final _collectionsController = TextEditingController(text: 'Lab Notes');

  @override
  void dispose() {
    _tagsController.dispose();
    _collectionsController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final strings = AppLocalizations.of(context);
    return ObmScreenFrame(
      title: strings.wave7KuOrganizeAction,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          TextField(
            controller: _tagsController,
            decoration: const InputDecoration(labelText: 'Private tags'),
          ),
          SizedBox(height: context.spacing.sm),
          TextField(
            controller: _collectionsController,
            decoration: const InputDecoration(labelText: 'Private collections'),
          ),
          SizedBox(height: context.spacing.lg),
          ObmButton(
            label: 'Save private organization',
            onPressed: () {
              ref
                  .read(wave7WorkspaceProvider.notifier)
                  .updateKuOrganization(
                    localRef: widget.localRef,
                    tags: _tagsController.text
                        .split(',')
                        .map((s) => s.trim())
                        .where((s) => s.isNotEmpty)
                        .toList(),
                    collections: _collectionsController.text
                        .split(',')
                        .map((s) => s.trim())
                        .where((s) => s.isNotEmpty)
                        .toList(),
                  );
              context.pop();
            },
          ),
        ],
      ),
    );
  }
}

/// `MOB-SCR-KNO-004` — 6-stage read-only workflow status inspection.
class PrivateKuWorkflowScreen extends StatelessWidget {
  const PrivateKuWorkflowScreen({required this.localRef, super.key});

  final String localRef;

  @override
  Widget build(BuildContext context) {
    final strings = AppLocalizations.of(context);
    return ObmScreenFrame(
      title: strings.wave7KuWorkflowAction,
      child: const ObmStepTimeline(
        steps: [
          ObmTimelineStep(
            title: '1. Private raw source sealed',
            detail: 'Authenticated XChaCha20-Poly1305 PrivateLocal draft.',
            tone: ObmStatusTone.ready,
            statusLabel: 'COMPLETE',
          ),
          ObmTimelineStep(
            title: '2. Concept lifecycle validated',
            detail: 'Canonical CCIDs resolved; redirects applied.',
            tone: ObmStatusTone.ready,
            statusLabel: 'COMPLETE',
          ),
          ObmTimelineStep(
            title: '3. Canonical SFS encoded',
            detail: 'Deterministic ObjectV1 envelope.',
            tone: ObmStatusTone.ready,
            statusLabel: 'COMPLETE',
          ),
          ObmTimelineStep(
            title: '4. Immutable LOCAL_ONLY saved',
            detail: 'Sealed in PrivateVault + RebuildableSemanticIndex.',
            tone: ObmStatusTone.ready,
            statusLabel: 'COMPLETE',
          ),
          ObmTimelineStep(
            title: '5. Local publisher EncodingAttempt recorded',
            detail: 'SELF_ATTESTED in RedbFidelityPortfolio.',
            tone: ObmStatusTone.ready,
            statusLabel: 'SELF_ATTESTED',
          ),
          ObmTimelineStep(
            title: '6. Public / blind-network corroboration',
            detail: 'Absent by design until MOB-GATE-NETWORKED-BETA.',
            tone: ObmStatusTone.offlineUnavailable,
            statusLabel: 'GATED OFF',
          ),
        ],
      ),
    );
  }
}

/// `MOB-SCR-KNO-005` — immutable revision & branch lineage view.
class PrivateKuLineageScreen extends ConsumerWidget {
  const PrivateKuLineageScreen({required this.localRef, super.key});

  final String localRef;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final strings = AppLocalizations.of(context);
    final w7 = ref.watch(wave7WorkspaceProvider);
    return ObmScreenFrame(
      title: strings.wave7KuLineageAction,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          for (final ku in w7.savedKus) ...[
            ObmCapabilityRow(
              title: '${ku.title} (rev ${ku.revisionNumber})',
              reason:
                  'CID: ${ku.objectCidHex} · Predecessor: ${ku.predecessorCidHex ?? 'Genesis'}',
              statusLabel: ku.disclosure,
              tone: ObmStatusTone.pausedPrivate,
            ),
            SizedBox(height: context.spacing.sm),
          ],
        ],
      ),
    );
  }
}

/// `MOB-SCR-ENC-003` — alternate candidate encodings for one source.
class PrivateKuAlternatesScreen extends ConsumerWidget {
  const PrivateKuAlternatesScreen({required this.localRef, super.key});

  final String localRef;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final strings = AppLocalizations.of(context);
    final w7 = ref.watch(wave7WorkspaceProvider);
    final ku = w7.savedKus.where((k) => k.objectCidHex == localRef).firstOrNull;
    return ObmScreenFrame(
      title: strings.wave7KuAlternateAction,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          for (final alt in ku?.alternateEncodings ?? const <String>[]) ...[
            ObmCapabilityRow(
              title: alt,
              reason: 'Coexists without overwriting canonical primary KU.',
              statusLabel: 'ALTERNATE',
              tone: ObmStatusTone.information,
            ),
            SizedBox(height: context.spacing.sm),
          ],
          ObmButton(
            label: 'Stage alternate encoding candidate',
            onPressed: () => ref
                .read(wave7WorkspaceProvider.notifier)
                .addAlternateEncoding(
                  localRef: localRef,
                  alternateSummary:
                      'Alternate SFS (${ku?.subjectLabel ?? 'copper'} -> thermal_conductor)',
                ),
          ),
        ],
      ),
    );
  }
}

/// `MOB-SCR-FID-001` — local `FidelityAssessmentReducer` portfolio view ONLY
/// (`MOB-SCR-FID-002..010` blind network screens remain unregistered!).
class PrivateKuFidelityScreen extends ConsumerWidget {
  const PrivateKuFidelityScreen({required this.localRef, super.key});

  final String localRef;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final strings = AppLocalizations.of(context);
    final w7 = ref.watch(wave7WorkspaceProvider);
    final ku = w7.savedKus.where((k) => k.objectCidHex == localRef).firstOrNull;
    return ObmScreenFrame(
      title: strings.wave7KuFidelityTitle,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          ObmScopeBanner(
            title: ku?.fidelityStatus ?? 'SELF_ATTESTED',
            body: strings.wave7KuFidelityDisclaimer,
            tone: ObmStatusTone.information,
            icon: ObmSymbol.shield,
          ),
          SizedBox(height: context.spacing.lg),
          ObmVerificationReceipt(
            title: 'Local FidelityAssessmentReducer summary',
            statusLabel: ku?.fidelityStatus ?? 'SELF_ATTESTED',
            tone: ObmStatusTone.ready,
            facts: [
              ObmPlanFact(label: 'Target KU', value: localRef),
              const ObmPlanFact(
                label: 'Publisher EncodingAttempt',
                value: 'Recorded locally',
              ),
              ObmPlanFact(
                label: 'Distinct 4D corroboration groups',
                value: '${ku?.distinct4dGroups ?? 0}',
              ),
              const ObmPlanFact(label: 'Global or final', value: 'false'),
              const ObmPlanFact(
                label: 'Establishes proposition truth',
                value: 'false',
              ),
            ],
          ),
        ],
      ),
    );
  }
}
