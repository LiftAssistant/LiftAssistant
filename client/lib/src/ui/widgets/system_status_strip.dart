import 'package:flutter/material.dart';

import '../../api/read_model.dart';
import '../../state/client_state.dart';
import '../theme/instrument_theme.dart';
import 'semantic_badge.dart';

(String, Color, Color, IconData) describeConnectionPhase(
  ConnectionPhase phase,
  InstrumentPalette palette,
) {
  switch (phase) {
    case ConnectionPhase.live:
      return (
        'LIVE SSE',
        palette.ok,
        palette.okContainer,
        Icons.sensors,
      );
    case ConnectionPhase.attachingStream:
      return (
        'ATTACHING SSE',
        palette.info,
        palette.infoContainer,
        Icons.sync,
      );
    case ConnectionPhase.connecting:
      return (
        'CONNECTING',
        palette.info,
        palette.infoContainer,
        Icons.downloading_outlined,
      );
    case ConnectionPhase.reconnecting:
      return (
        'RECONNECTING',
        palette.caution,
        palette.cautionContainer,
        Icons.wifi_find_outlined,
      );
    case ConnectionPhase.incompatibleApi:
      return (
        'UNSUPPORTED API',
        palette.danger,
        palette.dangerContainer,
        Icons.block_outlined,
      );
    case ConnectionPhase.protocolError:
      return (
        'PROTOCOL ERROR',
        palette.danger,
        palette.dangerContainer,
        Icons.warning_amber_rounded,
      );
    case ConnectionPhase.disconnected:
      return (
        'DISCONNECTED',
        palette.neutral,
        palette.neutralContainer,
        Icons.link_off,
      );
  }
}

(String, String, Color, Color) describeHostLifecycle(
  HostLifecycle lifecycle,
  InstrumentPalette palette,
) {
  switch (lifecycle) {
    case HostLifecycle.starting:
      return (
        'Starting',
        'Process started; awaiting initial observation activity',
        palette.info,
        palette.infoContainer,
      );
    case HostLifecycle.synchronizing:
      return (
        'Synchronizing',
        'Ingesting observations; operational state not yet complete',
        palette.caution,
        palette.cautionContainer,
      );
    case HostLifecycle.readyReadOnly:
      return (
        'ReadyReadOnly',
        'Complete operational state established for every Operational car',
        palette.ok,
        palette.okContainer,
      );
    case HostLifecycle.degraded:
      return (
        'Degraded',
        'Operational invariant violation or rejection revoked readiness',
        palette.danger,
        palette.dangerContainer,
      );
  }
}

(String, String, Color, Color) describeHostReadiness(
  HostReadiness readiness,
  InstrumentPalette palette,
) {
  switch (readiness) {
    case HostReadiness.readOnly:
      return (
        'ReadOnly',
        'Read-side operational projection is synchronized',
        palette.ok,
        palette.okContainer,
      );
    case HostReadiness.notReady:
      return (
        'NotReady',
        'Process reachable does NOT equal operational data ready',
        palette.caution,
        palette.cautionContainer,
      );
  }
}

class SystemStatusStrip extends StatelessWidget {
  const SystemStatusStrip({
    super.key,
    required this.state,
    required this.onReconnectNow,
    required this.onConfigureHost,
  });

  final ClientState state;
  final VoidCallback onReconnectNow;
  final VoidCallback onConfigureHost;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    final ReadModelSnapshot? snapshot = state.snapshot;
    final ClientConnectionError? error = state.error;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: <Widget>[
        if (state.hasRetainedStaleSnapshot) ...<Widget>[
          SemanticCalloutBanner(
            key: const Key('stale-retained-snapshot-banner'),
            icon: Icons.cloud_off_outlined,
            title:
                'CLIENT CONNECTION LOST — RETAINED SNAPSHOT IS NOT LIVE',
            message:
                'Displaying last complete snapshot received at Host revision #${snapshot!.revision} '
                'before connection stopped (${error?.message ?? state.phase.name}). '
                'Retained values are historical client context, NOT live operational truth.',
            accent: palette.caution,
            background: palette.cautionContainer,
            actions: <Widget>[
              FilledButton.tonalIcon(
                key: const Key('banner-reconnect-button'),
                onPressed: onReconnectNow,
                icon: const Icon(Icons.refresh, size: 16),
                label: const Text('Reconnect Now'),
              ),
              OutlinedButton.icon(
                onPressed: onConfigureHost,
                icon: const Icon(Icons.edit_outlined, size: 16),
                label: const Text('Change Host'),
              ),
            ],
          ),
          const SizedBox(height: 12),
        ] else if (error != null) ...<Widget>[
          _buildErrorBanner(context, error, palette),
          const SizedBox(height: 12),
        ],
        if (snapshot != null)
          Opacity(
            opacity: state.isSnapshotLive ? 1.0 : 0.78,
            child: _HostTruthMetricsGrid(
              snapshot: snapshot,
              isLive: state.isSnapshotLive,
              revisionReset: state.lastRevisionReset,
            ),
          ),
      ],
    );
  }

  Widget _buildErrorBanner(
    BuildContext context,
    ClientConnectionError error,
    InstrumentPalette palette,
  ) {
    final bool isHardError =
        state.phase == ConnectionPhase.incompatibleApi ||
        state.phase == ConnectionPhase.protocolError ||
        error.kind == ClientErrorKind.invalidUrl;

    final String title;
    switch (error.kind) {
      case ClientErrorKind.unsupportedApiVersion:
        title =
            'UNSUPPORTED HOST API VERSION (${error.unsupportedVersion ?? "unknown"})';
      case ClientErrorKind.malformedResponse:
        title = 'MALFORMED HOST RESPONSE';
      case ClientErrorKind.unexpectedContentType:
        title = 'UNEXPECTED ENDPOINT CONTENT TYPE';
      case ClientErrorKind.httpStatusError:
        title = 'HOST HTTP ERROR (${error.httpStatusCode ?? ""})';
      case ClientErrorKind.crossOriginBlockedOrUnreachable:
        title = 'HOST UNREACHABLE OR BROWSER CROSS-ORIGIN BLOCKED';
      case ClientErrorKind.invalidUrl:
        title = 'INVALID HOST BASE URL';
      case ClientErrorKind.streamDisconnected:
      case ClientErrorKind.unreachable:
        title = state.phase == ConnectionPhase.reconnecting
            ? 'HOST UNREACHABLE — AUTOMATICALLY RECONNECTING (ATTEMPT #${state.reconnectAttempt})'
            : 'CONNECTION ERROR';
    }

    return SemanticCalloutBanner(
      key: const Key('connection-error-banner'),
      icon: isHardError
          ? Icons.error_outline_rounded
          : Icons.wifi_tethering_error_rounded_outlined,
      title: title,
      message: error.message,
      accent: isHardError ? palette.danger : palette.caution,
      background:
          isHardError ? palette.dangerContainer : palette.cautionContainer,
      actions: <Widget>[
        if (state.endpoint != null)
          FilledButton.tonalIcon(
            key: const Key('error-retry-button'),
            onPressed: onReconnectNow,
            icon: const Icon(Icons.refresh, size: 16),
            label: const Text('Retry Now'),
          ),
        OutlinedButton.icon(
          key: const Key('error-configure-host-button'),
          onPressed: onConfigureHost,
          icon: const Icon(Icons.settings_ethernet, size: 16),
          label: const Text('Configure Host'),
        ),
      ],
    );
  }
}

class _HostTruthMetricsGrid extends StatelessWidget {
  const _HostTruthMetricsGrid({
    required this.snapshot,
    required this.isLive,
    required this.revisionReset,
  });

  final ReadModelSnapshot snapshot;
  final bool isLive;
  final RevisionResetNotice? revisionReset;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    final (
      String lifecycleLabel,
      String lifecycleNote,
      Color lifecycleFg,
      Color lifecycleBg,
    ) = describeHostLifecycle(snapshot.lifecycle, palette);
    final (
      String readinessLabel,
      String readinessNote,
      Color readinessFg,
      Color readinessBg,
    ) = describeHostReadiness(snapshot.readiness, palette);

    return LayoutBuilder(
      builder: (BuildContext context, BoxConstraints constraints) {
        final double width = constraints.maxWidth;
        final int columns = width >= 980 ? 4 : (width >= 560 ? 2 : 1);
        const double spacing = 10;
        final double tileWidth =
            (width - spacing * (columns - 1)) / columns;

        return Wrap(
          spacing: spacing,
          runSpacing: spacing,
          children: <Widget>[
            SizedBox(
              width: tileWidth,
              child: _MetricTile(
                label: 'HOST LIFECYCLE',
                value: lifecycleLabel,
                wireTag: snapshot.lifecycle.wireValue,
                note: lifecycleNote,
                accent: lifecycleFg,
                accentBg: lifecycleBg,
              ),
            ),
            SizedBox(
              width: tileWidth,
              child: _MetricTile(
                label: 'READ READINESS',
                value: readinessLabel,
                wireTag: snapshot.readiness.wireValue,
                note: readinessNote,
                accent: readinessFg,
                accentBg: readinessBg,
              ),
            ),
            SizedBox(
              width: tileWidth,
              child: _MetricTile(
                label: 'HOST REVISION',
                value: '#${snapshot.revision}',
                wireTag: isLive ? 'live-snapshot' : 'retained-non-live',
                note: revisionReset != null
                    ? 'New Host lifetime accepted (#${revisionReset!.previousRevision} → #${revisionReset!.newRevision})'
                    : 'Monotonic within current Host process lifetime only',
                accent: revisionReset != null ? palette.info : palette.neutral,
                accentBg: revisionReset != null
                    ? palette.infoContainer
                    : palette.neutralContainer,
                monospaceValue: true,
              ),
            ),
            SizedBox(
              width: tileWidth,
              child: _MetricTile(
                label: 'BOUNDARY CONTRACT',
                value: snapshot.apiVersion,
                wireTag: 'READ-ONLY',
                note:
                    '${snapshot.operationalCars.length} operational car(s) • ${snapshot.sources.length} source(s) • ${snapshot.recentDiagnostics.length} diagnostic(s)',
                accent: palette.info,
                accentBg: palette.infoContainer,
                monospaceValue: true,
              ),
            ),
          ],
        );
      },
    );
  }
}

class _MetricTile extends StatelessWidget {
  const _MetricTile({
    required this.label,
    required this.value,
    required this.wireTag,
    required this.note,
    required this.accent,
    required this.accentBg,
    this.monospaceValue = false,
  });

  final String label;
  final String value;
  final String wireTag;
  final String note;
  final Color accent;
  final Color accentBg;
  final bool monospaceValue;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    return Container(
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: palette.surface,
        borderRadius: BorderRadius.circular(10),
        border: Border.all(color: palette.border),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: <Widget>[
          Row(
            children: <Widget>[
              Expanded(
                child: Text(
                  label,
                  style: TextStyle(
                    fontSize: 11,
                    fontWeight: FontWeight.w700,
                    letterSpacing: 0.7,
                    color: palette.subtleText,
                  ),
                ),
              ),
              SemanticBadge(
                label: wireTag,
                foreground: accent,
                background: accentBg,
                monospace: true,
              ),
            ],
          ),
          const SizedBox(height: 8),
          Text(
            value,
            style: monospaceValue
                ? instrumentMonoStyle(
                    context,
                    fontSize: 18,
                    fontWeight: FontWeight.w700,
                  )
                : const TextStyle(
                    fontSize: 18,
                    fontWeight: FontWeight.w700,
                    letterSpacing: -0.3,
                  ),
          ),
          const SizedBox(height: 4),
          Text(
            note,
            style: TextStyle(
              fontSize: 11.5,
              height: 1.3,
              color: palette.subtleText,
            ),
          ),
        ],
      ),
    );
  }
}
