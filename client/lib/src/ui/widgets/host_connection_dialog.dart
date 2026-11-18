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
        Row(
          children: <Widget>[
            Container(
              width: 34,
              height: 34,
              decoration: BoxDecoration(
                color: palette.infoContainer,
                borderRadius: BorderRadius.circular(10),
              ),
              child: Icon(Icons.dns_outlined, size: 18, color: palette.info),
            ),
            const SizedBox(width: 12),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: <Widget>[
                  const Text(
                    'Host Endpoint',
                    style: TextStyle(
                      fontSize: 16,
                      fontWeight: FontWeight.w700,
                      letterSpacing: -0.2,
                    ),
                  ),
                  const SizedBox(height: 2),
                  Text(
                    'Read-only experimental/v0 snapshot + SSE stream (plain http:// only).',
                    style: TextStyle(fontSize: 12.5, color: palette.subtleText),
                  ),
                ],
              ),
            ),
          ],
        ),
        const SizedBox(height: 16),
        TextField(
          key: const Key('host-url-input'),
          controller: _controller,
          keyboardType: TextInputType.url,
          autocorrect: false,
          style: instrumentTabularStyle(context, fontSize: 13.5),
          decoration: InputDecoration(
            hintText: 'http://127.0.0.1:8080',
            prefixIcon: Icon(
              Icons.link_rounded,
              size: 18,
              color: palette.mutedText,
            ),
            errorText: _validationError,
            errorMaxLines: 3,
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
                tooltip: preset.hint,
                visualDensity: VisualDensity.compact,
                label: Text(
                  preset.label,
                  style: instrumentTabularStyle(
                    context,
                    fontSize: 11.5,
                    fontWeight: FontWeight.w600,
                    color: palette.subtleText,
                  ),
                ),
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
                  style: instrumentTabularStyle(
                    context,
                    fontSize: 11.5,
                    fontWeight: FontWeight.w600,
                    color: palette.subtleText,
                  ),
                ),
                const TextSpan(text: ' (Host omits wildcard CORS).'),
              ],
            ),
          ),
        ],
        const SizedBox(height: 16),
        Wrap(
          spacing: 8,
          runSpacing: 8,
          children: <Widget>[
            FilledButton.icon(
              key: const Key('connect-host-button'),
              onPressed: _submit,
              icon: const Icon(Icons.bolt_rounded, size: 16),
              label: const Text('Connect'),
            ),
            if (widget.isConnectedOrConnecting && widget.onDisconnect != null)
              OutlinedButton.icon(
                key: const Key('disconnect-host-button'),
                onPressed: widget.onDisconnect,
                icon: const Icon(Icons.link_off_rounded, size: 16),
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
  final bool isCompact = MediaQuery.sizeOf(context).width < 600;
  if (isCompact) {
    await showModalBottomSheet<void>(
      context: context,
      isScrollControlled: true,
      useSafeArea: true,
      clipBehavior: Clip.antiAlias,
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
        clipBehavior: Clip.antiAlias,
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 460),
          child: SingleChildScrollView(
            padding: const EdgeInsets.all(20),
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
      return Dialog(
        clipBehavior: Clip.antiAlias,
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 520),
          child: SingleChildScrollView(
            padding: const EdgeInsets.all(20),
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
                          fontSize: 15,
                          fontWeight: FontWeight.w700,
                        ),
                      ),
                    ),
                    IconButton(
                      visualDensity: VisualDensity.compact,
                      onPressed: () => Navigator.of(dialogContext).pop(),
                      icon: const Icon(Icons.close_rounded, size: 18),
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
