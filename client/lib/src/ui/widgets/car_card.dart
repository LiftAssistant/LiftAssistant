import 'package:flutter/material.dart';

import '../../api/read_model.dart';
import '../theme/instrument_theme.dart';
import 'semantic_badge.dart';

class OperationalCarCard extends StatelessWidget {
  const OperationalCarCard({
    super.key,
    required this.car,
    this.stretchToRowHeight = false,
    this.useTabularFooter = true,
    this.compactCard = false,
  });

  final OperationalCar car;
  final bool stretchToRowHeight;
  final bool useTabularFooter;
  final bool compactCard;

  (Color iconFg, Color iconBg) _avatarColors(InstrumentPalette palette) {
    if (car.operatingMode == CarOperatingMode.fireService ||
        car.door == CarDoorState.fault) {
      return (palette.danger, palette.dangerContainer);
    }
    if (car.operatingMode == CarOperatingMode.inspection ||
        car.door == CarDoorState.obstructed) {
      return (palette.caution, palette.cautionContainer);
    }
    if (car.operatingMode == CarOperatingMode.outOfService) {
      return (palette.neutral, palette.neutralContainer);
    }
    return (palette.info, palette.infoContainer);
  }

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    final (Color avatarFg, Color avatarBg) = _avatarColors(palette);

    return Card(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: stretchToRowHeight ? MainAxisSize.max : MainAxisSize.min,
        mainAxisAlignment: stretchToRowHeight
            ? MainAxisAlignment.spaceBetween
            : MainAxisAlignment.start,
        children: <Widget>[
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 14, 16, 14),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              mainAxisSize: MainAxisSize.min,
              children: <Widget>[
                // 1. Header: Tonal CircleAvatar + Logical CarId + OperatingMode M3 Chip
                Row(
                  children: <Widget>[
                    CircleAvatar(
                      radius: 17,
                      backgroundColor: avatarBg,
                      foregroundColor: avatarFg,
                      child: const Icon(Icons.elevator_outlined, size: 19),
                    ),
                    const SizedBox(width: 10),
                    Expanded(
                      child: Tooltip(
                        message: 'Logical CarId: ${car.id}',
                        child: Text(
                          car.id,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: instrumentTabularStyle(
                            context,
                            fontSize: 16,
                            fontWeight: FontWeight.w700,
                            letterSpacing: -0.2,
                          ),
                        ),
                      ),
                    ),
                    const SizedBox(width: 8),
                    ConstrainedBox(
                      constraints: const BoxConstraints(maxWidth: 168),
                      child: _ModeReadout(mode: car.operatingMode),
                    ),
                  ],
                ),
                const SizedBox(height: 16),

                // 2. Primary Visual Body: 76x88 Elevator Elevation Graphic + Location Readout
                Builder(
                  builder: (BuildContext context) {
                    final double textScale = MediaQuery.textScalerOf(context)
                        .scale(1.0);
                    final double screenWidth = MediaQuery.sizeOf(context).width;
                    final bool compactGraphic =
                        compactCard || screenWidth < 380 || textScale > 1.35;

                    return Row(
                      crossAxisAlignment: CrossAxisAlignment.center,
                      children: <Widget>[
                        _ElevatorCarElevationGraphic(
                          door: car.door,
                          motion: car.motion,
                          compact: compactGraphic,
                        ),
                        SizedBox(width: compactGraphic ? 12 : 16),
                        Expanded(
                          child: _LocationReadout(location: car.location),
                        ),
                      ],
                    );
                  },
                ),
              ],
            ),
          ),

          // 3. Footer: Divider + 3-column M3 metric items with leading icons
          Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: <Widget>[
              const Divider(height: 1),
              _CarTelemetryFooter(
                motion: car.motion,
                door: car.door,
                reportedDestination: car.reportedDestination,
                useTabularColumns: useTabularFooter,
                compactCard: compactCard,
              ),
            ],
          ),
        ],
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
        return SemanticBadge(
          label: 'Normal',
          foreground: palette.ok,
          background: palette.okContainer,
          icon: Icons.check_circle_outline_rounded,
          tooltip: 'Canonical operating mode: Normal',
        );
      case CarOperatingMode.independent:
        return SemanticBadge(
          label: 'Independent',
          foreground: palette.info,
          background: palette.infoContainer,
          icon: Icons.person_outline_rounded,
          tooltip: 'Canonical operating mode: Independent',
        );
      case CarOperatingMode.inspection:
        return SemanticBadge(
          label: 'Inspection',
          foreground: palette.caution,
          background: palette.cautionContainer,
          icon: Icons.engineering_outlined,
          tooltip: 'Canonical operating mode: Inspection',
        );
      case CarOperatingMode.fireService:
        return SemanticBadge(
          label: 'FireService',
          foreground: palette.danger,
          background: palette.dangerContainer,
          icon: Icons.local_fire_department_outlined,
          tooltip: 'Canonical operating mode: FireService',
        );
      case CarOperatingMode.outOfService:
        return SemanticBadge(
          label: 'OutOfService',
          foreground: palette.neutral,
          background: palette.neutralContainer,
          icon: Icons.do_not_disturb_on_outlined,
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
    final double textScale = MediaQuery.textScalerOf(context).scale(1.0);

    switch (location) {
      case FloorLocation(:final floor):
        return Wrap(
          spacing: 10,
          runSpacing: 6,
          crossAxisAlignment: WrapCrossAlignment.center,
          children: <Widget>[
            Text(
              '${floor}F',
              style: instrumentTabularStyle(
                context,
                fontSize: 28,
                fontWeight: FontWeight.w800,
                letterSpacing: -0.5,
                height: 1.05,
              ),
            ),
            SemanticBadge(
              label: 'Floor Reference',
              foreground: palette.subtleText,
              background: palette.elevatedSurface,
              icon: Icons.layers_outlined,
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
              spacing: 10,
              runSpacing: 6,
              crossAxisAlignment: WrapCrossAlignment.center,
              children: <Widget>[
                Text(
                  '${normalizedPos.toStringAsFixed(2)} F',
                  style: instrumentTabularStyle(
                    context,
                    fontSize: 26,
                    fontWeight: FontWeight.w800,
                    letterSpacing: -0.4,
                    height: 1.05,
                  ),
                ),
                SemanticBadge(
                  label: 'Continuous',
                  foreground: palette.info,
                  background: palette.infoContainer,
                  icon: Icons.straighten_rounded,
                  tooltip: 'Genuine continuous position evidence held by Host (kind: continuous).',
                ),
              ],
            ),
            const SizedBox(height: 10),
            _ContinuousOffsetGauge(
              fraction: clampedFraction,
              clamped: isClamped,
              signedDelta: signedDelta,
              compact: textScale > 1.35,
            ),
            const SizedBox(height: 6),
            Wrap(
              spacing: 6,
              runSpacing: 4,
              crossAxisAlignment: WrapCrossAlignment.center,
              children: <Widget>[
                Text(
                  'Nearest Floor $nearestFloor',
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: instrumentTabularStyle(
                    context,
                    fontSize: 12,
                    fontWeight: FontWeight.w500,
                    color: palette.subtleText,
                  ),
                ),
                SemanticBadge(
                  label: signedDelta,
                  foreground: isClamped ? palette.caution : palette.info,
                  background: isClamped
                      ? palette.cautionContainer
                      : palette.infoContainer,
                  compact: true,
                ),
              ],
            ),
          ],
        );
    }
  }
}

/// Material 3 landing-offset scale (`[-0.5, +0.5]`) rendered when [ContinuousLocation] is present.
class _ContinuousOffsetGauge extends StatelessWidget {
  const _ContinuousOffsetGauge({
    required this.fraction,
    required this.clamped,
    required this.signedDelta,
    this.compact = false,
  });

  final double fraction;
  final bool clamped;
  final String signedDelta;
  final bool compact;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    final Color markerColor = clamped ? palette.caution : palette.info;
    final Color trackColor = clamped
        ? palette.cautionContainer
        : palette.infoContainer;
    final double trackWidth = compact ? 88.0 : 136.0;
    final double markerX = ((trackWidth - 12) * fraction).clamp(
      0.0,
      trackWidth - 12,
    );

    return Tooltip(
      message: clamped
          ? 'Offset $signedDelta exceeds ±0.50 landing window (gauge pinned at boundary).'
          : 'Landing offset $signedDelta within [-0.50, +0.50] window.',
      child: SizedBox(
        width: trackWidth,
        height: 12,
        child: Stack(
          alignment: Alignment.centerLeft,
          children: <Widget>[
            Container(
              height: 6,
              width: trackWidth,
              decoration: BoxDecoration(
                color: trackColor,
                borderRadius: BorderRadius.circular(3),
              ),
            ),
            Positioned(
              left: (trackWidth / 2) - 1,
              top: 2,
              bottom: 2,
              child: Container(
                width: 2,
                decoration: BoxDecoration(
                  color: markerColor.withValues(alpha: 0.45),
                  borderRadius: BorderRadius.circular(1),
                ),
              ),
            ),
            AnimatedPositioned(
              duration: const Duration(milliseconds: 160),
              curve: Curves.easeOutCubic,
              left: markerX,
              child: Container(
                width: 12,
                height: 12,
                decoration: BoxDecoration(
                  color: markerColor,
                  shape: BoxShape.circle,
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// Crisp 76×88px elevator car & split-door elevation micro-graphic.
/// Depicts header transom with motion indicator, illuminated cabin backdrop,
/// left/right sliding door panels with dynamic opening gap, obstruction/fault cue, and landing sill.
class _ElevatorCarElevationGraphic extends StatelessWidget {
  const _ElevatorCarElevationGraphic({
    required this.door,
    required this.motion,
    this.compact = false,
  });

  final CarDoorState door;
  final CarMotion motion;
  final bool compact;

  double _doorGapFraction() {
    switch (door) {
      case CarDoorState.closed:
        return 0.0;
      case CarDoorState.opening:
      case CarDoorState.closing:
        return 0.44;
      case CarDoorState.open:
        return 0.78;
      case CarDoorState.obstructed:
        return 0.54;
      case CarDoorState.fault:
        return 0.34;
    }
  }

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    final bool isDark = Theme.of(context).brightness == Brightness.dark;

    final double outerWidth = compact ? 56.0 : 76.0;
    final double outerHeight = compact ? 66.0 : 88.0;
    const double borderWidth = 1.5;
    const double horizontalPad = 5.0;
    final double innerWidth =
        outerWidth - (borderWidth * 2) - (horizontalPad * 2);

    final Color frameColor = switch (door) {
      CarDoorState.fault => palette.danger,
      CarDoorState.obstructed => palette.caution,
      CarDoorState.open => palette.ok,
      CarDoorState.opening || CarDoorState.closing => palette.info,
      CarDoorState.closed =>
        motion is MovingMotion ? palette.info : palette.border,
    };

    final Color doorPanelColor = switch (door) {
      CarDoorState.fault => palette.dangerContainer.withValues(
        alpha: isDark ? 0.85 : 0.9,
      ),
      CarDoorState.obstructed => palette.cautionContainer.withValues(
        alpha: isDark ? 0.85 : 0.9,
      ),
      _ => isDark ? const Color(0xFF282D3A) : const Color(0xFFDCE2EC),
    };

    final Color doorEdgeColor = switch (door) {
      CarDoorState.fault => palette.danger.withValues(alpha: 0.7),
      CarDoorState.obstructed => palette.caution.withValues(alpha: 0.7),
      CarDoorState.open => palette.ok.withValues(alpha: 0.65),
      CarDoorState.opening ||
      CarDoorState.closing => palette.info.withValues(alpha: 0.65),
      CarDoorState.closed => palette.subtleText.withValues(alpha: 0.45),
    };

    final Color cabinBackdropColor = isDark
        ? const Color(0xFF141821)
        : const Color(0xFFF8FAFC);

    final IconData? motionArrow = switch (motion) {
      StoppedMotion() => null,
      MovingMotion(:final direction) =>
        direction == MotionDirection.up
            ? Icons.north_rounded
            : Icons.south_rounded,
    };

    final double gap = innerWidth * _doorGapFraction();
    final double leafW = ((innerWidth - gap) / 2).clamp(3.0, innerWidth / 2);

    return SizedBox(
      width: outerWidth,
      height: outerHeight,
      child: DecoratedBox(
        decoration: BoxDecoration(
          color: palette.elevatedSurface,
          borderRadius: BorderRadius.circular(10),
          border: Border.all(color: frameColor, width: borderWidth),
        ),
        child: Padding(
          padding: const EdgeInsets.fromLTRB(
            horizontalPad,
            5,
            horizontalPad,
            4,
          ),
          child: Column(
            children: <Widget>[
              // 1. Car header transom bar + directional indicator
              Container(
                height: compact ? 8 : 10,
                width: double.infinity,
                decoration: BoxDecoration(
                  color: palette.surface,
                  borderRadius: BorderRadius.circular(3),
                  border: Border.all(
                    color: motionArrow != null
                        ? palette.info.withValues(alpha: 0.6)
                        : palette.border,
                    width: 0.8,
                  ),
                ),
                child: Center(
                  child: Row(
                    mainAxisSize: MainAxisSize.min,
                    children: <Widget>[
                      Container(
                        width: 4,
                        height: 4,
                        decoration: BoxDecoration(
                          shape: BoxShape.circle,
                          color: motionArrow != null
                              ? palette.info
                              : palette.ok,
                        ),
                      ),
                      const SizedBox(width: 4),
                      Container(
                        width: compact ? 14 : 20,
                        height: 2.5,
                        decoration: BoxDecoration(
                          color: motionArrow != null
                              ? palette.info
                              : palette.mutedText.withValues(alpha: 0.55),
                          borderRadius: BorderRadius.circular(1.5),
                        ),
                      ),
                    ],
                  ),
                ),
              ),
              const SizedBox(height: 4),

              // 2. Elevator doorway + cabin interior + split sliding door panels
              Expanded(
                child: ClipRRect(
                  borderRadius: BorderRadius.circular(4),
                  child: DecoratedBox(
                    decoration: BoxDecoration(
                      color: cabinBackdropColor,
                      border: Border.all(
                        color: palette.border.withValues(alpha: 0.7),
                        width: 0.8,
                      ),
                    ),
                    child: Stack(
                      alignment: Alignment.center,
                      children: <Widget>[
                        // Cabin overhead ceiling glow when doors are open/parted
                        if (gap > 2)
                          Positioned(
                            top: 0,
                            left: 6,
                            right: 6,
                            child: Container(
                              height: 3,
                              decoration: BoxDecoration(
                                color: palette.info.withValues(alpha: 0.35),
                                borderRadius: const BorderRadius.vertical(
                                  bottom: Radius.circular(2),
                                ),
                              ),
                            ),
                          ),
                        // Cabin rear handrail bar
                        if (gap > 6)
                          Positioned(
                            left: 8,
                            right: 8,
                            child: Container(
                              height: 1.5,
                              color: palette.border.withValues(alpha: 0.65),
                            ),
                          ),
                        // Left sliding door leaf
                        Align(
                          alignment: Alignment.centerLeft,
                          child: AnimatedContainer(
                            duration: const Duration(milliseconds: 180),
                            curve: Curves.easeOutCubic,
                            width: leafW,
                            height: double.infinity,
                            decoration: BoxDecoration(
                              color: doorPanelColor,
                              border: Border(
                                right: BorderSide(
                                  color: doorEdgeColor,
                                  width: 1.3,
                                ),
                              ),
                            ),
                            child: leafW >= 10
                                ? Align(
                                    alignment: Alignment.centerRight,
                                    child: Container(
                                      width: 1.5,
                                      height: compact ? 14 : 20,
                                      margin: const EdgeInsets.only(right: 3),
                                      decoration: BoxDecoration(
                                        color: doorEdgeColor.withValues(
                                          alpha: 0.55,
                                        ),
                                        borderRadius: BorderRadius.circular(1),
                                      ),
                                    ),
                                  )
                                : null,
                          ),
                        ),
                        // Right sliding door leaf
                        Align(
                          alignment: Alignment.centerRight,
                          child: AnimatedContainer(
                            duration: const Duration(milliseconds: 180),
                            curve: Curves.easeOutCubic,
                            width: leafW,
                            height: double.infinity,
                            decoration: BoxDecoration(
                              color: doorPanelColor,
                              border: Border(
                                left: BorderSide(
                                  color: doorEdgeColor,
                                  width: 1.3,
                                ),
                              ),
                            ),
                            child: leafW >= 10
                                ? Align(
                                    alignment: Alignment.centerLeft,
                                    child: Container(
                                      width: 1.5,
                                      height: compact ? 14 : 20,
                                      margin: const EdgeInsets.only(left: 3),
                                      decoration: BoxDecoration(
                                        color: doorEdgeColor.withValues(
                                          alpha: 0.55,
                                        ),
                                        borderRadius: BorderRadius.circular(1),
                                      ),
                                    ),
                                  )
                                : null,
                          ),
                        ),
                        // Center status overlay (Fault / Obstructed / Motion Arrow)
                        if (door == CarDoorState.fault)
                          Container(
                            padding: const EdgeInsets.all(3),
                            decoration: BoxDecoration(
                              color: palette.dangerContainer,
                              shape: BoxShape.circle,
                              border: Border.all(color: palette.danger),
                            ),
                            child: Icon(
                              Icons.error_outline_rounded,
                              size: compact ? 12 : 15,
                              color: palette.danger,
                            ),
                          )
                        else if (door == CarDoorState.obstructed)
                          Container(
                            padding: const EdgeInsets.all(3),
                            decoration: BoxDecoration(
                              color: palette.cautionContainer,
                              shape: BoxShape.circle,
                              border: Border.all(color: palette.caution),
                            ),
                            child: Icon(
                              Icons.warning_amber_rounded,
                              size: compact ? 12 : 15,
                              color: palette.caution,
                            ),
                          )
                        else if (motionArrow != null)
                          Container(
                            padding: const EdgeInsets.all(3),
                            decoration: BoxDecoration(
                              color: palette.infoContainer.withValues(
                                alpha: 0.92,
                              ),
                              shape: BoxShape.circle,
                              border: Border.all(
                                color: palette.info.withValues(alpha: 0.7),
                              ),
                            ),
                            child: Icon(
                              motionArrow,
                              size: compact ? 12 : 15,
                              color: palette.info,
                            ),
                          ),
                      ],
                    ),
                  ),
                ),
              ),
              const SizedBox(height: 3.5),

              // 3. Bottom landing sill plate
              Container(
                height: 3,
                width: double.infinity,
                decoration: BoxDecoration(
                  color: frameColor,
                  borderRadius: BorderRadius.circular(1.5),
                ),
              ),
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
    this.compactCard = false,
  });

  final CarMotion motion;
  final CarDoorState door;
  final ReportedDestination reportedDestination;
  final bool useTabularColumns;
  final bool compactCard;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    final double textScale = MediaQuery.textScalerOf(context).scale(1.0);

    final (
      String motionText,
      Color motionColor,
      bool motionEmph,
    ) = switch (motion) {
      StoppedMotion() => ('Stopped', palette.subtleText, false),
      MovingMotion(:final direction) =>
        direction == MotionDirection.up
            ? ('Moving Up', palette.info, true)
            : ('Moving Down', palette.info, true),
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

    final bool showThreeColumns = useTabularColumns && textScale <= 1.35;

    if (showThreeColumns) {
      final double screenWidth = MediaQuery.sizeOf(context).width;
      final bool compactCols = compactCard || screenWidth < 1100;
      final double divMargin = compactCols ? 6.0 : 8.0;
      return Padding(
        padding: EdgeInsets.symmetric(
          horizontal: compactCols ? 10 : 12,
          vertical: 11,
        ),
        child: Row(
          children: <Widget>[
            Expanded(
              flex: compactCols ? 7 : 9,
              child: _FooterMetricTile(
                icon: Icons.swap_vert_rounded,
                label: 'Motion',
                value: motionText,
                valueColor: motionColor,
                emphasize: motionEmph,
                compact: compactCols,
              ),
            ),
            Container(
              width: 1,
              height: 26,
              margin: EdgeInsets.symmetric(horizontal: divMargin),
              color: palette.border,
            ),
            Expanded(
              flex: compactCols ? 6 : 8,
              child: _FooterMetricTile(
                icon: Icons.sensor_door_outlined,
                label: 'Door',
                value: doorText,
                valueColor: doorColor,
                emphasize: doorEmph,
                compact: compactCols,
              ),
            ),
            Container(
              width: 1,
              height: 26,
              margin: EdgeInsets.symmetric(horizontal: divMargin),
              color: palette.border,
            ),
            Expanded(
              flex: compactCols ? 12 : 12,
              child: Tooltip(
                message: destTooltip,
                child: _FooterMetricTile(
                  icon: Icons.outbox_outlined,
                  label: 'Reported Destination',
                  value: destText,
                  valueColor: destColor,
                  emphasize: destEmph,
                  compact: compactCols,
                ),
              ),
            ),
          ],
        ),
      );
    }

    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
      child: Wrap(
        spacing: 14,
        runSpacing: 8,
        children: <Widget>[
          _CompactFooterMetric(
            icon: Icons.swap_vert_rounded,
            label: 'Motion',
            value: motionText,
            valueColor: motionColor,
            emphasize: motionEmph,
          ),
          _CompactFooterMetric(
            icon: Icons.sensor_door_outlined,
            label: 'Door',
            value: doorText,
            valueColor: doorColor,
            emphasize: doorEmph,
          ),
          Tooltip(
            message: destTooltip,
            child: _CompactFooterMetric(
              icon: Icons.outbox_outlined,
              label: 'Reported Destination',
              value: destText,
              valueColor: destColor,
              emphasize: destEmph,
            ),
          ),
        ],
      ),
    );
  }
}

class _FooterMetricTile extends StatelessWidget {
  const _FooterMetricTile({
    required this.icon,
    required this.label,
    required this.value,
    required this.valueColor,
    required this.emphasize,
    this.compact = false,
  });

  final IconData icon;
  final String label;
  final String value;
  final Color valueColor;
  final bool emphasize;
  final bool compact;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    if (compact) {
      return Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisSize: MainAxisSize.min,
        children: <Widget>[
          Row(
            children: <Widget>[
              Icon(
                icon,
                size: 12.5,
                color: emphasize ? valueColor : palette.mutedText,
              ),
              const SizedBox(width: 4),
              Expanded(
                child: Text(
                  label,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                    fontSize: 10.5,
                    fontWeight: FontWeight.w500,
                    color: palette.mutedText,
                    height: 1.1,
                  ),
                ),
              ),
            ],
          ),
          const SizedBox(height: 2.5),
          Text(
            value,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: instrumentTabularStyle(
              context,
              fontSize: 12.5,
              fontWeight: emphasize ? FontWeight.w700 : FontWeight.w600,
              color: valueColor,
            ),
          ),
        ],
      );
    }

    return Row(
      children: <Widget>[
        Container(
          width: 26,
          height: 26,
          decoration: BoxDecoration(
            color: emphasize
                ? valueColor.withValues(alpha: 0.14)
                : palette.elevatedSurface,
            borderRadius: BorderRadius.circular(7),
          ),
          child: Icon(
            icon,
            size: 15,
            color: emphasize ? valueColor : palette.mutedText,
          ),
        ),
        const SizedBox(width: 7),
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            mainAxisSize: MainAxisSize.min,
            children: <Widget>[
              Text(
                label,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(
                  fontSize: 10.5,
                  fontWeight: FontWeight.w500,
                  color: palette.mutedText,
                  height: 1.1,
                ),
              ),
              const SizedBox(height: 2),
              Text(
                value,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: instrumentTabularStyle(
                  context,
                  fontSize: 12.5,
                  fontWeight: emphasize ? FontWeight.w700 : FontWeight.w600,
                  color: valueColor,
                ),
              ),
            ],
          ),
        ),
      ],
    );
  }
}

class _CompactFooterMetric extends StatelessWidget {
  const _CompactFooterMetric({
    required this.icon,
    required this.label,
    required this.value,
    required this.valueColor,
    required this.emphasize,
  });

  final IconData icon;
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
        Icon(icon, size: 15, color: emphasize ? valueColor : palette.mutedText),
        const SizedBox(width: 5),
        Flexible(
          child: Text(
            '$label: $value',
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: instrumentTabularStyle(
              context,
              fontSize: 12,
              fontWeight: emphasize ? FontWeight.w700 : FontWeight.w500,
              color: valueColor,
            ),
          ),
        ),
      ],
    );
  }
}
