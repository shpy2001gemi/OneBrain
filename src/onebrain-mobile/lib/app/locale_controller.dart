import 'dart:async';

import 'package:flutter/widgets.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:shared_preferences/shared_preferences.dart';

const _localePreferenceKey = 'onebrain.ui.locale.v1';

abstract interface class LocalePreferenceStore {
  Future<String?> readLanguageCode();

  Future<void> writeLanguageCode(String languageCode);
}

class SharedLocalePreferenceStore implements LocalePreferenceStore {
  SharedLocalePreferenceStore({SharedPreferencesAsync? preferences})
    : _preferences = preferences;

  SharedPreferencesAsync? _preferences;

  SharedPreferencesAsync get _client =>
      _preferences ??= SharedPreferencesAsync();

  @override
  Future<String?> readLanguageCode() => _client.getString(_localePreferenceKey);

  @override
  Future<void> writeLanguageCode(String languageCode) =>
      _client.setString(_localePreferenceKey, languageCode);
}

final localePreferenceStoreProvider = Provider<LocalePreferenceStore>(
  (ref) => SharedLocalePreferenceStore(),
);

final localeControllerProvider = NotifierProvider<LocaleController, Locale?>(
  LocaleController.new,
);

class LocaleController extends Notifier<Locale?> {
  @override
  Locale? build() {
    unawaited(_restore(ref.watch(localePreferenceStoreProvider)));
    return null;
  }

  void select(Locale locale) {
    state = locale;
    unawaited(_persist(locale));
  }

  Future<void> _restore(LocalePreferenceStore store) async {
    try {
      final languageCode = await store.readLanguageCode();
      final supportedLanguage = switch (languageCode) {
        'en' => 'en',
        'vi' => 'vi',
        _ => null,
      };
      if (state == null && supportedLanguage != null) {
        state = Locale(supportedLanguage);
      }
    } on Object {
      // A missing/unavailable preference backend leaves platform locale active.
    }
  }

  Future<void> _persist(Locale locale) async {
    try {
      await ref
          .read(localePreferenceStoreProvider)
          .writeLanguageCode(locale.languageCode);
    } on Object {
      // Locale choice is non-authoritative UI preference; keep the live state.
    }
  }
}
