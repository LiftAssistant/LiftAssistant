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
        Icons.sensors_rounded,
      );
    case ConnectionPhase.attachingStream:
      return (
        'ATTACHING',
        palette.info,
        palette.infoContainer,
        Icons.sync_rounded,
      );
    case ConnectionPhase.connecting:
      return (
        'CONNECTING',
        palette.info,
        palette.infoContainer,
        Icons.sync_rounded,
      );
    case ConnectionPhase.reconnecting:
      return (
        'RECONNECTING',
        palette.caution,
        palette.cautionContainer,
        Icons.sync_problem_rounded,
      );
    case ConnectionPhase.incompatibleApi:
      return (
        'UNSUPPORTED API',
        palette.danger,
        palette.dangerContainer,
        Icons.block_rounded,
      );
    case ConnectionPhase.protocolError:
      return (
        'PROTOCOL ERR',
        palette.danger,
        palette.dangerContainer,
        Icons.error_outline_rounded,
      );
    case ConnectionPhase.disconnected:
      return (
        'OFFLINE',
        palette.neutral,
        palette.neutralContainer,
        Icons.cloud_off_outlined,
      );
  }
}

(String, String, Color, Color, IconData) describeHostLifecycle(
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
        Icons.hourglass_top_rounded,
      );
    case HostLifecycle.synchronizing:
      return (
        'Synchronizing',
        'Ingesting observations; complete operational state not yet established.',
        palette.caution,
        palette.cautionContainer,
        Icons.sync_rounded,
      );
    case HostLifecycle.readyReadOnly:
      return (
        'ReadyReadOnly',
        'Complete operational state established for all configured Operational cars.',
        palette.ok,
        palette.okContainer,
        Icons.check_circle_outline_rounded,
      );
    case HostLifecycle.degraded:
      return (
        'Degraded',
        'Operational invariant violation or rejection revoked readiness.',
        palette.danger,
        palette.dangerContainer,
        Icons.warning_amber_rounded,
      );
  }
}

(String, String, Color, Color, IconData) describeHostReadiness(
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
        Icons.verified_outlined,
      );
    case HostReadiness.notReady:
      return (
        'NotReady',
        'Process reachable does not mean operational data is ready.',
        palette.caution,
        palette.cautionContainer,
        Icons.pending_outlined,
      );
  }
}

/// Top 4-card M3 Host Status Strip + conditional alert banners.
/// Displays explicit field labels (`Host Lifecycle`, `Read Readiness`,
/// `Host Revision`, `Snapshot Summary`) with Material 3 avatars and chips,
/// zero `●`/`·` cheap separators, and zero duplicate `snake_case` wire tags.
class SystemStatusStrip extends StatelessWidget {
  const SystemStatusStrip({
    super.key,
    required this.state,
    required this.onReconnectNow,
    this.onConfigureHost,
  });

  final ClientState state;
  final VoidCallback onReconnectNow;
  final VoidCallback? onConfigureHost;

  @override
  Widget build(BuildContext context) {
    final ReadModelSnapshot? snapshot = state.snapshot;
    final bool hasAlerts =
        state.error != null || state.hasRetainedStaleSnapshot;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      mainAxisSize: MainAxisSize.min,
      children: <Widget>[
        if (hasAlerts)
          ClientAlertBanners(
            state: state,
            onReconnectNow: onReconnectNow,
            onConfigureHost: onConfigureHost,
          ),
        if (hasAlerts && snapshot != null) const SizedBox(height: 12),
        if (snapshot != null)
          _HostStatusGrid(
            key: const Key('system-status-strip-grid'),
            snapshot: snapshot,
            isLive: state.isSnapshotLive,
            revisionReset: state.lastRevisionReset,
          ),
      ],
    );
  }
}

class _HostStatusGrid extends StatelessWidget {
  const _HostStatusGrid({
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
    final double textScale = MediaQuery.textScalerOf(context).scale(1.0);

    final (
      String lifecycleLabel,
      String lifecycleTip,
      Color lifecycleFg,
      Color lifecycleBg,
      IconData lifecycleIcon,
    ) = describeHostLifecycle(
      snapshot.lifecycle,
      palette,
    );

    final (
      String readinessLabel,
      String readinessTip,
      Color readinessFg,
      Color readinessBg,
      IconData readinessIcon,
    ) = describeHostReadiness(
      snapshot.readiness,
      palette,
    );

    final bool hasReset = revisionReset != null;
    final Color revFg = hasReset
        ? palette.info
        : (isLive ? palette.ok : palette.caution);
    final Color revBg = hasReset
        ? palette.infoContainer
        : (isLive ? palette.okContainer : palette.cautionContainer);

    final int carsCount = snapshot.operationalCars.length;
    final int sourcesCount = snapshot.sources.length;
    final int diagCount = snapshot.recentDiagnostics.length;
    final String carsBadgeLabel =
        '$carsCount ${carsCount == 1 ? "Car" : "Cars"}';
    final String sourcesBadgeLabel =
        '$sourcesCount ${sourcesCount == 1 ? "Source" : "Sources"}';
    final String eventsBadgeLabel =
        '$diagCount ${diagCount == 1 ? "Event" : "Events"}';
    final Color onSurfaceColor = Theme.of(context).colorScheme.onSurface;
    final bool hasMultiDigitSummary =
        carsCount >= 10 || sourcesCount >= 10 || diagCount >= 10;

    return LayoutBuilder(
      builder: (BuildContext context, BoxConstraints constraints) {
        final double width = constraints.maxWidth;
        const double spacing = 12.0;
        final bool compactSummaryCard =
            width < 1140 || textScale > 1.15 || hasMultiDigitSummary;

        final List<Widget> cards = <Widget>[
          _StatusStripCard(
            key: const Key('status-card-lifecycle'),
            icon: lifecycleIcon,
            iconForeground: lifecycleFg,
            iconBackground: lifecycleBg,
            label: 'Host Lifecycle',
            value: lifecycleLabel,
            valueColor: lifecycleFg,
            tooltip: 'Host Lifecycle: $lifecycleTip',
          ),
          _StatusStripCard(
            key: const Key('status-card-readiness'),
            icon: readinessIcon,
            iconForeground: readinessFg,
            iconBackground: readinessBg,
            label: 'Read Readiness',
            value: readinessLabel,
            valueColor: readinessFg,
            tooltip: 'Read Readiness: $readinessTip',
          ),
          _StatusStripCard(
            key: const Key('status-card-revision'),
            icon: hasReset ? Icons.restart_alt_rounded : Icons.tag_rounded,
            iconForeground: revFg,
            iconBackground: revBg,
            label: 'Host Revision',
            value: '#${snapshot.revision}',
            trailingBadge: hasReset
                ? SemanticBadge(
                    label:
                        'New lifetime (#${revisionReset!.previousRevision} → #${revisionReset!.newRevision})',
                    foreground: palette.info,
                    background: palette.infoContainer,
                    icon: Icons.restart_alt_rounded,
                    compact: true,
                  )
                : SemanticBadge(
                    label: isLive ? 'Live' : 'Retained',
                    foreground: isLive ? palette.ok : palette.caution,
                    background: isLive
                        ? palette.okContainer
                        : palette.cautionContainer,
                    icon: isLive ? Icons.bolt_rounded : Icons.history_rounded,
                    compact: true,
                  ),
            tooltip: hasReset
                ? 'Host restarted with a new process lifetime (#${revisionReset!.previousRevision} → #${revisionReset!.newRevision}).'
                : 'Host-local revision (${snapshot.apiVersion}); monotonic within one Host process lifetime and resets on Host restart.',
          ),
          _StatusStripCard(
            key: const Key('status-card-summary'),
            icon: Icons.layers_outlined,
            iconForeground: palette.info,
            iconBackground: palette.infoContainer,
            hideAvatarOnCompactDesktop: compactSummaryCard,
            label: 'Snapshot Summary',
            customContent: Wrap(
              spacing: 4,
              runSpacing: 4,
              crossAxisAlignment: WrapCrossAlignment.center,
              children: <Widget>[
                SemanticBadge(
                  label: carsBadgeLabel,
                  foreground: onSurfaceColor,
                  background: palette.elevatedSurface,
                  compact: true,
                ),
                SemanticBadge(
                  label: sourcesBadgeLabel,
                  foreground: onSurfaceColor,
                  background: palette.elevatedSurface,
                  compact: true,
                ),
                SemanticBadge(
                  label: eventsBadgeLabel,
                  foreground: diagCount > 0 ? palette.info : palette.subtleText,
                  background: diagCount > 0
                      ? palette.infoContainer
                      : palette.elevatedSurface,
                  compact: true,
                ),
              ],
            ),
            tooltip:
                'Boundary contract ${snapshot.apiVersion} (read-only): $carsCount operational car(s), $sourcesCount observation source(s), $diagCount recent diagnostic event(s).',
          ),
        ];

        if (width >= 960 && textScale <= 1.35) {
          return IntrinsicHeight(
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: <Widget>[
                Expanded(flex: 3, child: cards[0]),
                const SizedBox(width: spacing),
                Expanded(flex: 3, child: cards[1]),
                const SizedBox(width: spacing),
                Expanded(flex: 3, child: cards[2]),
                const SizedBox(width: spacing),
                Expanded(flex: 5, child: cards[3]),
              ],
            ),
          );
        }

        if (width >= 310 && textScale <= 1.35 ||
            width >= 540 && textScale <= 1.6) {
          final double gridSpacing = width < 480 ? 8.0 : spacing;
          return Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            mainAxisSize: MainAxisSize.min,
            children: <Widget>[
              IntrinsicHeight(
                child: Row(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: <Widget>[
                    Expanded(child: cards[0]),
                    SizedBox(width: gridSpacing),
                    Expanded(child: cards[1]),
                  ],
                ),
              ),
              SizedBox(height: gridSpacing),
              IntrinsicHeight(
                child: Row(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: <Widget>[
                    Expanded(child: cards[2]),
                    SizedBox(width: gridSpacing),
                    Expanded(child: cards[3]),
                  ],
                ),
              ),
            ],
          );
        }

        return Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          mainAxisSize: MainAxisSize.min,
          children: <Widget>[
            cards[0],
            const SizedBox(height: 8),
            cards[1],
            const SizedBox(height: 8),
            cards[2],
            const SizedBox(height: 8),
            cards[3],
          ],
        );
      },
    );
  }
}

class _StatusStripCard extends StatelessWidget {
  const _StatusStripCard({
    super.key,
    required this.icon,
    required this.iconForeground,
    required this.iconBackground,
    required this.label,
    required this.tooltip,
    this.value,
    this.valueColor,
    this.trailingBadge,
    this.customContent,
    this.hideAvatarOnCompactDesktop = false,
  });

  final IconData icon;
  final Color iconForeground;
  final Color iconBackground;
  final String label;
  final String tooltip;
  final String? value;
  final Color? valueColor;
  final Widget? trailingBadge;
  final Widget? customContent;
  final bool hideAvatarOnCompactDesktop;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    final double textScale = MediaQuery.textScalerOf(context).scale(1.0);
    final double screenWidth = MediaQuery.sizeOf(context).width;
    final bool compact = screenWidth < 520 || textScale > 1.35;
    final bool showAvatar = !compact && !hideAvatarOnCompactDesktop;

    return Tooltip(
      message: tooltip,
      child: Card(
        child: Padding(
          padding: EdgeInsets.symmetric(
            horizontal: compact ? 10 : 14,
            vertical: compact ? 8 : 12,
          ),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.center,
            children: <Widget>[
              if (showAvatar) ...<Widget>[
                CircleAvatar(
                  radius: 18,
                  backgroundColor: iconBackground,
                  foregroundColor: iconForeground,
                  child: Icon(icon, size: 19),
                ),
                const SizedBox(width: 12),
              ],
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  mainAxisAlignment: MainAxisAlignment.center,
                  mainAxisSize: MainAxisSize.min,
                  children: <Widget>[
                    Text(
                      label,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: TextStyle(
                        fontSize: 11.5,
                        fontWeight: FontWeight.w600,
                        color: palette.mutedText,
                      ),
                    ),
                    const SizedBox(height: 4),
                    if (customContent != null)
                      customContent!
                    else
                      Wrap(
                        spacing: 8,
                        runSpacing: 4,
                        crossAxisAlignment: WrapCrossAlignment.center,
                        children: <Widget>[
                          if (value != null)
                            Text(
                              value!,
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: instrumentTabularStyle(
                                context,
                                fontSize: 14.5,
                                fontWeight: FontWeight.w700,
                                color:
                                    valueColor ??
                                    Theme.of(context).colorScheme.onSurface,
                              ),
                            ),
                          ?trailingBadge,
                        ],
                      ),
                  ],
                ),
              ),
            ],
          ),
        ),
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
