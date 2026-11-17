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
          label: 'Same origin (${pageUri.origin})',
          url: pageUri.origin,
          hint: 'Recommended when served via same-origin reverse proxy',
        ),
      );
    }
    webPresets.add(
      const HostPreset(
        label: '127.0.0.1:8080',
        url: 'http://127.0.0.1:8080',
        hint: 'Local loopback (requires same origin in browsers)',
      ),
    );
    return webPresets;
  }

  if (defaultTargetPlatform == TargetPlatform.android) {
    return const <HostPreset>[
      HostPreset(
        label: '10.0.2.2:8080 (emulator)',
        url: 'http://10.0.2.2:8080',
        hint: 'Android Emulator bridge to host loopback',
      ),
      HostPreset(
        label: '127.0.0.1:8080 (adb reverse)',
        url: 'http://127.0.0.1:8080',
        hint: 'Use with `adb reverse tcp:8080 tcp:8080` or on-device Host',
      ),
    ];
  }

  return const <HostPreset>[
    HostPreset(
      label: '127.0.0.1:8080',
      url: 'http://127.0.0.1:8080',
      hint: 'Default loopback listener',
    ),
    HostPreset(
      label: 'localhost:8080',
      url: 'http://localhost:8080',
      hint: 'Localhost listener',
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
  void didUpdateWidget(covariant HostConnectionForm oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.initialUrl != oldWidget.initialUrl &&
        widget.initialUrl.isNotEmpty &&
        _controller.text != widget.initialUrl) {
      _controller.value = TextEditingValue(
        text: widget.initialUrl,
        selection: TextSelection.collapsed(offset: widget.initialUrl.length),
      );
      _validationError = null;
    }
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
          'Host Endpoint',
          style: TextStyle(
            fontSize: 15,
            fontWeight: FontWeight.w700,
            letterSpacing: -0.2,
          ),
        ),
        const SizedBox(height: 3),
        Text(
          'Read-only experimental/v0 snapshot + SSE stream (plain http:// only).',
          style: TextStyle(fontSize: 12, color: palette.subtleText),
        ),
        const SizedBox(height: 12),
        TextField(
          key: const Key('host-url-input'),
          controller: _controller,
          keyboardType: TextInputType.url,
          autocorrect: false,
          style: instrumentMonoStyle(context, fontSize: 13),
          decoration: InputDecoration(
            hintText: 'http://127.0.0.1:8080',
            errorText: _validationError,
            errorMaxLines: 3,
          ),
          onSubmitted: (_) => _submit(),
        ),
        const SizedBox(height: 8),
        Wrap(
          spacing: 6,
          runSpacing: 6,
          children: <Widget>[
            for (final HostPreset preset in presets)
              Tooltip(
                message: preset.hint,
                child: InkWell(
                  borderRadius: BorderRadius.circular(4),
                  onTap: () {
                    setState(() {
                      _controller.text = preset.url;
                      _validationError = null;
                    });
                  },
                  child: Container(
                    padding: const EdgeInsets.symmetric(
                      horizontal: 8,
                      vertical: 4,
                    ),
                    decoration: BoxDecoration(
                      color: palette.elevatedSurface,
                      borderRadius: BorderRadius.circular(4),
                      border: Border.all(color: palette.border),
                    ),
                    child: Text(
                      preset.label,
                      style: instrumentMonoStyle(
                        context,
                        fontSize: 11,
                        fontWeight: FontWeight.w500,
                        color: palette.subtleText,
                      ),
                    ),
                  ),
                ),
              ),
          ],
        ),
        if (kIsWeb) ...<Widget>[
          const SizedBox(height: 10),
          Text.rich(
            TextSpan(
              style: TextStyle(
                fontSize: 11.5,
                height: 1.4,
                color: palette.mutedText,
              ),
              children: <InlineSpan>[
                const TextSpan(
                  text: 'Browser Same-Origin: serve from the Host origin or reverse-proxy ',
                ),
                TextSpan(
                  text: '/\u2060api\u2060/\u2060experimental\u2060/\u2060v0',
                  style: instrumentMonoStyle(
                    context,
                    fontSize: 11,
                    fontWeight: FontWeight.w500,
                    color: palette.subtleText,
                  ),
                ),
                const TextSpan(text: ' (Host omits wildcard CORS).'),
              ],
            ),
          ),
        ],
        const SizedBox(height: 14),
        Wrap(
          spacing: 8,
          runSpacing: 8,
          children: <Widget>[
            FilledButton(
              key: const Key('connect-host-button'),
              onPressed: _submit,
              child: const Text('Connect'),
            ),
            if (widget.isConnectedOrConnecting && widget.onDisconnect != null)
              OutlinedButton(
                key: const Key('disconnect-host-button'),
                onPressed: widget.onDisconnect,
                child: const Text('Disconnect'),
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
  final bool isCompact = MediaQuery.sizeOf(context).width < 600;
  if (isCompact) {
    await showModalBottomSheet<void>(
      context: context,
      isScrollControlled: true,
      useSafeArea: true,
      backgroundColor: context.palette.surface,
      shape: const RoundedRectangleBorder(
        borderRadius: BorderRadius.vertical(top: Radius.circular(10)),
      ),
      builder: (BuildContext sheetContext) {
        return SingleChildScrollView(
          padding: EdgeInsets.fromLTRB(
            16,
            16,
            16,
            16 + MediaQuery.viewInsetsOf(sheetContext).bottom,
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
          borderRadius: BorderRadius.circular(8),
          side: BorderSide(color: dialogContext.palette.border),
        ),
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 460),
          child: SingleChildScrollView(
            padding: const EdgeInsets.all(18),
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

Future<void> showSemanticsReferenceModal(BuildContext context) async {
  await showDialog<void>(
    context: context,
    builder: (BuildContext dialogContext) {
      final InstrumentPalette palette = dialogContext.palette;
      return Dialog(
        backgroundColor: palette.surface,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(8),
          side: BorderSide(color: palette.border),
        ),
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 520),
          child: SingleChildScrollView(
            padding: const EdgeInsets.all(18),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              mainAxisSize: MainAxisSize.min,
              children: <Widget>[
                Row(
                  children: <Widget>[
                    const Expanded(
                      child: Text(
                        'experimental/v0 Semantic Reference',
                        style: TextStyle(
                          fontSize: 14.5,
                          fontWeight: FontWeight.w700,
                        ),
                      ),
                    ),
                    IconButton(
                      visualDensity: VisualDensity.compact,
                      onPressed: () => Navigator.of(dialogContext).pop(),
                      icon: const Icon(Icons.close, size: 17),
                    ),
                  ],
                ),
                const SizedBox(height: 8),
                const _RefRow(
                  term: 'Read-only boundary',
                  detail: 'Consumes complete snapshots and SSE updates only; exposes zero call, target, door, mode, or controller write actions.',
                ),
                const _RefRow(
                  term: 'Floor ref vs Continuous',
                  detail: 'Floor ref (kind: floor) indicates discrete floor association only. Continuous (kind: continuous) indicates genuine measured coordinate evidence.',
                ),
                const _RefRow(
                  term: 'Source dest',
                  detail: 'External source reported destination evidence (known / none / unknown) — never a LiftAssistant desired target, call assignment, or command.',
                ),
                const _RefRow(
                  term: 'Operational vs Auxiliary',
                  detail: 'Only Operational bindings enter operational_cars and readiness. Auxiliary bindings are inspectable/diagnostic only and never fail over automatically.',
                ),
                const _RefRow(
                  term: 'Separate health layers',
                  detail: 'Client link, Host lifecycle, Host readiness, source session, transport discovery, payload activity, and per-car availability are independent.',
                ),
                const _RefRow(
                  term: 'Host diagnostics & revision',
                  detail: 'Diagnostics are machine-readable Host evidence, not confirmed equipment faults or repair advice. Revision is monotonic per Host lifetime and resets on Host restart.',
                ),
              ],
            ),
          ),
        ),
      );
    },
  );
}

class _RefRow extends StatelessWidget {
  const _RefRow({required this.term, required this.detail});

  final String term;
  final String detail;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    return Padding(
      padding: const EdgeInsets.only(bottom: 10),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: <Widget>[
          Text(
            term,
            style: const TextStyle(fontSize: 12, fontWeight: FontWeight.w700),
          ),
          const SizedBox(height: 2),
          Text(
            detail,
            style: TextStyle(
              fontSize: 11.5,
              height: 1.35,
              color: palette.subtleText,
            ),
          ),
        ],
      ),
    );
  }
}
