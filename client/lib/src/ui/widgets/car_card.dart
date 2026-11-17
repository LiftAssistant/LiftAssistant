import 'package:flutter/material.dart';

import '../../api/read_model.dart';
import '../theme/instrument_theme.dart';
import 'semantic_badge.dart';

class OperationalCarCard extends StatelessWidget {
  const OperationalCarCard({super.key, required this.car});

  final OperationalCar car;

  Color? _leftStripeColor(InstrumentPalette palette) {
    if (car.operatingMode == CarOperatingMode.fireService ||
        car.door == CarDoorState.fault) {
      return palette.danger;
    }
    if (car.operatingMode == CarOperatingMode.inspection ||
        car.door == CarDoorState.obstructed) {
      return palette.caution;
    }
    if (car.motion is MovingMotion) {
      return palette.info;
    }
    return null;
  }

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    final Color? stripe = _leftStripeColor(palette);

    return DecoratedBox(
      decoration: BoxDecoration(
        color: palette.surface,
        borderRadius: BorderRadius.circular(6),
        border: Border.all(color: palette.border),
      ),
      child: ClipRRect(
        borderRadius: BorderRadius.circular(5),
        child: DecoratedBox(
          position: DecorationPosition.foreground,
          decoration: BoxDecoration(
            border: stripe != null
                ? Border(left: BorderSide(color: stripe, width: 2.5))
                : null,
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            mainAxisSize: MainAxisSize.min,
            children: <Widget>[
              Padding(
                padding: const EdgeInsets.fromLTRB(12, 10, 12, 11),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: <Widget>[
                    // Header: Logical CarId + Operating Mode
                    ConstrainedBox(
                      constraints: const BoxConstraints(minHeight: 22),
                      child: Row(
                        children: <Widget>[
                          Expanded(
                            child: Tooltip(
                              message: 'Logical CarId: ${car.id}',
                              child: Text(
                                car.id,
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                                style: instrumentMonoStyle(
                                  context,
                                  fontSize: 14,
                                  fontWeight: FontWeight.w700,
                                ),
                              ),
                            ),
                          ),
                          const SizedBox(width: 8),
                          _ModeReadout(mode: car.operatingMode),
                        ],
                      ),
                    ),
                    const SizedBox(height: 10),

                    // Hero: Door/Motion Cabin Schematic + Honest Location Precision
                    ConstrainedBox(
                      constraints: const BoxConstraints(minHeight: 42),
                      child: Row(
                        crossAxisAlignment: CrossAxisAlignment.center,
                        children: <Widget>[
                          _CarApertureGraphic(
                            door: car.door,
                            motion: car.motion,
                          ),
                          const SizedBox(width: 12),
                          Expanded(
                            child: _LocationReadout(location: car.location),
                          ),
                        ],
                      ),
                    ),
                  ],
                ),
              ),

              // Compact 3-cell bottom telemetry strip (zero nested boxes)
              DecoratedBox(
                decoration: BoxDecoration(
                  color: palette.elevatedSurface,
                  border: Border(top: BorderSide(color: palette.border)),
                ),
                child: Padding(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 12,
                    vertical: 7.5,
                  ),
                  child: Wrap(
                    spacing: 14,
                    runSpacing: 5,
                    crossAxisAlignment: WrapCrossAlignment.center,
                    children: <Widget>[
                      _MotionCell(motion: car.motion),
                      _DoorCell(door: car.door),
                      _ReportedDestinationCell(
                        destination: car.reportedDestination,
                      ),
                    ],
                  ),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _ModeReadout extends StatelessWidget {
  const _ModeReadout({required this.mode});

  final CarOperatingMode mode;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    switch (mode) {
      case CarOperatingMode.normal:
        return Tooltip(
          message: 'Canonical operating mode: Normal',
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: <Widget>[
              StatusDot(color: palette.ok, size: 6.5),
              const SizedBox(width: 5),
              Text(
                'Normal',
                style: TextStyle(
                  fontSize: 11.5,
                  fontWeight: FontWeight.w600,
                  color: palette.subtleText,
                ),
              ),
            ],
          ),
        );
      case CarOperatingMode.independent:
        return SemanticBadge(
          label: 'Independent',
          foreground: palette.info,
          background: palette.infoContainer,
          tooltip: 'Canonical operating mode: Independent',
        );
      case CarOperatingMode.inspection:
        return SemanticBadge(
          label: 'Inspection',
          foreground: palette.caution,
          background: palette.cautionContainer,
          tooltip: 'Canonical operating mode: Inspection',
        );
      case CarOperatingMode.fireService:
        return SemanticBadge(
          label: 'FireService',
          foreground: palette.danger,
          background: palette.dangerContainer,
          tooltip: 'Canonical operating mode: FireService',
        );
      case CarOperatingMode.outOfService:
        return SemanticBadge(
          label: 'OutOfService',
          foreground: palette.neutral,
          background: palette.neutralContainer,
          outlined: true,
          tooltip: 'Canonical operating mode: OutOfService',
        );
    }
  }
}

class _LocationReadout extends StatelessWidget {
  const _LocationReadout({required this.location});

  final CarLocation location;

  static double _normalizeCents(double raw) {
    final double rounded = (raw * 100).roundToDouble() / 100.0;
    return rounded == 0.0 ? 0.0 : rounded;
  }

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;

    switch (location) {
      case FloorLocation(:final floor):
        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          mainAxisSize: MainAxisSize.min,
          children: <Widget>[
            Wrap(
              spacing: 8,
              runSpacing: 4,
              crossAxisAlignment: WrapCrossAlignment.center,
              children: <Widget>[
                Text(
                  'Floor $floor',
                  style: instrumentMonoStyle(
                    context,
                    fontSize: 19,
                    fontWeight: FontWeight.w700,
                  ),
                ),
                SemanticBadge(
                  label: 'Floor ref',
                  foreground: palette.subtleText,
                  background: palette.neutralContainer,
                  outlined: true,
                  tooltip: 'Discrete floor association only (kind: floor). Host holds no continuous coordinate for this car.',
                ),
              ],
            ),
          ],
        );

      case ContinuousLocation(:final floorPosition, :final nearestFloor):
        final double normalizedPos = _normalizeCents(floorPosition);
        final double rawDelta = floorPosition - nearestFloor;
        final double normalizedDelta = _normalizeCents(rawDelta);
        final String signedDelta = normalizedDelta >= 0
            ? '+${normalizedDelta.toStringAsFixed(2)}'
            : normalizedDelta.toStringAsFixed(2);
        // Clamp visual offset indicator to [-0.5, +0.5] around nearestFloor.
        final double clampedFraction =
            ((rawDelta.clamp(-0.5, 0.5)) + 0.5); // 0.0 .. 1.0

        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          mainAxisSize: MainAxisSize.min,
          children: <Widget>[
            Wrap(
              spacing: 8,
              runSpacing: 4,
              crossAxisAlignment: WrapCrossAlignment.center,
              children: <Widget>[
                Text(
                  normalizedPos.toStringAsFixed(2),
                  style: instrumentMonoStyle(
                    context,
                    fontSize: 19,
                    fontWeight: FontWeight.w700,
                  ),
                ),
                Text(
                  'Nearest Floor $nearestFloor ($signedDelta)',
                  style: instrumentMonoStyle(
                    context,
                    fontSize: 11.5,
                    fontWeight: FontWeight.w500,
                    color: palette.subtleText,
                  ),
                ),
                SemanticBadge(
                  label: 'Continuous',
                  foreground: palette.info,
                  background: palette.infoContainer,
                  tooltip: 'Genuine continuous position evidence held by Host (kind: continuous).',
                ),
              ],
            ),
            const SizedBox(height: 6),
            _ContinuousOffsetGauge(fraction: clampedFraction),
          ],
        );
    }
  }
}

/// Subtle landing-offset scale rendered ONLY when [ContinuousLocation] is present.
class _ContinuousOffsetGauge extends StatelessWidget {
  const _ContinuousOffsetGauge({required this.fraction});

  final double fraction;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    return ConstrainedBox(
      constraints: const BoxConstraints(maxWidth: 160),
      child: SizedBox(
        height: 8,
        child: LayoutBuilder(
          builder: (BuildContext context, BoxConstraints constraints) {
            final double width = constraints.maxWidth;
            final double markerX = (width - 6) * fraction;
            return Stack(
              alignment: Alignment.centerLeft,
              children: <Widget>[
                Container(height: 2, width: width, color: palette.border),
                Positioned(
                  left: (width / 2) - 0.5,
                  top: 1,
                  bottom: 1,
                  child: Container(width: 1, color: palette.mutedText),
                ),
                AnimatedPositioned(
                  duration: const Duration(milliseconds: 140),
                  curve: Curves.easeOutCubic,
                  left: markerX,
                  child: Container(
                    width: 6,
                    height: 6,
                    decoration: BoxDecoration(
                      color: palette.info,
                      borderRadius: BorderRadius.circular(1.5),
                    ),
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

/// Geometric cabin + split-door schematic reflecting [CarDoorState] and [CarMotion].
class _CarApertureGraphic extends StatelessWidget {
  const _CarApertureGraphic({required this.door, required this.motion});

  final CarDoorState door;
  final CarMotion motion;

  double _doorGapFraction() {
    switch (door) {
      case CarDoorState.closed:
        return 0.04;
      case CarDoorState.opening:
      case CarDoorState.closing:
        return 0.44;
      case CarDoorState.open:
        return 0.80;
      case CarDoorState.obstructed:
        return 0.56;
      case CarDoorState.fault:
        return 0.34;
    }
  }

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    final Color frameColor = switch (door) {
      CarDoorState.fault => palette.danger,
      CarDoorState.obstructed => palette.caution,
      CarDoorState.open => palette.ok,
      CarDoorState.opening || CarDoorState.closing => palette.info,
      CarDoorState.closed => palette.border,
    };
    final Color leafColor = switch (door) {
      CarDoorState.fault => palette.danger.withValues(alpha: 0.35),
      CarDoorState.obstructed => palette.caution.withValues(alpha: 0.35),
      _ => palette.subtleText.withValues(alpha: 0.28),
    };

    final IconData? motionArrow = switch (motion) {
      StoppedMotion() => null,
      MovingMotion(:final direction) =>
        direction == MotionDirection.up
            ? Icons.keyboard_arrow_up_rounded
            : Icons.keyboard_arrow_down_rounded,
    };

    return SizedBox(
      width: 38,
      height: 40,
      child: DecoratedBox(
        decoration: BoxDecoration(
          color: palette.elevatedSurface,
          borderRadius: BorderRadius.circular(4),
          border: Border.all(color: frameColor, width: 1.2),
        ),
        child: Padding(
          padding: const EdgeInsets.all(3),
          child: LayoutBuilder(
            builder: (BuildContext context, BoxConstraints box) {
              final double innerW = box.maxWidth;
              final double gap = innerW * _doorGapFraction();
              final double leafW = ((innerW - gap) / 2).clamp(2.0, innerW / 2);

              return Stack(
                alignment: Alignment.center,
                children: <Widget>[
                  Align(
                    alignment: Alignment.centerLeft,
                    child: AnimatedContainer(
                      duration: const Duration(milliseconds: 150),
                      curve: Curves.easeOutCubic,
                      width: leafW,
                      height: box.maxHeight,
                      decoration: BoxDecoration(
                        color: leafColor,
                        borderRadius: BorderRadius.circular(1.5),
                      ),
                    ),
                  ),
                  Align(
                    alignment: Alignment.centerRight,
                    child: AnimatedContainer(
                      duration: const Duration(milliseconds: 150),
                      curve: Curves.easeOutCubic,
                      width: leafW,
                      height: box.maxHeight,
                      decoration: BoxDecoration(
                        color: leafColor,
                        borderRadius: BorderRadius.circular(1.5),
                      ),
                    ),
                  ),
                  if (door == CarDoorState.fault)
                    Icon(Icons.close_rounded, size: 13, color: palette.danger)
                  else if (door == CarDoorState.obstructed)
                    Icon(
                      Icons.priority_high_rounded,
                      size: 13,
                      color: palette.caution,
                    )
                  else if (motionArrow != null)
                    Icon(motionArrow, size: 16, color: palette.info),
                ],
              );
            },
          ),
        ),
      ),
    );
  }
}

class _MotionCell extends StatelessWidget {
  const _MotionCell({required this.motion});

  final CarMotion motion;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    switch (motion) {
      case StoppedMotion():
        return _CompactKv(
          label: 'Motion',
          value: 'Stopped',
          valueColor: palette.subtleText,
          emphasize: false,
        );
      case MovingMotion(:final direction):
        final bool isUp = direction == MotionDirection.up;
        return Row(
          mainAxisSize: MainAxisSize.min,
          children: <Widget>[
            Text(
              'Motion ',
              style: TextStyle(fontSize: 11, color: palette.mutedText),
            ),
            Icon(
              isUp ? Icons.north_rounded : Icons.south_rounded,
              size: 12,
              color: palette.info,
            ),
            const SizedBox(width: 2),
            Text(
              isUp ? 'Moving Up' : 'Moving Down',
              style: TextStyle(
                fontSize: 11.5,
                fontWeight: FontWeight.w600,
                color: palette.info,
              ),
            ),
          ],
        );
    }
  }
}

class _DoorCell extends StatelessWidget {
  const _DoorCell({required this.door});

  final CarDoorState door;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    final (String text, Color color, bool emphasize) = switch (door) {
      CarDoorState.closed => ('Closed', palette.subtleText, false),
      CarDoorState.opening => ('Opening', palette.info, true),
      CarDoorState.open => ('Open', palette.ok, true),
      CarDoorState.closing => ('Closing', palette.info, true),
      CarDoorState.obstructed => ('Obstructed', palette.caution, true),
      CarDoorState.fault => ('Fault', palette.danger, true),
    };

    return _CompactKv(
      label: 'Door',
      value: text,
      valueColor: color,
      emphasize: emphasize,
    );
  }
}

class _ReportedDestinationCell extends StatelessWidget {
  const _ReportedDestinationCell({required this.destination});

  final ReportedDestination destination;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    final (
      String valueText,
      Color valueColor,
      bool emphasize,
    ) = switch (destination) {
      KnownReportedDestination(:final floor) => (
        'Floor $floor',
        palette.info,
        true,
      ),
      NoReportedDestination() => ('None', palette.subtleText, false),
      UnknownReportedDestination() => ('Unknown', palette.caution, true),
    };

    return Tooltip(
      message: 'External source reported destination evidence — NOT a LiftAssistant desired target, call assignment, or command.',
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: <Widget>[
          Text(
            'Source dest ',
            style: TextStyle(fontSize: 11, color: palette.mutedText),
          ),
          Text(
            valueText,
            style: instrumentMonoStyle(
              context,
              fontSize: 11.5,
              fontWeight: emphasize ? FontWeight.w700 : FontWeight.w500,
              color: valueColor,
            ),
          ),
        ],
      ),
    );
  }
}

class _CompactKv extends StatelessWidget {
  const _CompactKv({
    required this.label,
    required this.value,
    required this.valueColor,
    required this.emphasize,
  });

  final String label;
  final String value;
  final Color valueColor;
  final bool emphasize;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: <Widget>[
        Text(
          '$label ',
          style: TextStyle(fontSize: 11, color: palette.mutedText),
        ),
        Text(
          value,
          style: TextStyle(
            fontSize: 11.5,
            fontWeight: emphasize ? FontWeight.w700 : FontWeight.w500,
            color: valueColor,
          ),
        ),
      ],
    );
  }
}
