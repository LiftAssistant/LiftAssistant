import 'package:flutter/material.dart';

import '../../api/read_model.dart';
import '../theme/instrument_theme.dart';
import 'semantic_badge.dart';

class SourceHealthCard extends StatelessWidget {
  const SourceHealthCard({
    super.key,
    required this.source,
  });

  final SourceHealth source;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;

    final (
      String sessionLabel,
      String? sessionId,
      Color sessionFg,
      Color sessionBg,
    ) = switch (source.session) {
      InactiveSourceSession() => (
        'Inactive',
        null,
        palette.neutral,
        palette.neutralContainer,
      ),
      ActiveSourceSession(:final session) => (
        'Active',
        session,
        palette.ok,
        palette.okContainer,
      ),
    };

    final (String transportLabel, Color transportFg, Color transportBg) =
        switch (source.transport) {
          TransportState.unknown => (
            'Unknown',
            palette.neutral,
            palette.neutralContainer,
          ),
          TransportState.subscriberCreated => (
            'Subscriber Created',
            palette.info,
            palette.infoContainer,
          ),
          TransportState.discoveryObserved => (
            'Discovery Observed',
            palette.ok,
            palette.okContainer,
          ),
        };

    final (String payloadLabel, Color payloadFg, Color payloadBg) =
        switch (source.payload) {
          PayloadActivity.awaiting => (
            'Awaiting Payload',
            palette.caution,
            palette.cautionContainer,
          ),
          PayloadActivity.active => (
            'Active Payload',
            palette.ok,
            palette.okContainer,
          ),
          PayloadActivity.stale => (
            'Stale Payload',
            palette.danger,
            palette.dangerContainer,
          ),
        };

    final bool discoveryWithoutActivePayload =
        source.transport == TransportState.discoveryObserved &&
        source.payload != PayloadActivity.active;

    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: <Widget>[
            // Source Identity Header
            Row(
              children: <Widget>[
                Icon(Icons.hub_outlined, size: 18, color: palette.info),
                const SizedBox(width: 8),
                Expanded(
                  child: Text(
                    source.source,
                    style: instrumentMonoStyle(
                      context,
                      fontSize: 15,
                      fontWeight: FontWeight.w700,
                    ),
                  ),
                ),
                Text(
                  '${source.observations.length} binding(s)',
                  style: TextStyle(fontSize: 12, color: palette.subtleText),
                ),
              ],
            ),
            const SizedBox(height: 12),

            // Separate Session / Transport / Payload Health Layers
            Wrap(
              spacing: 10,
              runSpacing: 10,
              children: <Widget>[
                _SourceLayerBox(
                  layerTitle: 'SOURCE SESSION',
                  badgeLabel: sessionLabel,
                  detail: sessionId ?? 'No active adapter session token',
                  monospaceDetail: sessionId != null,
                  foreground: sessionFg,
                  background: sessionBg,
                ),
                _SourceLayerBox(
                  layerTitle: 'TRANSPORT DISCOVERY',
                  badgeLabel: transportLabel,
                  detail: source.transport.wireValue,
                  monospaceDetail: true,
                  foreground: transportFg,
                  background: transportBg,
                ),
                _SourceLayerBox(
                  layerTitle: 'PAYLOAD ACTIVITY',
                  badgeLabel: payloadLabel,
                  detail: source.payload.wireValue,
                  monospaceDetail: true,
                  foreground: payloadFg,
                  background: payloadBg,
                ),
              ],
            ),

            if (discoveryWithoutActivePayload) ...<Widget>[
              const SizedBox(height: 10),
              Container(
                padding: const EdgeInsets.all(9),
                decoration: BoxDecoration(
                  color: palette.cautionContainer,
                  borderRadius: BorderRadius.circular(7),
                  border: Border.all(
                    color: palette.caution.withValues(alpha: 0.4),
                  ),
                ),
                child: Row(
                  children: <Widget>[
                    Icon(
                      Icons.info_outline,
                      size: 15,
                      color: palette.caution,
                    ),
                    const SizedBox(width: 8),
                    Expanded(
                      child: Text(
                        'Transport discovery observed does NOT imply usable payload (${source.payload.wireValue}).',
                        style: const TextStyle(fontSize: 11.5),
                      ),
                    ),
                  ],
                ),
              ),
            ],

            const SizedBox(height: 14),
            Text(
              'BOUND LOGICAL CARS & AUTHORITY ROLES',
              style: TextStyle(
                fontSize: 11,
                fontWeight: FontWeight.w700,
                letterSpacing: 0.6,
                color: palette.subtleText,
              ),
            ),
            const SizedBox(height: 8),
            if (source.observations.isEmpty)
              Text(
                'No bound logical cars reported for this source.',
                style: TextStyle(fontSize: 12, color: palette.subtleText),
              )
            else
              Column(
                children: <Widget>[
                  for (final SourceObservation obs in source.observations)
                    Padding(
                      padding: const EdgeInsets.only(bottom: 8),
                      child: _ObservationBindingRow(observation: obs),
                    ),
                ],
              ),
          ],
        ),
      ),
    );
  }
}

class _SourceLayerBox extends StatelessWidget {
  const _SourceLayerBox({
    required this.layerTitle,
    required this.badgeLabel,
    required this.detail,
    required this.foreground,
    required this.background,
    this.monospaceDetail = false,
  });

  final String layerTitle;
  final String badgeLabel;
  final String detail;
  final Color foreground;
  final Color background;
  final bool monospaceDetail;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    return ConstrainedBox(
      constraints: const BoxConstraints(minWidth: 180, maxWidth: 280),
      child: Container(
        padding: const EdgeInsets.all(10),
        decoration: BoxDecoration(
          color: palette.elevatedSurface,
          borderRadius: BorderRadius.circular(8),
          border: Border.all(color: palette.border),
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: <Widget>[
            Text(
              layerTitle,
              style: TextStyle(
                fontSize: 10.5,
                fontWeight: FontWeight.w700,
                letterSpacing: 0.6,
                color: palette.subtleText,
              ),
            ),
            const SizedBox(height: 6),
            SemanticBadge(
              label: badgeLabel,
              foreground: foreground,
              background: background,
            ),
            const SizedBox(height: 5),
            Text(
              detail,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: monospaceDetail
                  ? instrumentMonoStyle(
                      context,
                      fontSize: 11.5,
                      fontWeight: FontWeight.w500,
                      color: palette.subtleText,
                    )
                  : TextStyle(fontSize: 11.5, color: palette.subtleText),
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

    final (String availLabel, Color availFg, Color availBg) =
        switch (observation.availability) {
          ObservationAvailability.resolved => (
            'Resolved',
            palette.ok,
            palette.okContainer,
          ),
          ObservationAvailability.awaiting => (
            'Awaiting',
            palette.neutral,
            palette.neutralContainer,
          ),
          ObservationAvailability.incomplete => (
            'Incomplete',
            palette.caution,
            palette.cautionContainer,
          ),
          ObservationAvailability.unresolved => (
            'Unresolved',
            palette.caution,
            palette.cautionContainer,
          ),
          ObservationAvailability.revoked => (
            'Revoked',
            palette.danger,
            palette.dangerContainer,
          ),
        };

    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
      decoration: BoxDecoration(
        color: palette.elevatedSurface,
        borderRadius: BorderRadius.circular(8),
        border: Border.all(color: palette.border),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: <Widget>[
          Wrap(
            spacing: 8,
            runSpacing: 6,
            crossAxisAlignment: WrapCrossAlignment.center,
            children: <Widget>[
              Text(
                'Car ${observation.car}',
                style: instrumentMonoStyle(
                  context,
                  fontSize: 13.5,
                  fontWeight: FontWeight.w700,
                ),
              ),
              SemanticBadge(
                label: isOperational ? 'OPERATIONAL' : 'AUXILIARY',
                foreground: isOperational
                    ? palette.operationalRole
                    : palette.auxiliaryRole,
                background: isOperational
                    ? palette.operationalRoleContainer
                    : palette.auxiliaryRoleContainer,
                outlined: !isOperational,
                icon: isOperational
                    ? Icons.verified_user_outlined
                    : Icons.visibility_outlined,
              ),
              SemanticBadge(
                label: 'Availability: $availLabel',
                foreground: availFg,
                background: availBg,
              ),
            ],
          ),
          const SizedBox(height: 5),
          Text(
            isOperational
                ? 'Operational authority: permitted to enter LiftAssistant operational truth when resolved.'
                : 'Auxiliary role: inspectable and diagnostic only — excluded from operational truth (never automatic failover/backup).',
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
