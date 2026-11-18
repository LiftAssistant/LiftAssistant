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

(String, String, Color, Color, bool) describeHostLifecycle(
  HostLifecycle lifecycle,
  InstrumentPalette palette,
) {
  switch (lifecycle) {
    case HostLifecycle.starting:
      return (
        'Starting',
        'Host process started; awaiting initial observation activity.',
        palette.info,
        palette.infoContainer,
        true,
      );
    case HostLifecycle.synchronizing:
      return (
        'Synchronizing',
        'Ingesting observations; complete operational state not yet established.',
        palette.caution,
        palette.cautionContainer,
        true,
      );
    case HostLifecycle.readyReadOnly:
      return (
        'ReadyReadOnly',
        'Complete operational state established for all configured Operational cars.',
        palette.ok,
        palette.okContainer,
        false,
      );
    case HostLifecycle.degraded:
      return (
        'Degraded',
        'Operational invariant violation or rejection revoked readiness.',
        palette.danger,
        palette.dangerContainer,
        false,
      );
  }
}

(String, String, Color, Color, bool) describeHostReadiness(
  HostReadiness readiness,
  InstrumentPalette palette,
) {
  switch (readiness) {
    case HostReadiness.readOnly:
      return (
        'ReadOnly',
        'Read-side operational projection is synchronized.',
        palette.ok,
        palette.okContainer,
        false,
      );
    case HostReadiness.notReady:
      return (
        'NotReady',
        'Process reachable does not mean operational data is ready.',
        palette.caution,
        palette.cautionContainer,
        true,
      );
  }
}

/// Renders Host status (`Lifecycle`, `Readiness`, `Rev #N`, and optional
/// `New Host lifetime`) as genuine Material 3 status chips.
class HostStatusChipGroup extends StatelessWidget {
  const HostStatusChipGroup({
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
      Color lifecycleFg,
      Color lifecycleBg,
      bool lifecycleHollow,
    ) = describeHostLifecycle(
      snapshot.lifecycle,
      palette,
    );
    final (
      String readinessLabel,
      String readinessTip,
      Color readinessFg,
      Color readinessBg,
      bool readinessHollow,
    ) = describeHostReadiness(
      snapshot.readiness,
      palette,
    );

    final String revLabel = isLive
        ? 'Rev #${snapshot.revision}'
        : 'Rev #${snapshot.revision} (Retained)';

    return Wrap(
      spacing: 6,
      runSpacing: 6,
      crossAxisAlignment: WrapCrossAlignment.center,
      children: <Widget>[
        SemanticBadge(
          label: lifecycleLabel,
          foreground: lifecycleFg,
          background: lifecycleBg,
          dotColor: lifecycleFg,
          hollowDot: lifecycleHollow,
          tooltip: 'Host Lifecycle: $lifecycleTip',
        ),
        SemanticBadge(
          label: readinessLabel,
          foreground: readinessFg,
          background: readinessBg,
          dotColor: readinessFg,
          hollowDot: readinessHollow,
          tooltip: 'Host Readiness: $readinessTip',
        ),
        SemanticBadge(
          label: revLabel,
          foreground: isLive ? palette.subtleText : palette.caution,
          background: isLive
              ? palette.elevatedSurface
              : palette.cautionContainer,
          icon: Icons.tag_rounded,
          outlined: isLive,
          tooltip:
              'Host-local revision (${snapshot.apiVersion}); monotonic within one Host process lifetime and resets on Host restart.',
        ),
        if (revisionReset != null)
          SemanticBadge(
            label:
                'New Host lifetime (#${revisionReset!.previousRevision} → #${revisionReset!.newRevision})',
            foreground: palette.info,
            background: palette.infoContainer,
            icon: Icons.restart_alt_rounded,
            tooltip:
                'Host restarted with a new process lifetime; revision sequence reset from #${revisionReset!.previousRevision} to #${revisionReset!.newRevision}.',
          ),
      ],
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
    this.onConfigureHost,
  });

  final ClientState state;
  final VoidCallback onReconnectNow;
  final VoidCallback? onConfigureHost;

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
                hasHardErrorAbove: showHardErrorAbove(showHardErrorBanner),
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
                        if (onConfigureHost != null)
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

  bool showHardErrorAbove(bool flag) => flag;

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
        if (onConfigureHost != null)
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
