import 'package:flutter/material.dart';

import '../theme/instrument_theme.dart';

class StatusDot extends StatelessWidget {
  const StatusDot({
    super.key,
    required this.color,
    this.hollow = false,
    this.size = 7,
  });

  final Color color;
  final bool hollow;
  final double size;

  @override
  Widget build(BuildContext context) {
    return Container(
      width: size,
      height: size,
      decoration: BoxDecoration(
        shape: BoxShape.circle,
        color: hollow ? Colors.transparent : color,
        border: hollow ? Border.all(color: color, width: 1.5) : null,
      ),
    );
  }
}

class SemanticBadge extends StatelessWidget {
  const SemanticBadge({
    super.key,
    required this.label,
    required this.foreground,
    required this.background,
    this.icon,
    this.dotColor,
    this.hollowDot = false,
    this.monospace = false,
    this.outlined = false,
    this.tooltip,
  });

  final String label;
  final Color foreground;
  final Color background;
  final IconData? icon;
  final Color? dotColor;
  final bool hollowDot;
  final bool monospace;
  final bool outlined;
  final String? tooltip;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    final Widget tag = Container(
      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 4),
      decoration: BoxDecoration(
        color: outlined ? Colors.transparent : background,
        borderRadius: BorderRadius.circular(8),
        border: Border.all(
          color: outlined ? palette.border : foreground.withValues(alpha: 0.28),
        ),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: <Widget>[
          if (dotColor != null) ...<Widget>[
            StatusDot(color: dotColor!, hollow: hollowDot, size: 7),
            const SizedBox(width: 6),
          ] else if (icon != null) ...<Widget>[
            Icon(icon, size: 13.5, color: foreground),
            const SizedBox(width: 5),
          ],
          Flexible(
            child: Text(
              label,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: instrumentTabularStyle(
                context,
                fontSize: 11.5,
                fontWeight: FontWeight.w600,
                color: foreground,
              ),
            ),
          ),
        ],
      ),
    );

    if (tooltip != null && tooltip!.isNotEmpty) {
      return Tooltip(message: tooltip!, child: tag);
    }
    return tag;
  }
}

class SectionHeader extends StatelessWidget {
  const SectionHeader({
    super.key,
    required this.title,
    this.count,
    this.icon,
    this.trailing,
  });

  final String title;
  final int? count;
  final IconData? icon;
  final Widget? trailing;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    final Widget titleCluster = Row(
      mainAxisSize: MainAxisSize.min,
      children: <Widget>[
        if (icon != null) ...<Widget>[
          Icon(icon, size: 17, color: palette.info),
          const SizedBox(width: 8),
        ],
        Flexible(
          child: Text(
            title,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: const TextStyle(
              fontSize: 15,
              fontWeight: FontWeight.w700,
              letterSpacing: -0.2,
            ),
          ),
        ),
        if (count != null) ...<Widget>[
          const SizedBox(width: 8),
          Badge.count(
            count: count!,
            backgroundColor: palette.elevatedSurface,
            textColor: palette.subtleText,
            textStyle: instrumentTabularStyle(
              context,
              fontSize: 11.5,
              fontWeight: FontWeight.w700,
              color: palette.subtleText,
            ),
          ),
        ],
      ],
    );

    if (trailing == null) {
      return Align(alignment: Alignment.centerLeft, child: titleCluster);
    }

    return Row(
      children: <Widget>[
        Expanded(
          child: Align(alignment: Alignment.centerLeft, child: titleCluster),
        ),
        const SizedBox(width: 10),
        trailing!,
      ],
    );
  }
}

class SemanticCalloutBanner extends StatelessWidget {
  const SemanticCalloutBanner({
    super.key,
    required this.icon,
    required this.title,
    required this.message,
    required this.accent,
    required this.background,
    this.actions = const <Widget>[],
  });

  final IconData icon;
  final String title;
  final String message;
  final Color accent;
  final Color background;
  final List<Widget> actions;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    final double textScale = MediaQuery.textScalerOf(context).scale(1.0);

    final Widget copyRow = Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: <Widget>[
        Container(
          width: 32,
          height: 32,
          decoration: BoxDecoration(
            color: background,
            borderRadius: BorderRadius.circular(8),
          ),
          child: Icon(icon, size: 17, color: accent),
        ),
        const SizedBox(width: 12),
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            mainAxisSize: MainAxisSize.min,
            children: <Widget>[
              Text(
                title,
                style: TextStyle(
                  fontSize: 13,
                  fontWeight: FontWeight.w700,
                  color: accent,
                ),
              ),
              const SizedBox(height: 3),
              Text(
                message,
                style: TextStyle(
                  fontSize: 12.5,
                  height: 1.35,
                  color: palette.subtleText,
                ),
              ),
            ],
          ),
        ),
      ],
    );

    return Card(
      color: palette.surface,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(12),
        side: BorderSide(color: accent.withValues(alpha: 0.45)),
      ),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 12),
        child: LayoutBuilder(
          builder: (BuildContext context, BoxConstraints constraints) {
            final bool inlineActions =
                actions.isNotEmpty &&
                constraints.maxWidth >= 600 &&
                textScale <= 1.3;
            if (inlineActions) {
              return Row(
                crossAxisAlignment: CrossAxisAlignment.center,
                children: <Widget>[
                  Expanded(child: copyRow),
                  const SizedBox(width: 12),
                  ConstrainedBox(
                    constraints: BoxConstraints(
                      maxWidth: constraints.maxWidth * 0.44,
                    ),
                    child: Wrap(
                      spacing: 8,
                      runSpacing: 8,
                      alignment: WrapAlignment.end,
                      children: actions,
                    ),
                  ),
                ],
              );
            }

            return Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              mainAxisSize: MainAxisSize.min,
              children: <Widget>[
                copyRow,
                if (actions.isNotEmpty) ...<Widget>[
                  const SizedBox(height: 10),
                  Wrap(spacing: 8, runSpacing: 8, children: actions),
                ],
              ],
            );
          },
        ),
      ),
    );
  }
}
