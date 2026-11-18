import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';

import '../state/client_controller.dart';
import 'preview_fixtures.dart';
import 'shell_page.dart';
import 'theme/instrument_theme.dart';
import 'widgets/host_connection_dialog.dart';

const String _envHostUrl = String.fromEnvironment('LIFTASSISTANT_HOST_URL');
const bool _envAutoConnect = bool.fromEnvironment(
  'LIFTASSISTANT_AUTO_CONNECT',
  defaultValue: true,
);
const String _envPreviewFixture = String.fromEnvironment(
  'LIFTASSISTANT_PREVIEW_FIXTURE',
);

class LiftAssistantClientApp extends StatefulWidget {
  const LiftAssistantClientApp({
    super.key,
    this.controller,
    this.initialHostUrl,
    this.autoConnect = false,
  });

  final ClientController? controller;
  final String? initialHostUrl;
  final bool autoConnect;

  @override
  State<LiftAssistantClientApp> createState() => _LiftAssistantClientAppState();
}

class _LiftAssistantClientAppState extends State<LiftAssistantClientApp> {
  late final ClientController _controller;
  late final bool _ownsController;
  ThemeMode _themeMode = ThemeMode.dark;

  @override
  void initState() {
    super.initState();
    if (kIsWeb && Uri.base.queryParameters['theme'] == 'light') {
      _themeMode = ThemeMode.light;
    }
    final String? previewMode = _resolvePreviewMode();
    final (String resolvedUrl, bool shouldAutoConnect) = _resolveStartupTarget(
      hasPreviewFixture: previewMode != null,
    );
    if (widget.controller != null) {
      _controller = widget.controller!;
      _ownsController = false;
    } else if (previewMode != null) {
      _controller = ClientController(
        transport: PreviewFixtureTransport(
          simulateReconnectingAfterInitialSnapshot:
              previewMode == 'reconnecting',
        ),
        initialUrlInput: resolvedUrl,
        retryBackoff: (_) => const Duration(seconds: 30),
      );
      _ownsController = true;
    } else {
      _controller = ClientController(initialUrlInput: resolvedUrl);
      _ownsController = true;
    }
    if (shouldAutoConnect && resolvedUrl.trim().isNotEmpty) {
      unawaited(_controller.connectToUrl(resolvedUrl));
    }
  }

  String? _resolvePreviewMode() {
    if (widget.controller != null) {
      return null;
    }
    if (_envPreviewFixture == 'stress' ||
        _envPreviewFixture == 'reconnecting') {
      return _envPreviewFixture;
    }
    if (kIsWeb) {
      final String? qPreview = Uri.base.queryParameters['preview'];
      if (qPreview == 'stress' || qPreview == 'reconnecting') {
        return qPreview;
      }
    }
    return null;
  }

  (String, bool) _resolveStartupTarget({bool hasPreviewFixture = false}) {
    if (widget.initialHostUrl != null && widget.initialHostUrl!.isNotEmpty) {
      return (widget.initialHostUrl!, widget.autoConnect);
    }
    if (_envHostUrl.isNotEmpty) {
      return (_envHostUrl, _envAutoConnect);
    }
    if (kIsWeb) {
      final String? queryHost = Uri.base.queryParameters['host'];
      if (queryHost != null && queryHost.trim().isNotEmpty) {
        return (queryHost.trim(), true);
      }
    }
    if (hasPreviewFixture) {
      return (defaultSuggestedHostUrl(), true);
    }
    return (defaultSuggestedHostUrl(), widget.autoConnect);
  }

  @override
  void dispose() {
    if (_ownsController) {
      _controller.dispose();
    }
    super.dispose();
  }

  void _toggleTheme() {
    setState(() {
      _themeMode = _themeMode == ThemeMode.dark
          ? ThemeMode.light
          : ThemeMode.dark;
    });
  }

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'LiftAssistant Client',
      debugShowCheckedModeBanner: false,
      theme: buildInstrumentTheme(Brightness.light),
      darkTheme: buildInstrumentTheme(Brightness.dark),
      themeMode: _themeMode,
      home: ClientShellPage(
        controller: _controller,
        themeMode: _themeMode,
        onToggleThemeMode: _toggleTheme,
      ),
    );
  }
}
