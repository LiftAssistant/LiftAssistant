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
      padding: const EdgeInsets.symmetric(horizontal: 6.5, vertical: 2.5),
      decoration: BoxDecoration(
        color: outlined ? Colors.transparent : background,
        borderRadius: BorderRadius.circular(4),
        border: Border.all(
          color: outlined ? palette.border : foreground.withValues(alpha: 0.24),
        ),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: <Widget>[
          if (dotColor != null) ...<Widget>[
            StatusDot(color: dotColor!, hollow: hollowDot, size: 6.5),
            const SizedBox(width: 5),
          ] else if (icon != null) ...<Widget>[
            Icon(icon, size: 12, color: foreground),
            const SizedBox(width: 4),
          ],
          Flexible(
            child: Text(
              label,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: monospace
                  ? instrumentMonoStyle(
                      context,
                      fontSize: 11,
                      fontWeight: FontWeight.w600,
                      color: foreground,
                    )
                  : TextStyle(
                      fontSize: 11,
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
    this.trailingNote,
    this.trailing,
  });

  final String title;
  final int? count;
  final String? trailingNote;
  final Widget? trailing;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    final Widget titleCluster = Row(
      mainAxisSize: MainAxisSize.min,
      children: <Widget>[
        Flexible(
          child: Text(
            title,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: const TextStyle(
              fontSize: 13,
              fontWeight: FontWeight.w600,
              letterSpacing: -0.1,
            ),
          ),
        ),
        if (count != null) ...<Widget>[
          const SizedBox(width: 6),
          Container(
            padding: const EdgeInsets.symmetric(horizontal: 5.5, vertical: 1),
            decoration: BoxDecoration(
              color: palette.elevatedSurface,
              borderRadius: BorderRadius.circular(4),
              border: Border.all(color: palette.border),
            ),
            child: Text(
              '$count',
              style: instrumentMonoStyle(
                context,
                fontSize: 11,
                fontWeight: FontWeight.w600,
                color: palette.subtleText,
              ),
            ),
          ),
        ],
      ],
    );

    if (trailingNote == null && trailing == null) {
      return Align(alignment: Alignment.centerLeft, child: titleCluster);
    }

    return Row(
      mainAxisAlignment: MainAxisAlignment.spaceBetween,
      children: <Widget>[
        Flexible(flex: 5, child: titleCluster),
        Flexible(
          flex: 6,
          child: Padding(
            padding: const EdgeInsets.only(left: 10),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              mainAxisAlignment: MainAxisAlignment.end,
              children: <Widget>[
                if (trailingNote != null)
                  Flexible(
                    child: Text(
                      trailingNote!,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      textAlign: TextAlign.right,
                      style: TextStyle(fontSize: 11, color: palette.mutedText),
                    ),
                  ),
                if (trailing != null) ...<Widget>[
                  if (trailingNote != null) const SizedBox(width: 8),
                  trailing!,
                ],
              ],
            ),
          ),
        ),
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
            border: Border(left: BorderSide(color: accent, width: 3)),
          ),
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 9),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              mainAxisSize: MainAxisSize.min,
              children: <Widget>[
                Row(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: <Widget>[
                    Padding(
                      padding: const EdgeInsets.only(top: 1.5),
                      child: Icon(icon, size: 15, color: accent),
                    ),
                    const SizedBox(width: 8),
                    Expanded(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: <Widget>[
                          Text(
                            title,
                            style: TextStyle(
                              fontSize: 12,
                              fontWeight: FontWeight.w700,
                              color: accent,
                            ),
                          ),
                          const SizedBox(height: 2),
                          Text(
                            message,
                            style: TextStyle(
                              fontSize: 12,
                              height: 1.32,
                              color: palette.subtleText,
                            ),
                          ),
                        ],
                      ),
                    ),
                  ],
                ),
                if (actions.isNotEmpty) ...<Widget>[
                  const SizedBox(height: 8),
                  Wrap(spacing: 6, runSpacing: 6, children: actions),
                ],
              ],
            ),
          ),
        ),
      ),
    );
  }
}
