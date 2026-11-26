import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../design/onebrain_theme_extensions.dart';
import '../../domain/wave7_workspace_controller.dart';
import '../../l10n/app_localizations.dart';
import '../shared/obm_app_shell.dart';
import '../shared/obm_button.dart';
import '../shared/obm_icon.dart';
import '../shared/obm_scope_banner.dart';
import '../shared/obm_shell_destinations.dart';
import '../shared/obm_wave7_components.dart';

class AssistantScreen extends ConsumerStatefulWidget {
  const AssistantScreen({super.key});

  @override
  ConsumerState<AssistantScreen> createState() => _AssistantScreenState();
}

class _AssistantScreenState extends ConsumerState<AssistantScreen> {
  final _promptController = TextEditingController();

  @override
  void dispose() {
    _promptController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final strings = AppLocalizations.of(context);
    final w7 = ref.watch(wave7WorkspaceProvider);
    return ObmAppShell(
      title: strings.assistantTitle,
      selectedIndex: 3,
      destinations: obmShellDestinations(strings),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              Expanded(
                child: Text(
                  strings.assistantTitle,
                  style: Theme.of(context).textTheme.headlineLarge,
                ),
              ),
              ObmProviderChip(
                providerLabel: w7.activeLlmProvider == 'No LLM'
                    ? strings.wave7AssistantNoLlmChip
                    : w7.activeLlmProvider,
                tone: w7.activeLlmProvider == 'No LLM'
                    ? ObmStatusTone.information
                    : ObmStatusTone.ready,
                onTap: () => context.push('/assistant/provider'),
              ),
            ],
          ),
          SizedBox(height: context.spacing.md),
          ObmScopeBanner(
            title: strings.wave7AssistantLocalHelperLabel,
            body: strings.assistantBody,
            tone: ObmStatusTone.information,
            icon: ObmSymbol.assistant,
          ),
          if (w7.attachedContextRefs.isNotEmpty) ...[
            SizedBox(height: context.spacing.sm),
            Wrap(
              spacing: context.spacing.xs,
              children: [
                for (final item in w7.attachedContextRefs)
                  ObmContextCapsule(
                    refLabel: item,
                    scopeLabel: 'LOCAL_ONLY',
                    onRemove: () => ref
                        .read(wave7WorkspaceProvider.notifier)
                        .removeContextRef(item),
                  ),
              ],
            ),
          ],
          SizedBox(height: context.spacing.md),
          for (final turn in w7.assistantTurns) ...[
            ObmAssistantMessage(
              roleLabel: turn.roleLabel,
              body: turn.body,
              isUser: turn.isUser,
              providerBadge: turn.providerBadge,
            ),
            SizedBox(height: context.spacing.xs),
          ],
          for (final proposal in w7.toolProposals) ...[
            SizedBox(height: context.spacing.sm),
            ObmToolProposalCard(
              proposalId: proposal.proposalId,
              toolName: proposal.toolName,
              argumentsSummary: proposal.argumentsJson,
              stateLabel: proposal.journalState,
              approveLabel: strings.wave7ToolApproveAction,
              denyLabel: strings.wave7ToolDenyAction,
              onApprove: proposal.journalState == 'AwaitingConsent'
                  ? () => ref
                        .read(wave7WorkspaceProvider.notifier)
                        .approveToolProposal(proposal.proposalId)
                  : null,
              onDeny: proposal.journalState == 'AwaitingConsent'
                  ? () => ref
                        .read(wave7WorkspaceProvider.notifier)
                        .denyToolProposal(proposal.proposalId)
                  : null,
            ),
          ],
          SizedBox(height: context.spacing.md),
          TextField(
            key: const Key('assistant_prompt_field'),
            controller: _promptController,
            decoration: const InputDecoration(
              labelText: 'Prompt (e.g., kql: FIND (k:KU) or draft: note)',
            ),
          ),
          SizedBox(height: context.spacing.sm),
          ObmButton(
            key: const Key('assistant_send_button'),
            label: strings.wave7AssistantSendAction,
            onPressed: () {
              ref
                  .read(wave7WorkspaceProvider.notifier)
                  .sendAssistantPrompt(_promptController.text);
              _promptController.clear();
            },
          ),
          SizedBox(height: context.spacing.sm),
          ObmButton(
            label: 'Manage scoped context',
            variant: ObmButtonVariant.outline,
            onPressed: () => context.push('/assistant/context'),
          ),
        ],
      ),
    );
  }
}
