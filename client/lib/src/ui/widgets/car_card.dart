import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';

import '../../api/read_model.dart';
import '../theme/instrument_theme.dart';
import 'semantic_badge.dart';

/// A logical car with an explicit `Operational` observation binding in the
/// current snapshot that has no currently resolved entry in `operational_cars`.
///
/// This represents current observation availability only — never an elevator
/// equipment fault, `OutOfService` mode, timeout inference, or Auxiliary fallback.
@immutable
final class UnavailableOperationalCar {
  const UnavailableOperationalCar({
    required this.carId,
    required this.source,
    required this.session,
    required this.availability,
    required this.payload,
  });

  final String carId;
  final String source;
  final SourceSession session;
  final ObservationAvailability availability;
  final PayloadActivity payload;

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      (other is UnavailableOperationalCar &&
          other.carId == carId &&
          other.source == source &&
          other.session == session &&
          other.availability == availability &&
          other.payload == payload);

  @override
  int get hashCode =>
      Object.hash(carId, source, session, availability, payload);
}

@immutable
sealed class OperationalCarSlot {
  const OperationalCarSlot();

  String get carId;
}

@immutable
final class ResolvedCarSlot extends OperationalCarSlot {
  const ResolvedCarSlot(this.car);

  final OperationalCar car;

  @override
  String get carId => car.id;

  @override
  bool operator ==(Object other) =>
      identical(this, other) || (other is ResolvedCarSlot && other.car == car);

  @override
  int get hashCode => Object.hash(ResolvedCarSlot, car);
}

@immutable
final class UnavailableCarSlot extends OperationalCarSlot {
  const UnavailableCarSlot(this.car);

  final UnavailableOperationalCar car;

  @override
  String get carId => car.carId;

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      (other is UnavailableCarSlot && other.car == car);

  @override
  int get hashCode => Object.hash(UnavailableCarSlot, car);
}

@immutable
final class ConfiguredOperationalCarsView {
  const ConfiguredOperationalCarsView({
    required this.resolvedCars,
    required this.unavailableCars,
    required this.slots,
  });

  final List<OperationalCar> resolvedCars;
  final List<UnavailableOperationalCar> unavailableCars;
  final List<OperationalCarSlot> slots;

  bool get hasResolvedCars => resolvedCars.isNotEmpty;
  int get totalCount => slots.length;

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      (other is ConfiguredOperationalCarsView &&
          listEquals(other.resolvedCars, resolvedCars) &&
          listEquals(other.unavailableCars, unavailableCars) &&
          listEquals(other.slots, slots));

  @override
  int get hashCode => Object.hash(
    Object.hashAll(resolvedCars),
    Object.hashAll(unavailableCars),
    Object.hashAll(slots),
  );
}

/// Projects the Cars display list strictly from the current [ReadModelSnapshot]:
///
/// - Deduplicates resolved cars by logical [OperationalCar.id].
/// - Computes `configured Operational-bound logical cars MINUS current operational_cars`.
/// - Ignores `Auxiliary` bindings completely (never promotes Auxiliary to operational
///   truth or fallback, and never introduces Auxiliary-only cars into Cars).
/// - Deduplicates unavailable cars by logical `car` identity.
ConfiguredOperationalCarsView projectConfiguredOperationalCars(
  ReadModelSnapshot snapshot,
) {
  final List<OperationalCar> resolvedCars = <OperationalCar>[];
  final Set<String> resolvedIds = <String>{};
  for (final OperationalCar car in snapshot.operationalCars) {
    if (resolvedIds.add(car.id)) {
      resolvedCars.add(car);
    }
  }

  final Map<String, UnavailableOperationalCar> unavailableByCarId =
      <String, UnavailableOperationalCar>{};
  for (final SourceHealth src in snapshot.sources) {
    for (final SourceObservation obs in src.observations) {
      if (obs.role != BindingRole.operational) {
        continue;
      }
      if (resolvedIds.contains(obs.car)) {
        continue;
      }
      unavailableByCarId.putIfAbsent(
        obs.car,
        () => UnavailableOperationalCar(
          carId: obs.car,
          source: src.source,
          session: src.session,
          availability: obs.availability,
          payload: src.payload,
        ),
      );
    }
  }

  final List<UnavailableOperationalCar> unavailableCars =
      unavailableByCarId.values.toList()
        ..sort((UnavailableOperationalCar a, UnavailableOperationalCar b) {
          return a.carId.compareTo(b.carId);
        });

  final List<OperationalCarSlot> slots;
  if (unavailableCars.isEmpty) {
    slots = <OperationalCarSlot>[
      for (final OperationalCar car in resolvedCars) ResolvedCarSlot(car),
    ];
  } else {
    slots =
        <OperationalCarSlot>[
          for (final OperationalCar car in resolvedCars) ResolvedCarSlot(car),
          for (final UnavailableOperationalCar car in unavailableCars)
            UnavailableCarSlot(car),
        ]..sort((OperationalCarSlot a, OperationalCarSlot b) {
          return a.carId.compareTo(b.carId);
        });
  }

  return ConfiguredOperationalCarsView(
    resolvedCars: List<OperationalCar>.unmodifiable(resolvedCars),
    unavailableCars: List<UnavailableOperationalCar>.unmodifiable(
      unavailableCars,
    ),
    slots: List<OperationalCarSlot>.unmodifiable(slots),
  );
}

class OperationalCarCard extends StatelessWidget {
  const OperationalCarCard({
    super.key,
    required this.car,
    this.stretchToRowHeight = false,
    this.useTabularFooter = true,
  });

  final OperationalCar car;
  final bool stretchToRowHeight;
  final bool useTabularFooter;

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
            mainAxisSize: stretchToRowHeight
                ? MainAxisSize.max
                : MainAxisSize.min,
            mainAxisAlignment: stretchToRowHeight
                ? MainAxisAlignment.spaceBetween
                : MainAxisAlignment.start,
            children: <Widget>[
              Padding(
                padding: const EdgeInsets.fromLTRB(12, 10, 12, 10),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  mainAxisSize: MainAxisSize.min,
                  children: <Widget>[
                    // Header: Logical CarId + Operating Mode
                    Row(
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
                                fontSize: 13.5,
                                fontWeight: FontWeight.w700,
                              ),
                            ),
                          ),
                        ),
                        const SizedBox(width: 8),
                        ConstrainedBox(
                          constraints: const BoxConstraints(maxWidth: 156),
                          child: _ModeReadout(mode: car.operatingMode),
                        ),
                      ],
                    ),
                    const SizedBox(height: 9),

                    // Primary readout: Hoistway/Portal Indicator + Honest Location Precision
                    Row(
                      crossAxisAlignment: CrossAxisAlignment.center,
                      children: <Widget>[
                        _CarApertureGraphic(door: car.door, motion: car.motion),
                        const SizedBox(width: 11),
                        Expanded(
                          child: _LocationReadout(location: car.location),
                        ),
                      ],
                    ),
                  ],
                ),
              ),

              // Bottom tabular telemetry strip (aligned columns across cars; overflow-safe at 2.0x scale)
              _CarTelemetryFooter(
                motion: car.motion,
                door: car.door,
                reportedDestination: car.reportedDestination,
                useTabularColumns: useTabularFooter,
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
              StatusDot(color: palette.ok, size: 6),
              const SizedBox(width: 5),
              Flexible(
                child: Text(
                  'Normal',
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                    fontSize: 11.5,
                    fontWeight: FontWeight.w500,
                    color: palette.subtleText,
                  ),
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
        return Wrap(
          spacing: 8,
          runSpacing: 4,
          crossAxisAlignment: WrapCrossAlignment.center,
          children: <Widget>[
            Text(
              'Floor $floor',
              style: instrumentMonoStyle(
                context,
                fontSize: 18.5,
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
        );

      case ContinuousLocation(:final floorPosition, :final nearestFloor):
        final double normalizedPos = _normalizeCents(floorPosition);
        final double rawDelta = floorPosition - nearestFloor;
        final double normalizedDelta = _normalizeCents(rawDelta);
        final String signedDelta = normalizedDelta >= 0
            ? '+${normalizedDelta.toStringAsFixed(2)}'
            : normalizedDelta.toStringAsFixed(2);
        final bool isClamped = rawDelta < -0.5 || rawDelta > 0.5;
        final double clampedFraction =
            (rawDelta.clamp(-0.5, 0.5)) + 0.5; // 0.0 .. 1.0

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
                    fontSize: 18.5,
                    fontWeight: FontWeight.w700,
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
            const SizedBox(height: 4),
            Wrap(
              spacing: 8,
              runSpacing: 4,
              crossAxisAlignment: WrapCrossAlignment.center,
              children: <Widget>[
                Text(
                  'Nearest Floor $nearestFloor ($signedDelta)',
                  style: instrumentMonoStyle(
                    context,
                    fontSize: 11,
                    fontWeight: FontWeight.w500,
                    color: palette.subtleText,
                  ),
                ),
                _ContinuousOffsetGauge(
                  fraction: clampedFraction,
                  clamped: isClamped,
                  signedDelta: signedDelta,
                ),
              ],
            ),
          ],
        );
    }
  }
}

/// Subtle landing-offset scale (`[-0.5, +0.5]`) rendered ONLY when [ContinuousLocation] is present.
class _ContinuousOffsetGauge extends StatelessWidget {
  const _ContinuousOffsetGauge({
    required this.fraction,
    required this.clamped,
    required this.signedDelta,
  });

  static const double _gaugeWidth = 72;

  final double fraction;
  final bool clamped;
  final String signedDelta;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    final Color markerColor = clamped ? palette.caution : palette.info;
    final double markerX = (_gaugeWidth - 6) * fraction;

    return Tooltip(
      message: clamped
          ? 'Offset $signedDelta exceeds ±0.50 landing window (gauge pinned at boundary).'
          : 'Landing offset $signedDelta within [-0.50, +0.50] window.',
      child: SizedBox(
        width: _gaugeWidth,
        height: 8,
        child: Stack(
          alignment: Alignment.centerLeft,
          children: <Widget>[
            Container(height: 2, width: _gaugeWidth, color: palette.border),
            Positioned(
              left: (_gaugeWidth / 2) - 0.5,
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
                  color: clamped ? Colors.transparent : markerColor,
                  borderRadius: BorderRadius.circular(1.5),
                  border: clamped
                      ? Border.all(color: markerColor, width: 1.5)
                      : null,
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// Architectural elevator portal (transom + split door leaves + landing sill)
/// reflecting [CarDoorState] and [CarMotion] without resembling a media pause icon.
class _CarApertureGraphic extends StatelessWidget {
  const _CarApertureGraphic({required this.door, required this.motion});

  static const double _outerWidth = 38;
  static const double _outerHeight = 42;
  static const double _borderWidth = 1.15;
  static const double _horizontalPad = 3.0;
  static const double _innerWidth =
      _outerWidth - (_borderWidth * 2) - (_horizontalPad * 2);

  final CarDoorState door;
  final CarMotion motion;

  double _doorGapFraction() {
    switch (door) {
      case CarDoorState.closed:
        return 0.0;
      case CarDoorState.opening:
      case CarDoorState.closing:
        return 0.42;
      case CarDoorState.open:
        return 0.78;
      case CarDoorState.obstructed:
        return 0.54;
      case CarDoorState.fault:
        return 0.32;
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
      CarDoorState.closed =>
        motion is MovingMotion ? palette.info : palette.border,
    };
    final Color leafColor = switch (door) {
      CarDoorState.fault => palette.danger.withValues(alpha: 0.28),
      CarDoorState.obstructed => palette.caution.withValues(alpha: 0.28),
      _ => palette.subtleText.withValues(alpha: 0.16),
    };
    final Color transomColor = switch (motion) {
      MovingMotion() => palette.info,
      StoppedMotion() => palette.mutedText.withValues(alpha: 0.45),
    };

    final IconData? motionArrow = switch (motion) {
      StoppedMotion() => null,
      MovingMotion(:final direction) =>
        direction == MotionDirection.up
            ? Icons.keyboard_arrow_up_rounded
            : Icons.keyboard_arrow_down_rounded,
    };

    final double gap = _innerWidth * _doorGapFraction();
    final double leafW = ((_innerWidth - gap) / 2).clamp(2.0, _innerWidth / 2);

    return SizedBox(
      width: _outerWidth,
      height: _outerHeight,
      child: DecoratedBox(
        decoration: BoxDecoration(
          color: palette.elevatedSurface,
          borderRadius: BorderRadius.circular(4),
          border: Border.all(color: frameColor, width: _borderWidth),
        ),
        child: Padding(
          padding: const EdgeInsets.fromLTRB(
            _horizontalPad,
            3,
            _horizontalPad,
            2.5,
          ),
          child: Column(
            children: <Widget>[
              // Hoistway transom indicator bar
              Container(
                height: 2.5,
                width: double.infinity,
                decoration: BoxDecoration(
                  color: transomColor,
                  borderRadius: BorderRadius.circular(1),
                ),
              ),
              const SizedBox(height: 2),
              // Door aperture + leaves
              Expanded(
                child: Stack(
                  alignment: Alignment.center,
                  children: <Widget>[
                    Align(
                      alignment: Alignment.centerLeft,
                      child: AnimatedContainer(
                        duration: const Duration(milliseconds: 150),
                        curve: Curves.easeOutCubic,
                        width: leafW,
                        height: double.infinity,
                        decoration: BoxDecoration(
                          color: leafColor,
                          border: Border(
                            right: BorderSide(
                              color: palette.border,
                              width: 0.8,
                            ),
                          ),
                        ),
                      ),
                    ),
                    Align(
                      alignment: Alignment.centerRight,
                      child: AnimatedContainer(
                        duration: const Duration(milliseconds: 150),
                        curve: Curves.easeOutCubic,
                        width: leafW,
                        height: double.infinity,
                        decoration: BoxDecoration(
                          color: leafColor,
                          border: Border(
                            left: BorderSide(color: palette.border, width: 0.8),
                          ),
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
                      Icon(motionArrow, size: 15, color: palette.info),
                  ],
                ),
              ),
              const SizedBox(height: 1.5),
              // Landing sill threshold line
              Container(height: 1.5, width: double.infinity, color: frameColor),
            ],
          ),
        ),
      ),
    );
  }
}

class _CarTelemetryFooter extends StatelessWidget {
  const _CarTelemetryFooter({
    required this.motion,
    required this.door,
    required this.reportedDestination,
    required this.useTabularColumns,
  });

  final CarMotion motion;
  final CarDoorState door;
  final ReportedDestination reportedDestination;
  final bool useTabularColumns;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    final double textScale = MediaQuery.textScalerOf(context).scale(1.0);

    final (
      String motionText,
      Color motionColor,
      bool motionEmph,
      IconData? motionIcon,
    ) = switch (motion) {
      StoppedMotion() => ('Stopped', palette.subtleText, false, null),
      MovingMotion(:final direction) =>
        direction == MotionDirection.up
            ? ('Moving Up', palette.info, true, Icons.north_rounded)
            : ('Moving Down', palette.info, true, Icons.south_rounded),
    };

    final (String doorText, Color doorColor, bool doorEmph) = switch (door) {
      CarDoorState.closed => ('Closed', palette.subtleText, false),
      CarDoorState.opening => ('Opening', palette.info, true),
      CarDoorState.open => ('Open', palette.ok, true),
      CarDoorState.closing => ('Closing', palette.info, true),
      CarDoorState.obstructed => ('Obstructed', palette.caution, true),
      CarDoorState.fault => ('Fault', palette.danger, true),
    };

    final (
      String destText,
      Color destColor,
      bool destEmph,
    ) = switch (reportedDestination) {
      KnownReportedDestination(:final floor) => (
        'Floor $floor',
        palette.info,
        true,
      ),
      NoReportedDestination() => ('None', palette.subtleText, false),
      UnknownReportedDestination() => ('Unknown', palette.caution, true),
    };

    const String destTooltip =
        'External source reported destination evidence — NOT a LiftAssistant desired target, call assignment, or command.';

    final bool showTabular = useTabularColumns && textScale <= 1.35;

    return DecoratedBox(
      decoration: BoxDecoration(
        color: palette.elevatedSurface,
        border: Border(top: BorderSide(color: palette.border)),
      ),
      child: showTabular
          ? Padding(
              padding: const EdgeInsets.symmetric(
                horizontal: 12,
                vertical: 6.5,
              ),
              child: Row(
                children: <Widget>[
                  Expanded(
                    flex: 10,
                    child: _TabularField(
                      label: 'Motion',
                      value: motionText,
                      valueColor: motionColor,
                      emphasize: motionEmph,
                      leadingIcon: motionIcon,
                    ),
                  ),
                  Container(
                    width: 1,
                    height: 22,
                    margin: const EdgeInsets.symmetric(horizontal: 4.5),
                    color: palette.border,
                  ),
                  Expanded(
                    flex: 7,
                    child: _TabularField(
                      label: 'Door',
                      value: doorText,
                      valueColor: doorColor,
                      emphasize: doorEmph,
                    ),
                  ),
                  Container(
                    width: 1,
                    height: 22,
                    margin: const EdgeInsets.symmetric(horizontal: 4.5),
                    color: palette.border,
                  ),
                  Expanded(
                    flex: 7,
                    child: Tooltip(
                      message: destTooltip,
                      child: _TabularField(
                        label: 'Source dest',
                        value: destText,
                        valueColor: destColor,
                        emphasize: destEmph,
                        monospaceValue: true,
                      ),
                    ),
                  ),
                ],
              ),
            )
          : Padding(
              padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 7),
              child: Wrap(
                spacing: 12,
                runSpacing: 5,
                crossAxisAlignment: WrapCrossAlignment.center,
                children: <Widget>[
                  _SafeInlineKv(
                    label: 'Motion',
                    value: motionText,
                    valueColor: motionColor,
                    emphasize: motionEmph,
                    leadingIcon: motionIcon,
                  ),
                  _SafeInlineKv(
                    label: 'Door',
                    value: doorText,
                    valueColor: doorColor,
                    emphasize: doorEmph,
                  ),
                  Tooltip(
                    message: destTooltip,
                    child: _SafeInlineKv(
                      label: 'Source dest',
                      value: destText,
                      valueColor: destColor,
                      emphasize: destEmph,
                      monospaceValue: true,
                    ),
                  ),
                ],
              ),
            ),
    );
  }
}

class _TabularField extends StatelessWidget {
  const _TabularField({
    required this.label,
    required this.value,
    required this.valueColor,
    required this.emphasize,
    this.leadingIcon,
    this.monospaceValue = false,
  });

  final String label;
  final String value;
  final Color valueColor;
  final bool emphasize;
  final IconData? leadingIcon;
  final bool monospaceValue;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      mainAxisSize: MainAxisSize.min,
      children: <Widget>[
        Text(
          label,
          maxLines: 1,
          overflow: TextOverflow.ellipsis,
          style: TextStyle(fontSize: 10, color: palette.mutedText, height: 1.1),
        ),
        const SizedBox(height: 2),
        Row(
          mainAxisSize: MainAxisSize.min,
          children: <Widget>[
            if (leadingIcon != null) ...<Widget>[
              Icon(leadingIcon, size: 11.5, color: valueColor),
              const SizedBox(width: 2.5),
            ],
            Flexible(
              child: Text(
                value,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: monospaceValue
                    ? instrumentMonoStyle(
                        context,
                        fontSize: 11.5,
                        fontWeight: emphasize
                            ? FontWeight.w700
                            : FontWeight.w500,
                        color: valueColor,
                      )
                    : TextStyle(
                        fontSize: 11.5,
                        fontWeight: emphasize
                            ? FontWeight.w700
                            : FontWeight.w500,
                        color: valueColor,
                      ),
              ),
            ),
          ],
        ),
      ],
    );
  }
}

class _SafeInlineKv extends StatelessWidget {
  const _SafeInlineKv({
    required this.label,
    required this.value,
    required this.valueColor,
    required this.emphasize,
    this.leadingIcon,
    this.monospaceValue = false,
  });

  final String label;
  final String value;
  final Color valueColor;
  final bool emphasize;
  final IconData? leadingIcon;
  final bool monospaceValue;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: <Widget>[
        ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 96),
          child: Text(
            '$label ',
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: TextStyle(fontSize: 11, color: palette.mutedText),
          ),
        ),
        if (leadingIcon != null) ...<Widget>[
          Icon(leadingIcon, size: 11.5, color: valueColor),
          const SizedBox(width: 2),
        ],
        Flexible(
          child: Text(
            value,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: monospaceValue
                ? instrumentMonoStyle(
                    context,
                    fontSize: 11.5,
                    fontWeight: emphasize ? FontWeight.w700 : FontWeight.w500,
                    color: valueColor,
                  )
                : TextStyle(
                    fontSize: 11.5,
                    fontWeight: emphasize ? FontWeight.w700 : FontWeight.w500,
                    color: valueColor,
                  ),
          ),
        ),
      ],
    );
  }
}

/// Renders a configured `Operational`-bound logical car that currently has no
/// resolved `operational_cars` entry in the Host snapshot.
///
/// Deliberately displays zero floor, motion, door, mode, or destination values
/// and never infers timeouts, network failures, controller faults, or
/// `OutOfService` mode from observation availability.
class UnavailableOperationalCarCard extends StatelessWidget {
  const UnavailableOperationalCarCard({
    super.key,
    required this.car,
    this.stretchToRowHeight = false,
    this.useTabularFooter = true,
  });

  final UnavailableOperationalCar car;
  final bool stretchToRowHeight;
  final bool useTabularFooter;

  Color _leftStripeColor(InstrumentPalette palette) {
    return switch (car.availability) {
      ObservationAvailability.awaiting => palette.neutral,
      ObservationAvailability.incomplete ||
      ObservationAvailability.unresolved ||
      ObservationAvailability.revoked ||
      ObservationAvailability.resolved => palette.caution,
    };
  }

  String _detailSummary() {
    return switch (car.availability) {
      ObservationAvailability.awaiting =>
        'Configured Operational binding; awaiting observation state.',
      ObservationAvailability.incomplete =>
        'Configured Operational binding; partial evidence only.',
      ObservationAvailability.unresolved =>
        'Configured Operational binding; observation unresolved.',
      ObservationAvailability.revoked =>
        'Configured Operational binding; observation revoked.',
      ObservationAvailability.resolved =>
        'Configured Operational binding; no current operational state.',
    };
  }

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    final Color stripe = _leftStripeColor(palette);

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
            border: Border(left: BorderSide(color: stripe, width: 2.5)),
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            mainAxisSize: stretchToRowHeight
                ? MainAxisSize.max
                : MainAxisSize.min,
            mainAxisAlignment: stretchToRowHeight
                ? MainAxisAlignment.spaceBetween
                : MainAxisAlignment.start,
            children: <Widget>[
              Padding(
                padding: const EdgeInsets.fromLTRB(12, 10, 12, 10),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  mainAxisSize: MainAxisSize.min,
                  children: <Widget>[
                    // Header: Logical CarId + Observation Availability Badge
                    Row(
                      children: <Widget>[
                        Expanded(
                          child: Tooltip(
                            message: 'Logical CarId: ${car.carId}',
                            child: Text(
                              car.carId,
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: instrumentMonoStyle(
                                context,
                                fontSize: 13.5,
                                fontWeight: FontWeight.w700,
                              ),
                            ),
                          ),
                        ),
                        const SizedBox(width: 8),
                        ConstrainedBox(
                          constraints: const BoxConstraints(maxWidth: 156),
                          child: _UnavailableAvailabilityBadge(
                            availability: car.availability,
                          ),
                        ),
                      ],
                    ),
                    const SizedBox(height: 9),

                    // Primary body: Non-operational placeholder graphic + honest evidence message
                    Row(
                      crossAxisAlignment: CrossAxisAlignment.center,
                      children: <Widget>[
                        _UnavailableApertureGraphic(
                          availability: car.availability,
                        ),
                        const SizedBox(width: 11),
                        Expanded(
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            mainAxisSize: MainAxisSize.min,
                            children: <Widget>[
                              Text(
                                'Current observation unavailable',
                                style: TextStyle(
                                  fontSize: 13,
                                  fontWeight: FontWeight.w600,
                                  color: Theme.of(context)
                                      .colorScheme
                                      .onSurface,
                                ),
                              ),
                              const SizedBox(height: 2.5),
                              Text(
                                _detailSummary(),
                                style: TextStyle(
                                  fontSize: 11.5,
                                  height: 1.3,
                                  color: palette.subtleText,
                                ),
                              ),
                            ],
                          ),
                        ),
                      ],
                    ),
                  ],
                ),
              ),

              // Bottom source/availability context strip (aligned with OperationalCarCard footer)
              _UnavailableCarContextFooter(
                car: car,
                useTabularColumns: useTabularFooter,
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _UnavailableAvailabilityBadge extends StatelessWidget {
  const _UnavailableAvailabilityBadge({required this.availability});

  final ObservationAvailability availability;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    final (
      String label,
      Color fg,
      Color bg,
      Color dotColor,
      bool hollowDot,
    ) = switch (availability) {
      ObservationAvailability.awaiting => (
        'Awaiting',
        palette.subtleText,
        palette.neutralContainer,
        palette.neutral,
        true,
      ),
      ObservationAvailability.incomplete => (
        'Incomplete',
        palette.caution,
        palette.cautionContainer,
        palette.caution,
        true,
      ),
      ObservationAvailability.unresolved => (
        'Unresolved',
        palette.caution,
        palette.cautionContainer,
        palette.caution,
        true,
      ),
      ObservationAvailability.revoked => (
        'Revoked',
        palette.caution,
        palette.cautionContainer,
        palette.caution,
        false,
      ),
      ObservationAvailability.resolved => (
        'Unavailable',
        palette.caution,
        palette.cautionContainer,
        palette.caution,
        true,
      ),
    };

    return SemanticBadge(
      label: label,
      foreground: fg,
      background: bg,
      dotColor: dotColor,
      hollowDot: hollowDot,
      outlined: true,
      tooltip:
          'Observation availability (${availability.wireValue}) describes LiftAssistant evidence state only — NOT an elevator fault or OutOfService mode.',
    );
  }
}

class _UnavailableApertureGraphic extends StatelessWidget {
  const _UnavailableApertureGraphic({required this.availability});

  final ObservationAvailability availability;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    final Color accent = availability == ObservationAvailability.awaiting
        ? palette.mutedText
        : palette.caution;

    return SizedBox(
      width: 38,
      height: 42,
      child: DecoratedBox(
        decoration: BoxDecoration(
          color: palette.elevatedSurface,
          borderRadius: BorderRadius.circular(4),
          border: Border.all(color: palette.border, width: 1.15),
        ),
        child: Center(
          child: Icon(Icons.visibility_off_outlined, size: 16, color: accent),
        ),
      ),
    );
  }
}

class _UnavailableCarContextFooter extends StatelessWidget {
  const _UnavailableCarContextFooter({
    required this.car,
    required this.useTabularColumns,
  });

  final UnavailableOperationalCar car;
  final bool useTabularColumns;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    final double textScale = MediaQuery.textScalerOf(context).scale(1.0);
    final bool showTabular = useTabularColumns && textScale <= 1.35;

    final String sessionLabel = switch (car.session) {
      InactiveSourceSession() => 'inactive',
      ActiveSourceSession(:final session) => session,
    };

    final Color availabilityColor =
        car.availability == ObservationAvailability.awaiting
        ? palette.subtleText
        : palette.caution;

    final Color payloadColor = car.payload == PayloadActivity.active
        ? palette.subtleText
        : palette.caution;

    final String sourceTooltip =
        'Configured Operational source: ${car.source} (session: $sessionLabel)';
    final String availabilityTooltip =
        'Per-car observation availability (${car.availability.wireValue}) reported by ${car.source}.';
    final String payloadTooltip =
        'Source-level payload activity for ${car.source}; a source feed can stay active for sibling cars while this car has no usable observation.';

    return DecoratedBox(
      decoration: BoxDecoration(
        color: palette.elevatedSurface,
        border: Border(top: BorderSide(color: palette.border)),
      ),
      child: showTabular
          ? Padding(
              padding: const EdgeInsets.symmetric(
                horizontal: 12,
                vertical: 6.5,
              ),
              child: Row(
                children: <Widget>[
                  Expanded(
                    flex: 10,
                    child: Tooltip(
                      message: sourceTooltip,
                      child: _TabularField(
                        label: 'Source',
                        value: car.source,
                        valueColor: palette.subtleText,
                        emphasize: false,
                        monospaceValue: true,
                      ),
                    ),
                  ),
                  Container(
                    width: 1,
                    height: 22,
                    margin: const EdgeInsets.symmetric(horizontal: 4.5),
                    color: palette.border,
                  ),
                  Expanded(
                    flex: 8,
                    child: Tooltip(
                      message: availabilityTooltip,
                      child: _TabularField(
                        label: 'Availability',
                        value: car.availability.wireValue,
                        valueColor: availabilityColor,
                        emphasize: true,
                        monospaceValue: true,
                      ),
                    ),
                  ),
                  Container(
                    width: 1,
                    height: 22,
                    margin: const EdgeInsets.symmetric(horizontal: 4.5),
                    color: palette.border,
                  ),
                  Expanded(
                    flex: 7,
                    child: Tooltip(
                      message: payloadTooltip,
                      child: _TabularField(
                        label: 'Source payload',
                        value: car.payload.wireValue,
                        valueColor: payloadColor,
                        emphasize: car.payload != PayloadActivity.active,
                        monospaceValue: true,
                      ),
                    ),
                  ),
                ],
              ),
            )
          : Padding(
              padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 7),
              child: Wrap(
                spacing: 12,
                runSpacing: 5,
                crossAxisAlignment: WrapCrossAlignment.center,
                children: <Widget>[
                  Tooltip(
                    message: sourceTooltip,
                    child: _SafeInlineKv(
                      label: 'Source',
                      value: car.source,
                      valueColor: palette.subtleText,
                      emphasize: false,
                      monospaceValue: true,
                    ),
                  ),
                  Tooltip(
                    message: availabilityTooltip,
                    child: _SafeInlineKv(
                      label: 'Availability',
                      value: car.availability.wireValue,
                      valueColor: availabilityColor,
                      emphasize: true,
                      monospaceValue: true,
                    ),
                  ),
                  Tooltip(
                    message: payloadTooltip,
                    child: _SafeInlineKv(
                      label: 'Source payload',
                      value: car.payload.wireValue,
                      valueColor: payloadColor,
                      emphasize: car.payload != PayloadActivity.active,
                      monospaceValue: true,
                    ),
                  ),
                ],
              ),
            ),
    );
  }
}
