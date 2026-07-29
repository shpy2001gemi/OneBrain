import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../design/onebrain_theme_extensions.dart';
import '../../l10n/app_localizations.dart';
import '../../platform/mobile_host_gateway.dart';
import '../shared/obm_button.dart';
import '../shared/obm_icon.dart';
import '../shared/obm_scope_banner.dart';
import '../shared/obm_screen_frame.dart';

class TextCaptureScreen extends ConsumerStatefulWidget {
  const TextCaptureScreen({super.key});

  @override
  ConsumerState<TextCaptureScreen> createState() => _TextCaptureScreenState();
}

class _TextCaptureScreenState extends ConsumerState<TextCaptureScreen> {
  final _controller = TextEditingController();
  String _language = 'vi';
  bool _saving = false;
  String? _error;
  MobileRawDraftReceipt? _receipt;

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  Future<void> _save() async {
    final strings = AppLocalizations.of(context);
    if (_controller.text.trim().isEmpty) {
      setState(() => _error = strings.draftBlankError);
      return;
    }
    setState(() {
      _saving = true;
      _error = null;
    });
    try {
      final receipt = await ref
          .read(mobileHostGatewayProvider)
          .saveRawTextDraft(
            contentLanguage: _language,
            content: _controller.text,
          );
      ref.invalidate(mobileRuntimeSnapshotProvider);
      if (mounted) {
        setState(() {
          _receipt = receipt;
          _saving = false;
          _controller.clear();
        });
      }
    } on Object {
      if (mounted) {
        setState(() {
          _saving = false;
          _error = strings.draftSaveError;
        });
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final strings = AppLocalizations.of(context);
    return ObmScreenFrame(
      title: strings.textComposerTitle,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          ObmScopeBanner(
            title: strings.statusPrivate,
            body: strings.textComposerBody,
            tone: ObmStatusTone.pausedPrivate,
            icon: ObmSymbol.lock,
          ),
          SizedBox(height: context.spacing.twoXl),
          DropdownButtonFormField<String>(
            initialValue: _language,
            icon: const ObmIcon(ObmSymbol.expandMore, size: ObmIconSize.inline),
            decoration: InputDecoration(
              labelText: strings.contentLanguageLabel,
            ),
            items: [
              DropdownMenuItem(
                value: 'vi',
                child: Text(strings.languageVietnamese),
              ),
              DropdownMenuItem(
                value: 'en',
                child: Text(strings.languageEnglish),
              ),
            ],
            onChanged: _saving
                ? null
                : (value) {
                    if (value != null) {
                      setState(() => _language = value);
                    }
                  },
          ),
          SizedBox(height: context.spacing.lg),
          TextField(
            controller: _controller,
            enabled: !_saving,
            minLines: 8,
            maxLines: 16,
            maxLength: 200000,
            textCapitalization: TextCapitalization.sentences,
            decoration: InputDecoration(
              labelText: strings.draftTextLabel,
              hintText: strings.draftTextHint,
              errorText: _error,
              alignLabelWithHint: true,
            ),
          ),
          SizedBox(height: context.spacing.lg),
          ObmButton(
            label: strings.savePrivateDraftAction,
            onPressed: _saving ? null : _save,
            busy: _saving,
            leadingIcon: ObmSymbol.lock,
          ),
          if (_receipt case final receipt?) ...[
            SizedBox(height: context.spacing.lg),
            ObmScopeBanner(
              title: strings.draftSavedTitle,
              body: strings.draftSavedBody(
                receipt.contentBytes,
                receipt.totalDrafts,
              ),
              tone: ObmStatusTone.ready,
              icon: ObmSymbol.checkCircle,
              statusLabel: strings.statusPrivate,
            ),
          ],
        ],
      ),
    );
  }
}
