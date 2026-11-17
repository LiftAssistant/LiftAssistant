import 'package:flutter/material.dart';

import '../../api/read_model.dart';
import '../theme/instrument_theme.dart';
import 'semantic_badge.dart';

class OperationalCarCard extends StatelessWidget {
  const OperationalCarCard({
    super.key,
    required this.car,
  });

  final OperationalCar car;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    final (String modeLabel, Color modeFg, Color modeBg) = _describeMode(
      car.operatingMode,
      palette,
    );
    final (String doorLabel, Color doorFg, Color doorBg, IconData doorIcon) =
        _describeDoor(car.door, palette);
    final (
      String motionLabel,
      Color motionFg,
      Color motionBg,
      IconData motionIcon,
    ) = _describeMotion(car.motion, palette);

    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: <Widget>[
            // Header: Logical CarId + Operating Mode
            Row(
              children: <Widget>[
                Container(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 10,
                    vertical: 6,
                  ),
                  decoration: BoxDecoration(
                    color: palette.operationalRoleContainer,
                    borderRadius: BorderRadius.circular(8),
                    border: Border.all(
                      color: palette.operationalRole.withValues(alpha: 0.4),
                    ),
                  ),
                  child: Row(
                    mainAxisSize: MainAxisSize.min,
                    children: <Widget>[
                      Icon(
                        Icons.elevator_outlined,
                        size: 17,
                        color: palette.operationalRole,
                      ),
                      const SizedBox(width: 6),
                      Text(
                        car.id,
                        style: instrumentMonoStyle(
                          context,
                          fontSize: 15,
                          fontWeight: FontWeight.w700,
                          color: palette.operationalRole,
                        ),
                      ),
                    ],
                  ),
                ),
                const SizedBox(width: 8),
                Expanded(
                  child: Text(
                    'Logical Car (Operational Truth)',
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(
                      fontSize: 12,
                      color: palette.subtleText,
                    ),
                  ),
                ),
                SemanticBadge(
                  label: modeLabel,
                  foreground: modeFg,
                  background: modeBg,
                  tooltip: 'LiftAssistant canonical operating mode',
                ),
              ],
            ),
            const SizedBox(height: 14),

            // Location Precision Box (strictly distinguishes Floor vs Continuous)
            _LocationPrecisionPanel(location: car.location),
            const SizedBox(height: 12),

            // Motion & Door Row
            Wrap(
              spacing: 10,
              runSpacing: 10,
              children: <Widget>[
                _AttributeChip(
                  title: 'MOTION',
                  value: motionLabel,
                  icon: motionIcon,
                  foreground: motionFg,
                  background: motionBg,
                ),
                _AttributeChip(
                  title: 'DOOR STATE',
                  value: doorLabel,
                  icon: doorIcon,
                  foreground: doorFg,
                  background: doorBg,
                ),
              ],
            ),
            const SizedBox(height: 12),

            // Reported Destination Evidence Box
            _ReportedDestinationPanel(destination: car.reportedDestination),
          ],
        ),
      ),
    );
  }

  (String, Color, Color) _describeMode(
    CarOperatingMode mode,
    InstrumentPalette palette,
  ) {
    switch (mode) {
      case CarOperatingMode.normal:
        return ('Normal', palette.ok, palette.okContainer);
      case CarOperatingMode.independent:
        return ('Independent', palette.info, palette.infoContainer);
      case CarOperatingMode.inspection:
        return ('Inspection', palette.caution, palette.cautionContainer);
      case CarOperatingMode.fireService:
        return ('FireService', palette.danger, palette.dangerContainer);
      case CarOperatingMode.outOfService:
        return ('OutOfService', palette.neutral, palette.neutralContainer);
    }
  }

  (String, Color, Color, IconData) _describeMotion(
    CarMotion motion,
    InstrumentPalette palette,
  ) {
    switch (motion) {
      case StoppedMotion():
        return (
          'Stopped',
          palette.neutral,
          palette.neutralContainer,
          Icons.pause_circle_outline,
        );
      case MovingMotion(:final direction):
        return direction == MotionDirection.up
            ? (
                'Moving Up',
                palette.info,
                palette.infoContainer,
                Icons.north_rounded,
              )
            : (
                'Moving Down',
                palette.info,
                palette.infoContainer,
                Icons.south_rounded,
              );
    }
  }

  (String, Color, Color, IconData) _describeDoor(
    CarDoorState door,
    InstrumentPalette palette,
  ) {
    switch (door) {
      case CarDoorState.closed:
        return (
          'Closed',
          palette.neutral,
          palette.neutralContainer,
          Icons.sensor_door_outlined,
        );
      case CarDoorState.opening:
        return (
          'Opening',
          palette.info,
          palette.infoContainer,
          Icons.open_in_full_rounded,
        );
      case CarDoorState.open:
        return (
          'Open',
          palette.ok,
          palette.okContainer,
          Icons.meeting_room_outlined,
        );
      case CarDoorState.closing:
        return (
          'Closing',
          palette.info,
          palette.infoContainer,
          Icons.close_fullscreen_rounded,
        );
      case CarDoorState.obstructed:
        return (
          'Obstructed',
          palette.caution,
          palette.cautionContainer,
          Icons.report_problem_outlined,
        );
      case CarDoorState.fault:
        return (
          'Fault',
          palette.danger,
          palette.dangerContainer,
          Icons.error_outline,
        );
    }
  }
}

class _LocationPrecisionPanel extends StatelessWidget {
  const _LocationPrecisionPanel({required this.location});

  final CarLocation location;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;

    switch (location) {
      case FloorLocation(:final floor):
        return Container(
          padding: const EdgeInsets.all(12),
          decoration: BoxDecoration(
            color: palette.elevatedSurface,
            borderRadius: BorderRadius.circular(8),
            border: Border.all(color: palette.border),
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: <Widget>[
              Row(
                children: <Widget>[
                  Expanded(
                    child: Text(
                      'LOCATION EVIDENCE',
                      style: TextStyle(
                        fontSize: 11,
                        fontWeight: FontWeight.w700,
                        letterSpacing: 0.6,
                        color: palette.subtleText,
                      ),
                    ),
                  ),
                  SemanticBadge(
                    label: 'FLOOR ASSOCIATION',
                    foreground: palette.caution,
                    background: palette.cautionContainer,
                    icon: Icons.layers_outlined,
                  ),
                ],
              ),
              const SizedBox(height: 8),
              Text(
                'Floor $floor',
                style: instrumentMonoStyle(
                  context,
                  fontSize: 20,
                  fontWeight: FontWeight.w700,
                ),
              ),
              const SizedBox(height: 4),
              Text(
                'Discrete floor association only (kind: floor). Host does not hold continuous coordinate evidence for this car.',
                style: TextStyle(
                  fontSize: 11.5,
                  height: 1.3,
                  color: palette.subtleText,
                ),
              ),
            ],
          ),
        );

      case ContinuousLocation(:final floorPosition, :final nearestFloor):
        final double delta = floorPosition - nearestFloor;
        final String signedDelta = delta >= 0
            ? '+${delta.toStringAsFixed(2)}'
            : delta.toStringAsFixed(2);
        return Container(
          padding: const EdgeInsets.all(12),
          decoration: BoxDecoration(
            color: palette.elevatedSurface,
            borderRadius: BorderRadius.circular(8),
            border: Border.all(color: palette.border),
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: <Widget>[
              Row(
                children: <Widget>[
                  Expanded(
                    child: Text(
                      'LOCATION EVIDENCE',
                      style: TextStyle(
                        fontSize: 11,
                        fontWeight: FontWeight.w700,
                        letterSpacing: 0.6,
                        color: palette.subtleText,
                      ),
                    ),
                  ),
                  SemanticBadge(
                    label: 'CONTINUOUS POSITION',
                    foreground: palette.ok,
                    background: palette.okContainer,
                    icon: Icons.straighten_outlined,
                  ),
                ],
              ),
              const SizedBox(height: 8),
              Wrap(
                spacing: 14,
                runSpacing: 6,
                crossAxisAlignment: WrapCrossAlignment.center,
                children: <Widget>[
                  Text(
                    floorPosition.toStringAsFixed(2),
                    style: instrumentMonoStyle(
                      context,
                      fontSize: 20,
                      fontWeight: FontWeight.w700,
                    ),
                  ),
                  SemanticBadge(
                    label: 'Nearest Floor $nearestFloor ($signedDelta)',
                    foreground: palette.info,
                    background: palette.infoContainer,
                    monospace: true,
                  ),
                ],
              ),
              const SizedBox(height: 4),
              Text(
                'Genuine continuous position evidence held by Host (kind: continuous).',
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
}

class _AttributeChip extends StatelessWidget {
  const _AttributeChip({
    required this.title,
    required this.value,
    required this.icon,
    required this.foreground,
    required this.background,
  });

  final String title;
  final String value;
  final IconData icon;
  final Color foreground;
  final Color background;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 8),
      decoration: BoxDecoration(
        color: palette.elevatedSurface,
        borderRadius: BorderRadius.circular(8),
        border: Border.all(color: palette.border),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: <Widget>[
          Text(
            '$title:',
            style: TextStyle(
              fontSize: 11,
              fontWeight: FontWeight.w700,
              letterSpacing: 0.5,
              color: palette.subtleText,
            ),
          ),
          const SizedBox(width: 8),
          SemanticBadge(
            label: value,
            foreground: foreground,
            background: background,
            icon: icon,
          ),
        ],
      ),
    );
  }
}

class _ReportedDestinationPanel extends StatelessWidget {
  const _ReportedDestinationPanel({required this.destination});

  final ReportedDestination destination;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    final (String valueText, String detailText, Color fg, Color bg) =
        switch (destination) {
          KnownReportedDestination(:final floor) => (
            'Floor $floor',
            'External source reports destination Floor $floor',
            palette.info,
            palette.infoContainer,
          ),
          NoReportedDestination() => (
            'None',
            'External source explicitly reports no destination',
            palette.neutral,
            palette.neutralContainer,
          ),
          UnknownReportedDestination() => (
            'Unknown',
            'Destination not reported or unestablished by source',
            palette.caution,
            palette.cautionContainer,
          ),
        };

    return Container(
      padding: const EdgeInsets.all(10),
      decoration: BoxDecoration(
        color: palette.elevatedSurface,
        borderRadius: BorderRadius.circular(8),
        border: Border.all(color: palette.border),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: <Widget>[
          Row(
            children: <Widget>[
              Expanded(
                child: Text(
                  'REPORTED DESTINATION (SOURCE EVIDENCE)',
                  style: TextStyle(
                    fontSize: 10.5,
                    fontWeight: FontWeight.w700,
                    letterSpacing: 0.6,
                    color: palette.subtleText,
                  ),
                ),
              ),
              SemanticBadge(
                label: valueText,
                foreground: fg,
                background: bg,
                monospace: true,
              ),
            ],
          ),
          const SizedBox(height: 5),
          Text(
            '$detailText. Not a LiftAssistant desired target, call assignment, or command.',
            style: TextStyle(
              fontSize: 11,
              height: 1.3,
              color: palette.subtleText,
            ),
          ),
        ],
      ),
    );
  }
}
