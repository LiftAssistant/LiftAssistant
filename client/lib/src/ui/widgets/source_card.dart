import 'package:flutter/material.dart';

import '../../api/read_model.dart';
import '../theme/instrument_theme.dart';
import 'semantic_badge.dart';

String formatTransportStateLabel(TransportState state) {
  switch (state) {
    case TransportState.unknown:
      return 'Unknown';
    case TransportState.subscriberCreated:
      return 'Subscriber Created';
    case TransportState.discoveryObserved:
      return 'Discovery Observed';
  }
}

String formatPayloadActivityLabel(PayloadActivity payload) {
  switch (payload) {
    case PayloadActivity.awaiting:
      return 'Awaiting';
    case PayloadActivity.active:
      return 'Active';
    case PayloadActivity.stale:
      return 'Stale';
  }
}

String formatAvailabilityLabel(ObservationAvailability availability) {
  switch (availability) {
    case ObservationAvailability.awaiting:
      return 'Awaiting';
    case ObservationAvailability.incomplete:
      return 'Incomplete';
    case ObservationAvailability.unresolved:
      return 'Unresolved';
    case ObservationAvailability.resolved:
      return 'Resolved';
    case ObservationAvailability.revoked:
      return 'Revoked';
  }
}

class SourceHealthCard extends StatelessWidget {
  const SourceHealthCard({
    super.key,
    required this.source,
    this.fillRowHeight = false,
  });

  final SourceHealth source;
  final bool fillRowHeight;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;

    final (
      String sessionStateLabel,
      String sessionSubtitle,
      Color sessionColor,
      Color sessionBg,
      bool sessionHollow,
    ) = switch (source.session) {
      InactiveSourceSession() => (
        'Inactive',
        'Session: Inactive',
        palette.neutral,
        palette.elevatedSurface,
        true,
      ),
      ActiveSourceSession(:final session) => (
        'Active',
        'Session: $session',
        palette.ok,
        palette.okContainer,
        false,
      ),
    };

    final (
      String transportLabel,
      Color transportColor,
      bool transportHollow,
    ) = switch (source.transport) {
      TransportState.unknown => ('Unknown', palette.neutral, true),
      TransportState.subscriberCreated => (
        'Subscriber Created',
        palette.info,
        true,
      ),
      TransportState.discoveryObserved => (
        'Discovery Observed',
        palette.ok,
        false,
      ),
    };

    final (
      String payloadLabel,
      Color payloadColor,
      bool payloadHollow,
    ) = switch (source.payload) {
      PayloadActivity.awaiting => ('Awaiting', palette.caution, true),
      PayloadActivity.active => ('Active', palette.ok, false),
      PayloadActivity.stale => ('Stale', palette.danger, false),
    };

    final bool discoveryWithoutActivePayload =
        source.transport == TransportState.discoveryObserved &&
        source.payload != PayloadActivity.active;

    return Card(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: fillRowHeight ? MainAxisSize.max : MainAxisSize.min,
        children: <Widget>[
          // 1. Header: M3 ListTile-style header + bound count chip
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 14, 16, 12),
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.center,
              children: <Widget>[
                CircleAvatar(
                  radius: 18,
                  backgroundColor: sessionBg,
                  foregroundColor: sessionColor,
                  child: const Icon(Icons.hub_outlined, size: 18),
                ),
                const SizedBox(width: 12),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    mainAxisSize: MainAxisSize.min,
                    children: <Widget>[
                      Tooltip(
                        message: 'ObservationSourceId: ${source.source}',
                        child: Text(
                          source.source,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: instrumentTabularStyle(
                            context,
                            fontSize: 15,
                            fontWeight: FontWeight.w700,
                            letterSpacing: -0.15,
                          ),
                        ),
                      ),
                      const SizedBox(height: 2),
                      Tooltip(
                        message: sessionSubtitle,
                        child: Text(
                          sessionSubtitle,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: instrumentTabularStyle(
                            context,
                            fontSize: 12,
                            fontWeight: FontWeight.w500,
                            color: palette.subtleText,
                          ),
                        ),
                      ),
                    ],
                  ),
                ),
                const SizedBox(width: 8),
                ConstrainedBox(
                  constraints: const BoxConstraints(maxWidth: 120),
                  child: SemanticBadge(
                    label: '${source.observations.length} bound',
                    foreground: palette.subtleText,
                    background: palette.elevatedSurface,
                    outlined: true,
                    tooltip:
                        '${source.observations.length} logical car binding(s) on this source',
                  ),
                ),
              ],
            ),
          ),

          // 2. 3-Column Equal-Width Tonal Health Tiles
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 0, 16, 14),
            child: Builder(
              builder: (BuildContext context) {
                final double textScale = MediaQuery.textScalerOf(context)
                    .scale(1.0);
                final double screenWidth = MediaQuery.sizeOf(context).width;
                final bool useThreeColumns =
                    screenWidth >= 340 && textScale <= 1.35;
                final bool equalWidths =
                    (screenWidth >= 460 && screenWidth < 760) ||
                    screenWidth >= 1120;

                if (useThreeColumns) {
                  return Row(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: <Widget>[
                      Expanded(
                        flex: equalWidths ? 1 : 7,
                        child: _SourceTonalHealthTile(
                          title: 'Session',
                          value: sessionStateLabel,
                          dotColor: sessionColor,
                          hollowDot: sessionHollow,
                          tooltip: sessionSubtitle,
                        ),
                      ),
                      const SizedBox(width: 8),
                      Expanded(
                        flex: equalWidths ? 1 : 11,
                        child: _SourceTonalHealthTile(
                          title: 'Transport',
                          value: transportLabel,
                          dotColor: transportColor,
                          hollowDot: transportHollow,
                          tooltip:
                              'Transport state: ${source.transport.wireValue}',
                        ),
                      ),
                      const SizedBox(width: 8),
                      Expanded(
                        flex: equalWidths ? 1 : 7,
                        child: _SourceTonalHealthTile(
                          title: 'Payload',
                          value: payloadLabel,
                          dotColor: payloadColor,
                          hollowDot: payloadHollow,
                          tooltip:
                              'Payload activity: ${source.payload.wireValue}',
                        ),
                      ),
                    ],
                  );
                }

                return Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: <Widget>[
                    _SourceTonalHealthTile(
                      title: 'Session',
                      value: sessionStateLabel,
                      dotColor: sessionColor,
                      hollowDot: sessionHollow,
                      tooltip: sessionSubtitle,
                    ),
                    const SizedBox(height: 6),
                    _SourceTonalHealthTile(
                      title: 'Transport',
                      value: transportLabel,
                      dotColor: transportColor,
                      hollowDot: transportHollow,
                      tooltip: 'Transport state: ${source.transport.wireValue}',
                    ),
                    const SizedBox(height: 6),
                    _SourceTonalHealthTile(
                      title: 'Payload',
                      value: payloadLabel,
                      dotColor: payloadColor,
                      hollowDot: payloadHollow,
                      tooltip: 'Payload activity: ${source.payload.wireValue}',
                    ),
                  ],
                );
              },
            ),
          ),

          if (fillRowHeight) const Spacer(),

          // Discovery != usable payload M3 warning banner
          if (discoveryWithoutActivePayload)
            Padding(
              padding: const EdgeInsets.fromLTRB(16, 0, 16, 12),
              child: Container(
                padding: const EdgeInsets.symmetric(
                  horizontal: 12,
                  vertical: 9,
                ),
                decoration: BoxDecoration(
                  color: palette.cautionContainer,
                  borderRadius: BorderRadius.circular(10),
                  border: Border.all(
                    color: palette.caution.withValues(alpha: 0.4),
                  ),
                ),
                child: Row(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: <Widget>[
                    Padding(
                      padding: const EdgeInsets.only(top: 1),
                      child: Icon(
                        Icons.warning_amber_rounded,
                        size: 16,
                        color: palette.caution,
                      ),
                    ),
                    const SizedBox(width: 8),
                    Expanded(
                      child: Text(
                        'Discovery observed without active payload (${payloadLabel.toLowerCase()}) — discovery alone is not usable data.',
                        style: TextStyle(
                          fontSize: 12,
                          height: 1.35,
                          fontWeight: FontWeight.w500,
                          color: Theme.of(context).colorScheme.onSurface,
                        ),
                      ),
                    ),
                  ],
                ),
              ),
            ),

          // 3. Bound logical cars & authority roles M3 rows
          const Divider(height: 1),
          if (source.observations.isEmpty)
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 12),
              child: Text(
                'No bound logical cars on this source.',
                style: TextStyle(fontSize: 12.5, color: palette.mutedText),
              ),
            )
          else
            for (int i = 0; i < source.observations.length; i++) ...<Widget>[
              if (i > 0) const Divider(height: 1),
              _ObservationBindingRow(observation: source.observations[i]),
            ],
        ],
      ),
    );
  }
}

class _SourceTonalHealthTile extends StatelessWidget {
  const _SourceTonalHealthTile({
    required this.title,
    required this.value,
    required this.dotColor,
    required this.hollowDot,
    required this.tooltip,
  });

  final String title;
  final String value;
  final Color dotColor;
  final bool hollowDot;
  final String tooltip;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    return Tooltip(
      message: tooltip,
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 8),
        decoration: BoxDecoration(
          color: palette.elevatedSurface,
          borderRadius: BorderRadius.circular(10),
          border: Border.all(color: palette.border),
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          mainAxisSize: MainAxisSize.min,
          children: <Widget>[
            Text(
              title,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(
                fontSize: 11,
                fontWeight: FontWeight.w500,
                color: palette.mutedText,
              ),
            ),
            const SizedBox(height: 4),
            Row(
              mainAxisSize: MainAxisSize.min,
              children: <Widget>[
                StatusDot(color: dotColor, hollow: hollowDot, size: 7),
                const SizedBox(width: 6),
                Flexible(
                  child: Text(
                    value,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: instrumentTabularStyle(
                      context,
                      fontSize: 12,
                      fontWeight: FontWeight.w700,
                      color: Theme.of(context).colorScheme.onSurface,
                    ),
                  ),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

class _ObservationBindingRow extends StatelessWidget {
  const _ObservationBindingRow({required this.observation});

  final SourceObservation observation;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    final bool isOperational = observation.role == BindingRole.operational;

    final (
      String availLabel,
      Color availFg,
      Color availBg,
      bool availHollow,
      bool availOutlined,
    ) = switch (observation.availability) {
      ObservationAvailability.resolved => (
        'Resolved',
        palette.ok,
        palette.okContainer,
        false,
        false,
      ),
      ObservationAvailability.awaiting => (
        'Awaiting',
        palette.neutral,
        palette.neutralContainer,
        true,
        true,
      ),
      ObservationAvailability.incomplete => (
        'Incomplete',
        palette.caution,
        palette.cautionContainer,
        true,
        false,
      ),
      ObservationAvailability.unresolved => (
        'Unresolved',
        palette.caution,
        palette.cautionContainer,
        true,
        false,
      ),
      ObservationAvailability.revoked => (
        'Revoked',
        palette.danger,
        palette.dangerContainer,
        false,
        false,
      ),
    };

    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 10),
      child: Wrap(
        spacing: 10,
        runSpacing: 6,
        alignment: WrapAlignment.spaceBetween,
        crossAxisAlignment: WrapCrossAlignment.center,
        children: <Widget>[
          Row(
            mainAxisSize: MainAxisSize.min,
            children: <Widget>[
              Icon(
                Icons.elevator_outlined,
                size: 16,
                color: isOperational ? palette.info : palette.mutedText,
              ),
              const SizedBox(width: 8),
              ConstrainedBox(
                constraints: const BoxConstraints(maxWidth: 160),
                child: Tooltip(
                  message: 'Bound logical car: ${observation.car}',
                  child: Text(
                    observation.car,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: instrumentTabularStyle(
                      context,
                      fontSize: 13.5,
                      fontWeight: FontWeight.w700,
                    ),
                  ),
                ),
              ),
            ],
          ),
          Wrap(
            spacing: 6,
            runSpacing: 4,
            crossAxisAlignment: WrapCrossAlignment.center,
            children: <Widget>[
              SemanticBadge(
                label: isOperational ? 'Operational' : 'Auxiliary',
                foreground: isOperational
                    ? palette.operationalRole
                    : palette.auxiliaryRole,
                background: isOperational
                    ? palette.operationalRoleContainer
                    : palette.auxiliaryRoleContainer,
                outlined: !isOperational,
                tooltip: isOperational
                    ? 'Operational authority: permitted into LiftAssistant operational truth when resolved.'
                    : 'Auxiliary role: inspectable and diagnostic only — excluded from operational truth and never used for automatic failover.',
              ),
              SemanticBadge(
                label: availLabel,
                foreground: availFg,
                background: availBg,
                dotColor: availFg,
                hollowDot: availHollow,
                outlined: availOutlined,
                tooltip:
                    'Observation availability: ${observation.availability.wireValue}',
              ),
            ],
          ),
        ],
      ),
    );
  }
}
