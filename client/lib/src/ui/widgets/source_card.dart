import 'package:flutter/material.dart';

import '../../api/read_model.dart';
import '../theme/instrument_theme.dart';
import 'semantic_badge.dart';

class SourceHealthCard extends StatelessWidget {
  const SourceHealthCard({super.key, required this.source});

  final SourceHealth source;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;

    final (
      String sessionText,
      Color sessionDot,
      bool sessionHollow,
    ) = switch (source.session) {
      InactiveSourceSession() => ('inactive', palette.neutral, true),
      ActiveSourceSession(:final session) => (session, palette.ok, false),
    };

    final (
      Color transportColor,
      bool transportHollow,
    ) = switch (source.transport) {
      TransportState.unknown => (palette.neutral, true),
      TransportState.subscriberCreated => (palette.info, true),
      TransportState.discoveryObserved => (palette.ok, false),
    };

    final (Color payloadColor, bool payloadHollow) = switch (source.payload) {
      PayloadActivity.awaiting => (palette.caution, true),
      PayloadActivity.active => (palette.ok, false),
      PayloadActivity.stale => (palette.danger, false),
    };

    final bool discoveryWithoutActivePayload =
        source.transport == TransportState.discoveryObserved &&
        source.payload != PayloadActivity.active;

    return DecoratedBox(
      decoration: BoxDecoration(
        color: palette.surface,
        borderRadius: BorderRadius.circular(6),
        border: Border.all(color: palette.border),
      ),
      child: ClipRRect(
        borderRadius: BorderRadius.circular(5),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          mainAxisSize: MainAxisSize.min,
          children: <Widget>[
            // Source header + 3 orthogonal health dimensions
            Padding(
              padding: const EdgeInsets.fromLTRB(12, 9, 12, 9),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: <Widget>[
                  Row(
                    children: <Widget>[
                      Expanded(
                        child: Text(
                          source.source,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: instrumentMonoStyle(
                            context,
                            fontSize: 13.5,
                            fontWeight: FontWeight.w700,
                          ),
                        ),
                      ),
                      const SizedBox(width: 8),
                      Text(
                        '${source.observations.length} bound',
                        style: TextStyle(
                          fontSize: 11,
                          color: palette.mutedText,
                        ),
                      ),
                    ],
                  ),
                  const SizedBox(height: 6),
                  Wrap(
                    spacing: 14,
                    runSpacing: 5,
                    crossAxisAlignment: WrapCrossAlignment.center,
                    children: <Widget>[
                      _SourceTelemetryItem(
                        label: 'Session',
                        value: sessionText,
                        dotColor: sessionDot,
                        hollowDot: sessionHollow,
                      ),
                      _SourceTelemetryItem(
                        label: 'Transport',
                        value: source.transport.wireValue,
                        dotColor: transportColor,
                        hollowDot: transportHollow,
                      ),
                      _SourceTelemetryItem(
                        label: 'Payload',
                        value: source.payload.wireValue,
                        dotColor: payloadColor,
                        hollowDot: payloadHollow,
                      ),
                    ],
                  ),
                ],
              ),
            ),

            // Discovery != usable payload contextual callout
            if (discoveryWithoutActivePayload)
              DecoratedBox(
                decoration: BoxDecoration(
                  color: palette.cautionContainer,
                  border: Border(top: BorderSide(color: palette.border)),
                ),
                child: Padding(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 12,
                    vertical: 6,
                  ),
                  child: Row(
                    children: <Widget>[
                      Icon(
                        Icons.info_outline_rounded,
                        size: 13.5,
                        color: palette.caution,
                      ),
                      const SizedBox(width: 6),
                      Expanded(
                        child: Text(
                          'Discovery observed without active payload (${source.payload.wireValue}) — discovery alone is not usable data.',
                          style: TextStyle(
                            fontSize: 11.5,
                            color: Theme.of(context).colorScheme.onSurface,
                          ),
                        ),
                      ),
                    ],
                  ),
                ),
              ),

            // Bound logical cars & authority roles ledger
            if (source.observations.isEmpty)
              DecoratedBox(
                decoration: BoxDecoration(
                  color: palette.elevatedSurface,
                  border: Border(top: BorderSide(color: palette.border)),
                ),
                child: Padding(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 12,
                    vertical: 8,
                  ),
                  child: Text(
                    'No bound logical cars on this source.',
                    style: TextStyle(fontSize: 11.5, color: palette.mutedText),
                  ),
                ),
              )
            else
              for (int i = 0; i < source.observations.length; i++)
                _ObservationBindingRow(
                  observation: source.observations[i],
                  isFirst: i == 0,
                ),
          ],
        ),
      ),
    );
  }
}

class _SourceTelemetryItem extends StatelessWidget {
  const _SourceTelemetryItem({
    required this.label,
    required this.value,
    required this.dotColor,
    required this.hollowDot,
  });

  final String label;
  final String value;
  final Color dotColor;
  final bool hollowDot;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    return ConstrainedBox(
      constraints: const BoxConstraints(maxWidth: 260),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: <Widget>[
          Text(
            '$label ',
            style: TextStyle(fontSize: 11, color: palette.mutedText),
          ),
          StatusDot(color: dotColor, hollow: hollowDot, size: 6),
          const SizedBox(width: 4.5),
          Flexible(
            child: Tooltip(
              message: '$label: $value',
              child: Text(
                value,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: instrumentMonoStyle(
                  context,
                  fontSize: 11.5,
                  fontWeight: FontWeight.w500,
                  color: palette.subtleText,
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class _ObservationBindingRow extends StatelessWidget {
  const _ObservationBindingRow({
    required this.observation,
    required this.isFirst,
  });

  final SourceObservation observation;
  final bool isFirst;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    final bool isOperational = observation.role == BindingRole.operational;

    final (
      Color availColor,
      bool availHollow,
    ) = switch (observation.availability) {
      ObservationAvailability.resolved => (palette.ok, false),
      ObservationAvailability.awaiting => (palette.neutral, true),
      ObservationAvailability.incomplete => (palette.caution, true),
      ObservationAvailability.unresolved => (palette.caution, true),
      ObservationAvailability.revoked => (palette.danger, false),
    };

    return DecoratedBox(
      decoration: BoxDecoration(
        color: isFirst
            ? palette.elevatedSurface
            : palette.elevatedSurface.withValues(alpha: 0.7),
        border: Border(top: BorderSide(color: palette.border)),
      ),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 7),
        child: Wrap(
          spacing: 8,
          runSpacing: 4,
          alignment: WrapAlignment.spaceBetween,
          crossAxisAlignment: WrapCrossAlignment.center,
          children: <Widget>[
            Row(
              mainAxisSize: MainAxisSize.min,
              children: <Widget>[
                Flexible(
                  child: ConstrainedBox(
                    constraints: const BoxConstraints(maxWidth: 150),
                    child: Tooltip(
                      message: observation.car,
                      child: Text(
                        observation.car,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: instrumentMonoStyle(
                          context,
                          fontSize: 12.5,
                          fontWeight: FontWeight.w700,
                        ),
                      ),
                    ),
                  ),
                ),
                const SizedBox(width: 8),
                Flexible(
                  child: SemanticBadge(
                    label: isOperational ? 'Operational' : 'Auxiliary · diag',
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
                ),
              ],
            ),
            Row(
              mainAxisSize: MainAxisSize.min,
              children: <Widget>[
                StatusDot(color: availColor, hollow: availHollow, size: 6.5),
                const SizedBox(width: 5),
                Text(
                  observation.availability.wireValue,
                  style: instrumentMonoStyle(
                    context,
                    fontSize: 11.5,
                    fontWeight:
                        observation.availability ==
                            ObservationAvailability.resolved
                        ? FontWeight.w500
                        : FontWeight.w700,
                    color:
                        observation.availability ==
                            ObservationAvailability.resolved
                        ? palette.subtleText
                        : availColor,
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
