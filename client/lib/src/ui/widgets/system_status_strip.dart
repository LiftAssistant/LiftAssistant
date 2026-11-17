import 'package:flutter/material.dart';

import '../../api/read_model.dart';
import '../../state/client_state.dart';
import '../theme/instrument_theme.dart';
import 'semantic_badge.dart';

(String, Color, Color, bool) describeConnectionPhase(
  ConnectionPhase phase,
  InstrumentPalette palette,
) {
  switch (phase) {
    case ConnectionPhase.live:
      return ('LIVE SSE', palette.ok, palette.okContainer, false);
    case ConnectionPhase.attachingStream:
      return ('ATTACHING', palette.info, palette.infoContainer, true);
    case ConnectionPhase.connecting:
      return ('CONNECTING', palette.info, palette.infoContainer, true);
    case ConnectionPhase.reconnecting:
      return ('RECONNECTING', palette.caution, palette.cautionContainer, true);
    case ConnectionPhase.incompatibleApi:
      return (
        'UNSUPPORTED API',
        palette.danger,
        palette.dangerContainer,
        false,
      );
    case ConnectionPhase.protocolError:
      return ('PROTOCOL ERR', palette.danger, palette.dangerContainer, false);
    case ConnectionPhase.disconnected:
      return ('OFFLINE', palette.neutral, palette.neutralContainer, true);
  }
}

(String, String, Color, bool) describeHostLifecycle(
  HostLifecycle lifecycle,
  InstrumentPalette palette,
) {
  switch (lifecycle) {
    case HostLifecycle.starting:
      return (
        'Starting',
        'Host process started; awaiting initial observation activity.',
        palette.info,
        true,
      );
    case HostLifecycle.synchronizing:
      return (
        'Synchronizing',
        'Ingesting observations; complete operational state not yet established.',
        palette.caution,
        true,
      );
    case HostLifecycle.readyReadOnly:
      return (
        'ReadyReadOnly',
        'Complete operational state established for all configured Operational cars.',
        palette.ok,
        false,
      );
    case HostLifecycle.degraded:
      return (
        'Degraded',
        'Operational invariant violation or rejection revoked readiness.',
        palette.danger,
        false,
      );
  }
}

(String, String, Color, bool) describeHostReadiness(
  HostReadiness readiness,
  InstrumentPalette palette,
) {
  switch (readiness) {
    case HostReadiness.readOnly:
      return (
        'ReadOnly',
        'Read-side operational projection is synchronized.',
        palette.ok,
        false,
      );
    case HostReadiness.notReady:
      return (
        'NotReady',
        'Process reachable does not mean operational data is ready.',
        palette.caution,
        true,
      );
  }
}

/// Compact pinned telemetry bar replacing the old 4-card KPI grid.
class HostTelemetryBar extends StatelessWidget {
  const HostTelemetryBar({
    super.key,
    required this.snapshot,
    required this.isLive,
    this.revisionReset,
  });

  final ReadModelSnapshot snapshot;
  final bool isLive;
  final RevisionResetNotice? revisionReset;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    final (
      String lifecycleLabel,
      String lifecycleTip,
      Color lifecycleColor,
      bool lifecycleHollow,
    ) = describeHostLifecycle(
      snapshot.lifecycle,
      palette,
    );
    final (
      String readinessLabel,
      String readinessTip,
      Color readinessColor,
      bool readinessHollow,
    ) = describeHostReadiness(
      snapshot.readiness,
      palette,
    );

    return DecoratedBox(
      decoration: BoxDecoration(
        color: palette.elevatedSurface,
        border: Border(bottom: BorderSide(color: palette.border)),
      ),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 7),
        child: LayoutBuilder(
          builder: (BuildContext context, BoxConstraints constraints) {
            final bool showContractRight = constraints.maxWidth >= 680;
            final Widget leftCluster = Wrap(
              spacing: 18,
              runSpacing: 5,
              crossAxisAlignment: WrapCrossAlignment.center,
              children: <Widget>[
                _TelemetryCell(
                  label: 'Lifecycle',
                  value: lifecycleLabel,
                  dotColor: lifecycleColor,
                  hollowDot: lifecycleHollow,
                  tooltip: lifecycleTip,
                ),
                _TelemetryCell(
                  label: 'Readiness',
                  value: readinessLabel,
                  dotColor: readinessColor,
                  hollowDot: readinessHollow,
                  tooltip: readinessTip,
                ),
                Tooltip(
                  message: 'Host-local revision; monotonic within one Host process lifetime and resets on Host restart.',
                  child: Row(
                    mainAxisSize: MainAxisSize.min,
                    children: <Widget>[
                      Text(
                        'Rev ',
                        style: TextStyle(
                          fontSize: 11.5,
                          color: palette.mutedText,
                        ),
                      ),
                      Text(
                        '#${snapshot.revision}',
                        style: instrumentMonoStyle(
                          context,
                          fontSize: 12,
                          fontWeight: FontWeight.w700,
                        ),
                      ),
                      if (!isLive) ...<Widget>[
                        const SizedBox(width: 5),
                        Text(
                          '(retained)',
                          style: TextStyle(
                            fontSize: 11,
                            fontWeight: FontWeight.w600,
                            color: palette.caution,
                          ),
                        ),
                      ],
                    ],
                  ),
                ),
                if (revisionReset != null)
                  SemanticBadge(
                    label:
                        'New Host lifetime (#${revisionReset!.previousRevision} → #${revisionReset!.newRevision})',
                    foreground: palette.info,
                    background: palette.infoContainer,
                    monospace: true,
                  ),
                if (!showContractRight)
                  Text(
                    snapshot.apiVersion,
                    style: instrumentMonoStyle(
                      context,
                      fontSize: 11,
                      fontWeight: FontWeight.w500,
                      color: palette.mutedText,
                    ),
                  ),
              ],
            );

            if (!showContractRight) {
              return leftCluster;
            }

            return Row(
              children: <Widget>[
                Expanded(child: leftCluster),
                const SizedBox(width: 12),
                Text(
                  snapshot.apiVersion,
                  style: instrumentMonoStyle(
                    context,
                    fontSize: 11.5,
                    fontWeight: FontWeight.w500,
                    color: palette.mutedText,
                  ),
                ),
              ],
            );
          },
        ),
      ),
    );
  }
}

class _TelemetryCell extends StatelessWidget {
  const _TelemetryCell({
    required this.label,
    required this.value,
    required this.dotColor,
    required this.hollowDot,
    required this.tooltip,
  });

  final String label;
  final String value;
  final Color dotColor;
  final bool hollowDot;
  final String tooltip;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    return Tooltip(
      message: tooltip,
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: <Widget>[
          Text(
            '$label ',
            style: TextStyle(fontSize: 11.5, color: palette.mutedText),
          ),
          StatusDot(color: dotColor, hollow: hollowDot, size: 6.5),
          const SizedBox(width: 5),
          Text(
            value,
            style: const TextStyle(fontSize: 12, fontWeight: FontWeight.w600),
          ),
        ],
      ),
    );
  }
}

/// Renders connection / protocol / non-live-snapshot alert banners ONLY when
/// needed. Occupies zero pixels when synchronized and healthy.
class ClientAlertBanners extends StatelessWidget {
  const ClientAlertBanners({
    super.key,
    required this.state,
    required this.onReconnectNow,
    required this.onConfigureHost,
  });

  final ClientState state;
  final VoidCallback onReconnectNow;
  final VoidCallback onConfigureHost;

  bool _isHardError(ClientConnectionError? error) {
    if (error == null) {
      return false;
    }
    return state.phase == ConnectionPhase.incompatibleApi ||
        state.phase == ConnectionPhase.protocolError ||
        error.kind == ClientErrorKind.invalidUrl;
  }

  (String, String) _describeRetainedStaleBanner(
    ReadModelSnapshot snapshot,
    ClientConnectionError? error, {
    required bool hasHardErrorAbove,
  }) {
    if (hasHardErrorAbove) {
      return (
        'Retained snapshot (#${snapshot.revision}) is NOT live',
        'Showing last valid snapshot received before the error above; values below are historical context only.',
      );
    }
    if (state.phase == ConnectionPhase.connecting) {
      return (
        'Reconnecting — retained snapshot (#${snapshot.revision}) is NOT live',
        'Fetching fresh snapshot from ${state.endpoint?.origin ?? "Host"}; values below are historical context until sync completes.',
      );
    }
    if (state.phase == ConnectionPhase.disconnected && error == null) {
      return (
        'Client disconnected — retained snapshot (#${snapshot.revision}) is NOT live',
        'Live stream stopped; values below are retained client context, not live Host state.',
      );
    }
    final String detail = error?.message ?? 'connection interrupted';
    final String retryDetail = state.reconnectAttempt > 0
        ? ' (attempt #${state.reconnectAttempt})'
        : '';
    return (
      'Connection lost — retained snapshot (#${snapshot.revision}) is NOT live',
      'Last snapshot before disconnect$retryDetail: $detail',
    );
  }

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    final ReadModelSnapshot? snapshot = state.snapshot;
    final ClientConnectionError? error = state.error;
    final bool showHardErrorBanner = _isHardError(error);
    final bool showError =
        error != null &&
        (showHardErrorBanner || !state.hasRetainedStaleSnapshot);
    final bool showStale = state.hasRetainedStaleSnapshot;

    if (!showError && !showStale) {
      return const SizedBox.shrink();
    }

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      mainAxisSize: MainAxisSize.min,
      children: <Widget>[
        if (showError) _buildErrorBanner(context, error, palette),
        if (showError && showStale) const SizedBox(height: 8),
        if (showStale)
          Builder(
            builder: (BuildContext context) {
              final (
                String staleTitle,
                String staleMessage,
              ) = _describeRetainedStaleBanner(
                snapshot!,
                error,
                hasHardErrorAbove: showHardErrorBanner,
              );
              return SemanticCalloutBanner(
                key: const Key('stale-retained-snapshot-banner'),
                icon: Icons.cloud_off_outlined,
                title: staleTitle,
                message: staleMessage,
                accent: palette.caution,
                background: palette.cautionContainer,
                actions: showHardErrorBanner
                    ? const <Widget>[]
                    : <Widget>[
                        FilledButton.tonalIcon(
                          key: const Key('banner-reconnect-button'),
                          onPressed: state.phase == ConnectionPhase.connecting
                              ? null
                              : onReconnectNow,
                          icon: const Icon(Icons.refresh, size: 15),
                          label: Text(
                            state.phase == ConnectionPhase.connecting
                                ? 'Reconnecting...'
                                : 'Reconnect',
                          ),
                        ),
                        OutlinedButton.icon(
                          onPressed: onConfigureHost,
                          icon: const Icon(Icons.edit_outlined, size: 15),
                          label: const Text('Host URL'),
                        ),
                      ],
              );
            },
          ),
      ],
    );
  }

  Widget _buildErrorBanner(
    BuildContext context,
    ClientConnectionError error,
    InstrumentPalette palette,
  ) {
    final bool isHardError = _isHardError(error);
    final String versionLabel =
        (error.unsupportedVersion == null || error.unsupportedVersion!.isEmpty)
        ? 'empty'
        : error.unsupportedVersion!;

    final String title;
    switch (error.kind) {
      case ClientErrorKind.unsupportedApiVersion:
        title = 'Unsupported Host API version ($versionLabel)';
      case ClientErrorKind.malformedResponse:
        title = 'Malformed Host response';
      case ClientErrorKind.unexpectedContentType:
        title = 'Unexpected endpoint Content-Type';
      case ClientErrorKind.httpStatusError:
        title = 'Host HTTP ${error.httpStatusCode ?? "error"}';
      case ClientErrorKind.crossOriginBlockedOrUnreachable:
        title = 'Host unreachable or browser cross-origin blocked';
      case ClientErrorKind.invalidUrl:
        title = 'Invalid Host URL';
      case ClientErrorKind.streamDisconnected:
      case ClientErrorKind.unreachable:
        title = state.phase == ConnectionPhase.reconnecting
            ? 'Host unreachable — reconnecting (attempt #${state.reconnectAttempt})'
            : 'Connection error';
    }

    return SemanticCalloutBanner(
      key: const Key('connection-error-banner'),
      icon: isHardError
          ? Icons.error_outline_rounded
          : Icons.wifi_tethering_error_rounded_outlined,
      title: title,
      message: error.message,
      accent: isHardError ? palette.danger : palette.caution,
      background: isHardError
          ? palette.dangerContainer
          : palette.cautionContainer,
      actions: <Widget>[
        if (state.endpoint != null)
          FilledButton.tonalIcon(
            key: const Key('error-retry-button'),
            onPressed: onReconnectNow,
            icon: const Icon(Icons.refresh, size: 15),
            label: const Text('Retry'),
          ),
        OutlinedButton.icon(
          key: const Key('error-configure-host-button'),
          onPressed: onConfigureHost,
          icon: const Icon(Icons.dns_outlined, size: 15),
          label: const Text('Host URL'),
        ),
      ],
    );
  }
}
