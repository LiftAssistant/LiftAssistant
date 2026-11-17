import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';

import '../../api/host_url.dart';
import '../theme/instrument_theme.dart';

@immutable
class HostPreset {
  const HostPreset({
    required this.label,
    required this.url,
    required this.hint,
  });

  final String label;
  final String url;
  final String hint;
}

List<HostPreset> buildPlatformHostPresets() {
  if (kIsWeb) {
    final List<HostPreset> webPresets = <HostPreset>[];
    final Uri pageUri = Uri.base;
    if (pageUri.scheme == 'http' && pageUri.host.isNotEmpty) {
      webPresets.add(
        HostPreset(
          label: 'Same Origin (${pageUri.origin})',
          url: pageUri.origin,
          hint: 'Recommended when served via same-origin reverse proxy',
        ),
      );
    }
    webPresets.add(
      const HostPreset(
        label: 'Local Loopback (127.0.0.1:8080)',
        url: 'http://127.0.0.1:8080',
        hint: 'Direct loopback (requires same origin on browsers)',
      ),
    );
    return webPresets;
  }

  if (defaultTargetPlatform == TargetPlatform.android) {
    return const <HostPreset>[
      HostPreset(
        label: 'Android Emulator Host (10.0.2.2:8080)',
        url: 'http://10.0.2.2:8080',
        hint: 'Connects from Android Emulator to development machine loopback',
      ),
      HostPreset(
        label: 'Loopback / ADB Reverse (127.0.0.1:8080)',
        url: 'http://127.0.0.1:8080',
        hint: 'Use with `adb reverse tcp:8080 tcp:8080` or on-device Host',
      ),
    ];
  }

  return const <HostPreset>[
    HostPreset(
      label: 'Local Loopback (127.0.0.1:8080)',
      url: 'http://127.0.0.1:8080',
      hint: 'Default explicit loopback listener',
    ),
    HostPreset(
      label: 'Localhost (localhost:8080)',
      url: 'http://localhost:8080',
      hint: 'Local hostname listener',
    ),
  ];
}

String defaultSuggestedHostUrl() {
  final List<HostPreset> presets = buildPlatformHostPresets();
  return presets.first.url;
}

class HostConnectionForm extends StatefulWidget {
  const HostConnectionForm({
    super.key,
    required this.initialUrl,
    required this.onConnect,
    this.onDisconnect,
    this.isConnectedOrConnecting = false,
    this.compact = false,
  });

  final String initialUrl;
  final Future<void> Function(String url) onConnect;
  final Future<void> Function()? onDisconnect;
  final bool isConnectedOrConnecting;
  final bool compact;

  @override
  State<HostConnectionForm> createState() => _HostConnectionFormState();
}

class _HostConnectionFormState extends State<HostConnectionForm> {
  late final TextEditingController _controller;
  String? _validationError;

  @override
  void initState() {
    super.initState();
    _controller = TextEditingController(
      text: widget.initialUrl.isEmpty
          ? defaultSuggestedHostUrl()
          : widget.initialUrl,
    );
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  Future<void> _submit() async {
    final String raw = _controller.text.trim();
    try {
      final HostEndpoint parsed = HostEndpoint.parse(raw);
      setState(() {
        _validationError = null;
      });
      await widget.onConnect(parsed.origin);
    } on InvalidHostUrlException catch (error) {
      setState(() {
        _validationError = error.message;
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    final List<HostPreset> presets = buildPlatformHostPresets();

    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.start,
      children: <Widget>[
        const Text(
          'Connect to LiftAssistant Host',
          style: TextStyle(fontSize: 17, fontWeight: FontWeight.w700),
        ),
        const SizedBox(height: 4),
        Text(
          'Consumes the experimental/v0 read-only HTTP snapshot and SSE stream. '
          'Plain http:// only (no built-in TLS, authentication, or write capability).',
          style: TextStyle(
            fontSize: 12.5,
            height: 1.4,
            color: palette.subtleText,
          ),
        ),
        const SizedBox(height: 14),
        TextField(
          key: const Key('host-url-input'),
          controller: _controller,
          keyboardType: TextInputType.url,
          autocorrect: false,
          style: instrumentMonoStyle(context, fontSize: 13.5),
          decoration: InputDecoration(
            labelText: 'Host Base URL',
            hintText: 'http://127.0.0.1:8080',
            errorText: _validationError,
            errorMaxLines: 3,
            prefixIcon: const Icon(Icons.dns_outlined, size: 18),
          ),
          onSubmitted: (_) => _submit(),
        ),
        const SizedBox(height: 10),
        Wrap(
          spacing: 8,
          runSpacing: 8,
          children: <Widget>[
            for (final HostPreset preset in presets)
              ActionChip(
                avatar: const Icon(Icons.bolt_outlined, size: 15),
                label: Text(
                  preset.label,
                  style: const TextStyle(fontSize: 12),
                ),
                tooltip: preset.hint,
                onPressed: () {
                  setState(() {
                    _controller.text = preset.url;
                    _validationError = null;
                  });
                },
              ),
          ],
        ),
        if (kIsWeb) ...<Widget>[
          const SizedBox(height: 12),
          Container(
            padding: const EdgeInsets.all(10),
            decoration: BoxDecoration(
              color: palette.infoContainer,
              borderRadius: BorderRadius.circular(8),
              border: Border.all(color: palette.info.withValues(alpha: 0.35)),
            ),
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: <Widget>[
                Icon(Icons.public, size: 16, color: palette.info),
                const SizedBox(width: 8),
                Expanded(
                  child: Text(
                    'Web deployment note: LiftAssistant Host intentionally omits wildcard CORS '
                    'headers on unauthenticated HTTP. Serve the Web client on the same origin '
                    '(or behind a controlled reverse proxy) so browser Same-Origin Policy permits '
                    'snapshot and SSE requests.',
                    style: TextStyle(
                      fontSize: 12,
                      height: 1.35,
                      color: Theme.of(context).colorScheme.onSurface,
                    ),
                  ),
                ),
              ],
            ),
          ),
        ],
        const SizedBox(height: 16),
        Wrap(
          spacing: 10,
          runSpacing: 10,
          children: <Widget>[
            FilledButton.icon(
              key: const Key('connect-host-button'),
              onPressed: _submit,
              icon: const Icon(Icons.link, size: 18),
              label: const Text('Connect Read-Only'),
            ),
            if (widget.isConnectedOrConnecting && widget.onDisconnect != null)
              OutlinedButton.icon(
                key: const Key('disconnect-host-button'),
                onPressed: widget.onDisconnect,
                icon: const Icon(Icons.link_off, size: 18),
                label: const Text('Disconnect'),
              ),
          ],
        ),
      ],
    );
  }
}

Future<void> showHostConnectionModal({
  required BuildContext context,
  required String currentUrl,
  required bool isConnectedOrConnecting,
  required Future<void> Function(String url) onConnect,
  required Future<void> Function() onDisconnect,
}) async {
  final bool isCompact = MediaQuery.sizeOf(context).width < 640;
  if (isCompact) {
    await showModalBottomSheet<void>(
      context: context,
      isScrollControlled: true,
      useSafeArea: true,
      backgroundColor: context.palette.surface,
      shape: const RoundedRectangleBorder(
        borderRadius: BorderRadius.vertical(top: Radius.circular(16)),
      ),
      builder: (BuildContext sheetContext) {
        return Padding(
          padding: EdgeInsets.fromLTRB(
            18,
            18,
            18,
            18 + MediaQuery.viewInsetsOf(sheetContext).bottom,
          ),
          child: HostConnectionForm(
            initialUrl: currentUrl,
            isConnectedOrConnecting: isConnectedOrConnecting,
            compact: true,
            onConnect: (String url) async {
              Navigator.of(sheetContext).pop();
              await onConnect(url);
            },
            onDisconnect: () async {
              Navigator.of(sheetContext).pop();
              await onDisconnect();
            },
          ),
        );
      },
    );
    return;
  }

  await showDialog<void>(
    context: context,
    builder: (BuildContext dialogContext) {
      return Dialog(
        backgroundColor: dialogContext.palette.surface,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(12),
          side: BorderSide(color: dialogContext.palette.border),
        ),
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 540),
          child: Padding(
            padding: const EdgeInsets.all(22),
            child: HostConnectionForm(
              initialUrl: currentUrl,
              isConnectedOrConnecting: isConnectedOrConnecting,
              onConnect: (String url) async {
                Navigator.of(dialogContext).pop();
                await onConnect(url);
              },
              onDisconnect: () async {
                Navigator.of(dialogContext).pop();
                await onDisconnect();
              },
            ),
          ),
        ),
      );
    },
  );
}
