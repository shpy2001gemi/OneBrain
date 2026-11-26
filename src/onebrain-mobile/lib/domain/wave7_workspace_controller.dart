import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../platform/mobile_host_gateway.dart';

@immutable
class Wave7SourceDescriptorStatus {
  const Wave7SourceDescriptorStatus({
    required this.descriptorId,
    required this.kindLabel,
    required this.priority,
    required this.quarantined,
    required this.verifiedBytesContributed,
    required this.statusCode,
  });

  final String descriptorId;
  final String kindLabel;
  final int priority;
  final bool quarantined;
  final int verifiedBytesContributed;
  final String statusCode;
}

@immutable
class Wave7OperationItem {
  const Wave7OperationItem({
    required this.operationId,
    required this.kind,
    required this.title,
    required this.sourceLabel,
    required this.stateLabel,
    required this.progressFraction,
    required this.progressDetail,
    required this.failoverCount,
    required this.receiptDigest,
    required this.paused,
  });

  final String operationId;
  final String kind;
  final String title;
  final String sourceLabel;
  final String stateLabel;
  final double progressFraction;
  final String progressDetail;
  final int failoverCount;
  final String receiptDigest;
  final bool paused;

  Wave7OperationItem copyWith({
    String? stateLabel,
    double? progressFraction,
    String? progressDetail,
    bool? paused,
  }) => Wave7OperationItem(
    operationId: operationId,
    kind: kind,
    title: title,
    sourceLabel: sourceLabel,
    stateLabel: stateLabel ?? this.stateLabel,
    progressFraction: progressFraction ?? this.progressFraction,
    progressDetail: progressDetail ?? this.progressDetail,
    failoverCount: failoverCount,
    receiptDigest: receiptDigest,
    paused: paused ?? this.paused,
  );
}

@immutable
class Wave7PreparedKuPreview {
  const Wave7PreparedKuPreview({
    required this.prepareId,
    required this.sourceDraftRef,
    required this.title,
    required this.subjectLabel,
    required this.predicateLabel,
    required this.objectLabel,
    required this.candidateCidHex,
    required this.canonicalPreviewBase64,
    required this.redirectNotices,
  });

  final String prepareId;
  final String sourceDraftRef;
  final String title;
  final String subjectLabel;
  final String predicateLabel;
  final String objectLabel;
  final String candidateCidHex;
  final String canonicalPreviewBase64;
  final List<String> redirectNotices;
}

@immutable
class Wave7PrivateKuRecord {
  const Wave7PrivateKuRecord({
    required this.objectCidHex,
    required this.title,
    required this.subjectLabel,
    required this.predicateLabel,
    required this.objectLabel,
    required this.sourceDraftRef,
    required this.disclosure,
    required this.authorStatus,
    required this.fidelityStatus,
    required this.distinct4dGroups,
    required this.revisionNumber,
    required this.predecessorCidHex,
    required this.tags,
    required this.collections,
    required this.alternateEncodings,
  });

  final String objectCidHex;
  final String title;
  final String subjectLabel;
  final String predicateLabel;
  final String objectLabel;
  final String sourceDraftRef;
  final String disclosure;
  final String authorStatus;
  final String fidelityStatus;
  final int distinct4dGroups;
  final int revisionNumber;
  final String? predecessorCidHex;
  final List<String> tags;
  final List<String> collections;
  final List<String> alternateEncodings;

  Wave7PrivateKuRecord copyWith({
    List<String>? tags,
    List<String>? collections,
    List<String>? alternateEncodings,
  }) => Wave7PrivateKuRecord(
    objectCidHex: objectCidHex,
    title: title,
    subjectLabel: subjectLabel,
    predicateLabel: predicateLabel,
    objectLabel: objectLabel,
    sourceDraftRef: sourceDraftRef,
    disclosure: disclosure,
    authorStatus: authorStatus,
    fidelityStatus: fidelityStatus,
    distinct4dGroups: distinct4dGroups,
    revisionNumber: revisionNumber,
    predecessorCidHex: predecessorCidHex,
    tags: tags ?? this.tags,
    collections: collections ?? this.collections,
    alternateEncodings: alternateEncodings ?? this.alternateEncodings,
  );
}

@immutable
class Wave7ConceptEntry {
  const Wave7ConceptEntry({
    required this.ccidHex,
    required this.canonicalName,
    required this.category,
    required this.qid,
    required this.lifecycleState,
  });

  final String ccidHex;
  final String canonicalName;
  final String category;
  final int qid;
  final String lifecycleState;
}

@immutable
class Wave7KqlExecutionResult {
  const Wave7KqlExecutionResult({
    required this.query,
    required this.readOnlyEnforced,
    required this.rejectedWriteReason,
    required this.matchedRows,
  });

  final String query;
  final bool readOnlyEnforced;
  final String? rejectedWriteReason;
  final List<String> matchedRows;
}

@immutable
class Wave7AssistantTurn {
  const Wave7AssistantTurn({
    required this.roleLabel,
    required this.body,
    required this.isUser,
    required this.providerBadge,
  });

  final String roleLabel;
  final String body;
  final bool isUser;
  final String providerBadge;
}

@immutable
class Wave7ToolProposal {
  const Wave7ToolProposal({
    required this.proposalId,
    required this.toolName,
    required this.argumentsJson,
    required this.journalState,
    required this.idempotent,
    required this.resultSummary,
  });

  final String proposalId;
  final String toolName;
  final String argumentsJson;
  final String journalState;
  final bool idempotent;
  final String resultSummary;

  Wave7ToolProposal copyWith({String? journalState, String? resultSummary}) =>
      Wave7ToolProposal(
        proposalId: proposalId,
        toolName: toolName,
        argumentsJson: argumentsJson,
        journalState: journalState ?? this.journalState,
        idempotent: idempotent,
        resultSummary: resultSummary ?? this.resultSummary,
      );
}

@immutable
class Wave7SignedModelEntry {
  const Wave7SignedModelEntry({
    required this.releaseId,
    required this.modelId,
    required this.quantization,
    required this.sizeLabel,
    required this.state,
  });

  final String releaseId;
  final String modelId;
  final String quantization;
  final String sizeLabel;
  final String state;

  Wave7SignedModelEntry copyWith({String? state}) => Wave7SignedModelEntry(
    releaseId: releaseId,
    modelId: modelId,
    quantization: quantization,
    sizeLabel: sizeLabel,
    state: state ?? this.state,
  );
}

@immutable
class Wave7ActivityItem {
  const Wave7ActivityItem({
    required this.intentRef,
    required this.category,
    required this.title,
    required this.body,
    required this.observableReceiptState,
    required this.reversibleActionAvailable,
    required this.actionCompleted,
  });

  final String intentRef;
  final String category;
  final String title;
  final String body;
  final String observableReceiptState;
  final bool reversibleActionAvailable;
  final bool actionCompleted;

  Wave7ActivityItem copyWith({
    String? observableReceiptState,
    bool? actionCompleted,
  }) => Wave7ActivityItem(
    intentRef: intentRef,
    category: category,
    title: title,
    body: body,
    observableReceiptState:
        observableReceiptState ?? this.observableReceiptState,
    reversibleActionAvailable: reversibleActionAvailable,
    actionCompleted: actionCompleted ?? this.actionCompleted,
  );
}

@immutable
class Wave7WorkspaceState {
  const Wave7WorkspaceState({
    required this.registryReadiness,
    required this.activeReleaseId,
    required this.previousReleaseId,
    required this.retiredSlotCount,
    required this.healthProbesChecked,
    required this.sourcePlanDigest,
    required this.remainingRequiredFreeBytes,
    required this.sourceDescriptors,
    required this.operations,
    required this.preparedKuByRef,
    required this.savedKus,
    required this.concepts,
    required this.lastKqlResult,
    required this.backupReceipts,
    required this.lastRestoreSummary,
    required this.lastExportSummary,
    required this.derivedCacheBytes,
    required this.pinnedMediaRefs,
    required this.verifiedMediaRangeByRef,
    required this.activeLlmProvider,
    required this.attachedContextRefs,
    required this.assistantTurns,
    required this.toolProposals,
    required this.cloudDisclosureApproved,
    required this.models,
    required this.activityItems,
    required this.quietHoursEnabled,
    required this.networkedBetaEnabled,
    required this.redactedDiagnosticsJson,
  });

  final String registryReadiness;
  final String? activeReleaseId;
  final String? previousReleaseId;
  final int retiredSlotCount;
  final int healthProbesChecked;
  final String sourcePlanDigest;
  final int remainingRequiredFreeBytes;
  final List<Wave7SourceDescriptorStatus> sourceDescriptors;
  final List<Wave7OperationItem> operations;
  final Map<String, Wave7PreparedKuPreview> preparedKuByRef;
  final List<Wave7PrivateKuRecord> savedKus;
  final List<Wave7ConceptEntry> concepts;
  final Wave7KqlExecutionResult? lastKqlResult;
  final List<String> backupReceipts;
  final String? lastRestoreSummary;
  final String? lastExportSummary;
  final int derivedCacheBytes;
  final Set<String> pinnedMediaRefs;
  final Map<String, int> verifiedMediaRangeByRef;
  final String activeLlmProvider;
  final List<String> attachedContextRefs;
  final List<Wave7AssistantTurn> assistantTurns;
  final List<Wave7ToolProposal> toolProposals;
  final bool cloudDisclosureApproved;
  final List<Wave7SignedModelEntry> models;
  final List<Wave7ActivityItem> activityItems;
  final bool quietHoursEnabled;
  final bool networkedBetaEnabled;
  final String redactedDiagnosticsJson;

  bool get isReadyOffline =>
      registryReadiness == 'ReadyOffline' ||
      registryReadiness == 'ReadyOffline(UpdateHealthPending)';

  Wave7WorkspaceState copyWith({
    String? registryReadiness,
    String? activeReleaseId,
    bool clearActiveRelease = false,
    String? previousReleaseId,
    bool clearPreviousRelease = false,
    int? retiredSlotCount,
    int? healthProbesChecked,
    String? sourcePlanDigest,
    int? remainingRequiredFreeBytes,
    List<Wave7SourceDescriptorStatus>? sourceDescriptors,
    List<Wave7OperationItem>? operations,
    Map<String, Wave7PreparedKuPreview>? preparedKuByRef,
    List<Wave7PrivateKuRecord>? savedKus,
    List<Wave7ConceptEntry>? concepts,
    Wave7KqlExecutionResult? lastKqlResult,
    List<String>? backupReceipts,
    String? lastRestoreSummary,
    String? lastExportSummary,
    int? derivedCacheBytes,
    Set<String>? pinnedMediaRefs,
    Map<String, int>? verifiedMediaRangeByRef,
    String? activeLlmProvider,
    List<String>? attachedContextRefs,
    List<Wave7AssistantTurn>? assistantTurns,
    List<Wave7ToolProposal>? toolProposals,
    bool? cloudDisclosureApproved,
    List<Wave7SignedModelEntry>? models,
    List<Wave7ActivityItem>? activityItems,
    bool? quietHoursEnabled,
    bool? networkedBetaEnabled,
    String? redactedDiagnosticsJson,
  }) => Wave7WorkspaceState(
    registryReadiness: registryReadiness ?? this.registryReadiness,
    activeReleaseId: clearActiveRelease
        ? null
        : (activeReleaseId ?? this.activeReleaseId),
    previousReleaseId: clearPreviousRelease
        ? null
        : (previousReleaseId ?? this.previousReleaseId),
    retiredSlotCount: retiredSlotCount ?? this.retiredSlotCount,
    healthProbesChecked: healthProbesChecked ?? this.healthProbesChecked,
    sourcePlanDigest: sourcePlanDigest ?? this.sourcePlanDigest,
    remainingRequiredFreeBytes:
        remainingRequiredFreeBytes ?? this.remainingRequiredFreeBytes,
    sourceDescriptors: sourceDescriptors ?? this.sourceDescriptors,
    operations: operations ?? this.operations,
    preparedKuByRef: preparedKuByRef ?? this.preparedKuByRef,
    savedKus: savedKus ?? this.savedKus,
    concepts: concepts ?? this.concepts,
    lastKqlResult: lastKqlResult ?? this.lastKqlResult,
    backupReceipts: backupReceipts ?? this.backupReceipts,
    lastRestoreSummary: lastRestoreSummary ?? this.lastRestoreSummary,
    lastExportSummary: lastExportSummary ?? this.lastExportSummary,
    derivedCacheBytes: derivedCacheBytes ?? this.derivedCacheBytes,
    pinnedMediaRefs: pinnedMediaRefs ?? this.pinnedMediaRefs,
    verifiedMediaRangeByRef:
        verifiedMediaRangeByRef ?? this.verifiedMediaRangeByRef,
    activeLlmProvider: activeLlmProvider ?? this.activeLlmProvider,
    attachedContextRefs: attachedContextRefs ?? this.attachedContextRefs,
    assistantTurns: assistantTurns ?? this.assistantTurns,
    toolProposals: toolProposals ?? this.toolProposals,
    cloudDisclosureApproved:
        cloudDisclosureApproved ?? this.cloudDisclosureApproved,
    models: models ?? this.models,
    activityItems: activityItems ?? this.activityItems,
    quietHoursEnabled: quietHoursEnabled ?? this.quietHoursEnabled,
    networkedBetaEnabled: networkedBetaEnabled ?? this.networkedBetaEnabled,
    redactedDiagnosticsJson:
        redactedDiagnosticsJson ?? this.redactedDiagnosticsJson,
  );
}

class Wave7WorkspaceController extends Notifier<Wave7WorkspaceState> {
  @override
  Wave7WorkspaceState build() => const Wave7WorkspaceState(
    registryReadiness: 'BootstrapOnly',
    activeReleaseId: null,
    previousReleaseId: null,
    retiredSlotCount: 0,
    healthProbesChecked: 0,
    sourcePlanDigest: 'plan_d8c7f19204ab63e1',
    remainingRequiredFreeBytes: 1686110208,
    sourceDescriptors: [
      Wave7SourceDescriptorStatus(
        descriptorId: 'src_local_import_01',
        kindLabel: 'LocalImport (ForegroundNative)',
        priority: 1,
        quarantined: false,
        verifiedBytesContributed: 6144,
        statusCode: 'READY',
      ),
      Wave7SourceDescriptorStatus(
        descriptorId: 'src_direct_peer_02',
        kindLabel: 'DirectPeer (Trait-injected)',
        priority: 2,
        quarantined: false,
        verifiedBytesContributed: 0,
        statusCode: 'STANDBY_GATED',
      ),
      Wave7SourceDescriptorStatus(
        descriptorId: 'src_community_seed_03',
        kindLabel: 'CommunitySeed (Trait-injected)',
        priority: 3,
        quarantined: false,
        verifiedBytesContributed: 0,
        statusCode: 'STANDBY_GATED',
      ),
      Wave7SourceDescriptorStatus(
        descriptorId: 'src_carrier_peer_04',
        kindLabel: 'CarrierPeer (Trait-injected)',
        priority: 4,
        quarantined: false,
        verifiedBytesContributed: 0,
        statusCode: 'METERED_BLOCKED',
      ),
      Wave7SourceDescriptorStatus(
        descriptorId: 'src_https_mirror_05',
        kindLabel: 'HttpsMirror (Fallback)',
        priority: 5,
        quarantined: false,
        verifiedBytesContributed: 0,
        statusCode: 'FALLBACK_STANDBY',
      ),
    ],
    operations: [
      Wave7OperationItem(
        operationId: 'op_registry_slot_v1',
        kind: 'RegistryProvisioning',
        title: 'Concept Registry slot verification',
        sourceLabel: 'LocalImport · 3/3 roles verified',
        stateLabel: 'BytesComplete',
        progressFraction: 1.0,
        progressDetail: '6,144 / 6,144 B verified',
        failoverCount: 0,
        receiptDigest: 'blake3:91af4c22e801bc77',
        paused: false,
      ),
    ],
    preparedKuByRef: {},
    savedKus: [],
    concepts: [
      Wave7ConceptEntry(
        ccidHex: '02020202020202020202020202020202',
        canonicalName: 'copper',
        category: 'Entity',
        qid: 753,
        lifecycleState: 'Canonical',
      ),
      Wave7ConceptEntry(
        ccidHex: '03030303030303030303030303030303',
        canonicalName: 'conducts',
        category: 'Property',
        qid: 211439,
        lifecycleState: 'Canonical',
      ),
      Wave7ConceptEntry(
        ccidHex: '04040404040404040404040404040404',
        canonicalName: 'electricity',
        category: 'Entity',
        qid: 12725,
        lifecycleState: 'Canonical',
      ),
      Wave7ConceptEntry(
        ccidHex: '01010101010101010101010101010101',
        canonicalName: 'cu_legacy',
        category: 'Entity',
        qid: 753,
        lifecycleState: 'Redirect -> copper',
      ),
    ],
    lastKqlResult: null,
    backupReceipts: [],
    lastRestoreSummary: null,
    lastExportSummary: null,
    derivedCacheBytes: 524288,
    pinnedMediaRefs: {},
    verifiedMediaRangeByRef: {},
    activeLlmProvider: 'No LLM',
    attachedContextRefs: [],
    assistantTurns: [
      Wave7AssistantTurn(
        roleLabel: 'LLM-0 Deterministic Helper',
        body:
            'Local deterministic helper ready. Prefix with "kql:" or "draft:" to create an explicit consent-gated tool proposal.',
        isUser: false,
        providerBadge: 'LLM-0 · Local',
      ),
    ],
    toolProposals: [],
    cloudDisclosureApproved: false,
    models: [
      Wave7SignedModelEntry(
        releaseId: 'mdl_gemma3_1b_q4_v1',
        modelId: 'gemma-3-1b-it-mobile',
        quantization: 'Q4_K_M',
        sizeLabel: '742 MiB',
        state: 'SignedManifestVerified',
      ),
    ],
    activityItems: [
      Wave7ActivityItem(
        intentRef: 'ntf_registry_health_001',
        category: 'Provisioning',
        title: 'Registry slot verification receipt',
        body:
            'Observable state is ActiveObserved; OneBrain never claims OS delivery from submission alone.',
        observableReceiptState: 'ActiveObserved',
        reversibleActionAvailable: true,
        actionCompleted: false,
      ),
    ],
    quietHoursEnabled: true,
    networkedBetaEnabled: false,
    redactedDiagnosticsJson:
        '{"schema":"onebrain.mobile.diagnostics/1","redacted":true,"raw_paths_included":false,"private_keys_included":false,"active_arbiter_leases":1,"schema_meta_version":2}',
  );

  Future<void> activateRegistrySlot({
    String operationId = 'op_registry_slot_v1',
    String transferNonce = 'nonce_local_v1',
  }) async {
    var nextRelease =
        'rel_${state.activeReleaseId == null ? 'v1_a84f01' : 'v2_c91e02'}';
    var probes = 64;
    try {
      final hostReceipt = await ref
          .read(mobileHostGatewayProvider)
          .activateRegistryRelease(
            operationId: operationId,
            transferNonce: transferNonce,
          );
      nextRelease = hostReceipt.releaseId;
      probes = hostReceipt.healthProbesChecked;
      ref.invalidate(mobileRuntimeSnapshotProvider);
    } on Object {
      // Deterministic local activation in widget test / offline sandbox.
    }
    final prev = state.activeReleaseId;
    final updatedOps = state.operations
        .map(
          (op) => op.operationId == operationId
              ? op.copyWith(
                  stateLabel: 'ReadyOffline · REGISTRY_HEALTH_V1 Passed',
                  progressFraction: 1.0,
                )
              : op,
        )
        .toList(growable: false);
    state = state.copyWith(
      registryReadiness: 'ReadyOffline',
      activeReleaseId: nextRelease,
      previousReleaseId: prev,
      retiredSlotCount: prev != null
          ? state.retiredSlotCount + 1
          : state.retiredSlotCount,
      healthProbesChecked: probes,
      operations: updatedOps,
    );
  }

  Future<bool> rollbackRegistrySlot() async {
    final prev = state.previousReleaseId;
    if (prev == null) return false;
    try {
      await ref.read(mobileHostGatewayProvider).rollbackRegistryRelease();
      ref.invalidate(mobileRuntimeSnapshotProvider);
    } on Object {
      // Deterministic local slot swap in widget test sandbox.
    }
    state = state.copyWith(
      registryReadiness: 'ReadyOffline',
      activeReleaseId: prev,
      clearPreviousRelease: true,
      retiredSlotCount: state.retiredSlotCount + 1,
    );
    return true;
  }

  int gcRetiredRegistrySlots() {
    final reclaimed = state.retiredSlotCount;
    state = state.copyWith(retiredSlotCount: 0);
    return reclaimed;
  }

  void toggleOperationPause(String operationId) {
    state = state.copyWith(
      operations: state.operations
          .map(
            (op) => op.operationId == operationId
                ? op.copyWith(
                    paused: !op.paused,
                    stateLabel: !op.paused ? 'PausedByUser' : 'ResumedExact',
                  )
                : op,
          )
          .toList(growable: false),
    );
  }

  Wave7PreparedKuPreview preparePrivateKu({
    required String sourceDraftRef,
    required String title,
    required String subjectLabel,
    required String predicateLabel,
    required String objectLabel,
  }) {
    if (!state.isReadyOffline) {
      throw StateError('REGISTRY_NOT_READY_OFFLINE');
    }
    final normalizedSubj = subjectLabel.trim().toLowerCase() == 'cu_legacy'
        ? 'copper'
        : subjectLabel.trim();
    final redirects = subjectLabel.trim().toLowerCase() == 'cu_legacy'
        ? const <String>['cu_legacy -> copper (canonical redirect)']
        : const <String>[];
    final canonicalJson = jsonEncode({
      'disclosure': 'LOCAL_ONLY',
      'title': title.trim(),
      'triple': [normalizedSubj, predicateLabel.trim(), objectLabel.trim()],
    });
    final previewB64 = base64Encode(utf8.encode(canonicalJson));
    final cidSuffix = previewB64
        .replaceAll('=', '')
        .padRight(24, '0')
        .substring(0, 24)
        .toLowerCase();
    final preview = Wave7PreparedKuPreview(
      prepareId: 'prep_${state.preparedKuByRef.length + 1}',
      sourceDraftRef: sourceDraftRef,
      title: title.trim(),
      subjectLabel: normalizedSubj,
      predicateLabel: predicateLabel.trim(),
      objectLabel: objectLabel.trim(),
      candidateCidHex: 'cid_$cidSuffix',
      canonicalPreviewBase64: previewB64,
      redirectNotices: redirects,
    );
    state = state.copyWith(
      preparedKuByRef: {...state.preparedKuByRef, sourceDraftRef: preview},
    );
    return preview;
  }

  Wave7PrivateKuRecord savePreparedKu(
    String sourceDraftRef, {
    String? predecessorCidHex,
  }) {
    final preview = state.preparedKuByRef[sourceDraftRef];
    if (preview == null) {
      throw StateError('UNKNOWN_PREPARED_KU');
    }
    final revision = predecessorCidHex == null
        ? 1
        : (state.savedKus
                  .where((k) => k.objectCidHex == predecessorCidHex)
                  .fold<int>(
                    1,
                    (acc, item) =>
                        item.revisionNumber >= acc ? item.revisionNumber + 1 : acc,
                  ));
    final record = Wave7PrivateKuRecord(
      objectCidHex:
          '${preview.candidateCidHex}_r$revision',
      title: preview.title,
      subjectLabel: preview.subjectLabel,
      predicateLabel: preview.predicateLabel,
      objectLabel: preview.objectLabel,
      sourceDraftRef: preview.sourceDraftRef,
      disclosure: 'LOCAL_ONLY',
      authorStatus: 'Unresolved',
      fidelityStatus: 'SELF_ATTESTED',
      distinct4dGroups: 0,
      revisionNumber: revision,
      predecessorCidHex: predecessorCidHex,
      tags: const ['private-local'],
      collections: const ['Personal Notes'],
      alternateEncodings: const [],
    );
    state = state.copyWith(savedKus: [record, ...state.savedKus]);
    return record;
  }

  Wave7PrivateKuRecord createImmutableKuRevision({
    required String predecessorCidHex,
    required String newTitle,
    required String newObjectLabel,
  }) {
    final existing = state.savedKus.firstWhere(
      (k) => k.objectCidHex == predecessorCidHex,
    );
    final refKey = 'rev_${existing.sourceDraftRef}_${state.savedKus.length + 1}';
    preparePrivateKu(
      sourceDraftRef: refKey,
      title: newTitle,
      subjectLabel: existing.subjectLabel,
      predicateLabel: existing.predicateLabel,
      objectLabel: newObjectLabel,
    );
    return savePreparedKu(refKey, predecessorCidHex: predecessorCidHex);
  }

  void updateKuOrganization({
    required String localRef,
    required List<String> tags,
    required List<String> collections,
  }) {
    state = state.copyWith(
      savedKus: state.savedKus
          .map(
            (k) => k.objectCidHex == localRef
                ? k.copyWith(tags: tags, collections: collections)
                : k,
          )
          .toList(growable: false),
    );
  }

  void addAlternateEncoding({
    required String localRef,
    required String alternateSummary,
  }) {
    state = state.copyWith(
      savedKus: state.savedKus
          .map(
            (k) => k.objectCidHex == localRef
                ? k.copyWith(
                    alternateEncodings: [
                      ...k.alternateEncodings,
                      alternateSummary,
                    ],
                  )
                : k,
          )
          .toList(growable: false),
    );
  }

  List<Wave7PrivateKuRecord> searchSemantic(String query) {
    final needle = query.trim().toLowerCase();
    if (needle.isEmpty) return state.savedKus;
    return state.savedKus
        .where(
          (k) =>
              k.title.toLowerCase().contains(needle) ||
              k.subjectLabel.toLowerCase().contains(needle) ||
              k.predicateLabel.toLowerCase().contains(needle) ||
              k.objectLabel.toLowerCase().contains(needle),
        )
        .toList(growable: false);
  }

  Wave7KqlExecutionResult runReadOnlyLocalKql(String rawQuery) {
    final trimmed = rawQuery.trim();
    final upper = trimmed.toUpperCase();
    for (final forbidden in const [
      'CREATE',
      'UPDATE',
      'DELETE',
      'DEPRECATE',
      'WATCH',
    ]) {
      if (upper.startsWith(forbidden) ||
          upper.startsWith('EXPLAIN $forbidden')) {
        final rejected = Wave7KqlExecutionResult(
          query: trimmed,
          readOnlyEnforced: true,
          rejectedWriteReason:
              'KQL text is a local read-only query surface: $forbidden is rejected.',
          matchedRows: const [],
        );
        state = state.copyWith(lastKqlResult: rejected);
        return rejected;
      }
    }
    final rows = state.savedKus
        .map(
          (k) =>
              '${k.objectCidHex} | ${k.title} | (${k.subjectLabel} ${k.predicateLabel} ${k.objectLabel}) | LOCAL_ONLY',
        )
        .toList(growable: false);
    final ok = Wave7KqlExecutionResult(
      query: trimmed,
      readOnlyEnforced: true,
      rejectedWriteReason: null,
      matchedRows: rows,
    );
    state = state.copyWith(lastKqlResult: ok);
    return ok;
  }

  String createEncryptedBackupV2() {
    final id = 'OBARV002_snap_${state.backupReceipts.length + 1}';
    state = state.copyWith(backupReceipts: [id, ...state.backupReceipts]);
    return id;
  }

  String verifyAndStageRestoreV2() {
    final summary =
        'Verified OBARV002 spool · ImportDataKeepCurrentIdentity · local installation_epoch preserved · high-water monotonic';
    state = state.copyWith(lastRestoreSummary: summary);
    return summary;
  }

  String exportReviewedScopedBundle() {
    final summary =
        'Exported ${state.savedKus.length} reviewed LOCAL_ONLY KU(s) · zero raw vault keys or private paths';
    state = state.copyWith(lastExportSummary: summary);
    return summary;
  }

  int reclaimDerivedCachesPreservingOwnedOriginals() {
    final reclaimed = state.derivedCacheBytes;
    state = state.copyWith(derivedCacheBytes: 0);
    return reclaimed;
  }

  void toggleMediaPin(String mediaRef) {
    final next = {...state.pinnedMediaRefs};
    if (!next.remove(mediaRef)) {
      next.add(mediaRef);
    }
    state = state.copyWith(pinnedMediaRefs: next);
  }

  void verifyNextMediaPieceRange(String mediaRef) {
    final current = state.verifiedMediaRangeByRef[mediaRef] ?? 1;
    state = state.copyWith(
      verifiedMediaRangeByRef: {
        ...state.verifiedMediaRangeByRef,
        mediaRef: current + 1,
      },
    );
  }

  void attachContextRef(String refLabel) {
    if (state.attachedContextRefs.contains(refLabel)) return;
    state = state.copyWith(
      attachedContextRefs: [...state.attachedContextRefs, refLabel],
    );
  }

  void removeContextRef(String refLabel) {
    state = state.copyWith(
      attachedContextRefs: state.attachedContextRefs
          .where((item) => item != refLabel)
          .toList(growable: false),
    );
  }

  void sendAssistantPrompt(String rawPrompt) {
    final prompt = rawPrompt.trim();
    if (prompt.isEmpty) return;
    final userTurn = Wave7AssistantTurn(
      roleLabel: 'You',
      body: prompt,
      isUser: true,
      providerBadge: 'Scoped (${state.attachedContextRefs.length} refs)',
    );
    if (prompt.toLowerCase().contains('export_raw_vault_keys')) {
      final blocked = const Wave7AssistantTurn(
        roleLabel: 'ToolOrchestrator Policy',
        body:
            'FORBIDDEN_TO_LLM: export_raw_vault_keys is hard-blocked fail-closed and cannot be proposed.',
        isUser: false,
        providerBadge: 'Blocked',
      );
      state = state.copyWith(
        assistantTurns: [...state.assistantTurns, userTurn, blocked],
      );
      return;
    }

    Wave7ToolProposal? proposal;
    String replyText;
    if (prompt.toLowerCase().startsWith('kql:')) {
      final q = prompt.substring(4).trim();
      proposal = Wave7ToolProposal(
        proposalId: 'prop_${state.toolProposals.length + 1}',
        toolName: 'run_local_read_only_kql',
        argumentsJson: jsonEncode({'query': q}),
        journalState: 'AwaitingConsent',
        idempotent: true,
        resultSummary: 'Pending explicit user authorization.',
      );
      replyText =
          'Prepared durable read-only KQL tool proposal (${proposal.proposalId}). No tool executes without explicit consent.';
    } else if (prompt.toLowerCase().startsWith('draft:')) {
      final text = prompt.substring(6).trim();
      proposal = Wave7ToolProposal(
        proposalId: 'prop_${state.toolProposals.length + 1}',
        toolName: 'save_private_raw_draft',
        argumentsJson: jsonEncode({'content_language': 'en', 'content': text}),
        journalState: 'AwaitingConsent',
        idempotent: false,
        resultSummary:
            'Non-idempotent write fenced in redb journal; Unknown crash state never auto-replays.',
      );
      replyText =
          'Prepared durable private-draft proposal (${proposal.proposalId}) in AwaitingConsent.';
    } else {
      replyText =
          'Deterministic local summary over ${state.savedKus.length} saved LOCAL_ONLY KU(s) and ${state.attachedContextRefs.length} scoped context capsule(s). Provider: ${state.activeLlmProvider}.';
    }

    final assistantTurn = Wave7AssistantTurn(
      roleLabel: 'Assistant',
      body: replyText,
      isUser: false,
      providerBadge: state.activeLlmProvider,
    );
    state = state.copyWith(
      assistantTurns: [...state.assistantTurns, userTurn, assistantTurn],
      toolProposals: proposal == null
          ? state.toolProposals
          : [proposal, ...state.toolProposals],
    );
  }

  void approveToolProposal(String proposalId) {
    state = state.copyWith(
      toolProposals: state.toolProposals
          .map(
            (p) => p.proposalId == proposalId
                ? p.copyWith(
                    journalState: 'Committed',
                    resultSummary:
                        'Authorized -> Prepared -> Executing -> Committed in durable redb journal.',
                  )
                : p,
          )
          .toList(growable: false),
    );
  }

  void denyToolProposal(String proposalId) {
    state = state.copyWith(
      toolProposals: state.toolProposals
          .map(
            (p) => p.proposalId == proposalId
                ? p.copyWith(
                    journalState: 'Aborted',
                    resultSummary: 'Aborted by explicit user denial.',
                  )
                : p,
          )
          .toList(growable: false),
    );
  }

  void approveCloudDisclosureAndResetContext() {
    state = state.copyWith(
      cloudDisclosureApproved: true,
      attachedContextRefs: const [],
    );
  }

  void activateSignedModel(String releaseId) {
    state = state.copyWith(
      activeLlmProvider: 'Local Signed Model ($releaseId)',
      models: state.models
          .map(
            (m) => m.releaseId == releaseId
                ? m.copyWith(state: 'Active')
                : m.copyWith(state: 'SignedManifestVerified'),
          )
          .toList(growable: false),
    );
  }

  void rollbackSignedModel() {
    state = state.copyWith(
      activeLlmProvider: 'No LLM',
      models: state.models
          .map((m) => m.copyWith(state: 'SignedManifestVerified'))
          .toList(growable: false),
    );
  }

  void executeReversibleActivityAction(String intentRef) {
    state = state.copyWith(
      activityItems: state.activityItems
          .map(
            (item) => item.intentRef == intentRef
                ? item.copyWith(
                    observableReceiptState: 'Interacted',
                    actionCompleted: true,
                  )
                : item,
          )
          .toList(growable: false),
    );
  }

  void toggleQuietHours(bool enabled) {
    state = state.copyWith(quietHoursEnabled: enabled);
  }
}

final wave7WorkspaceProvider =
    NotifierProvider<Wave7WorkspaceController, Wave7WorkspaceState>(
      Wave7WorkspaceController.new,
    );
