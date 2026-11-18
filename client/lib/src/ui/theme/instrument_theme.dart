import 'package:flutter/material.dart';

@immutable
class InstrumentPalette extends ThemeExtension<InstrumentPalette> {
  const InstrumentPalette({
    required this.canvas,
    required this.surface,
    required this.elevatedSurface,
    required this.border,
    required this.subtleText,
    required this.mutedText,
    required this.ok,
    required this.okContainer,
    required this.caution,
    required this.cautionContainer,
    required this.danger,
    required this.dangerContainer,
    required this.info,
    required this.infoContainer,
    required this.operationalRole,
    required this.operationalRoleContainer,
    required this.auxiliaryRole,
    required this.auxiliaryRoleContainer,
    required this.neutral,
    required this.neutralContainer,
  });

  final Color canvas;
  final Color surface;
  final Color elevatedSurface;
  final Color border;
  final Color subtleText;
  final Color mutedText;

  final Color ok;
  final Color okContainer;
  final Color caution;
  final Color cautionContainer;
  final Color danger;
  final Color dangerContainer;
  final Color info;
  final Color infoContainer;
  final Color operationalRole;
  final Color operationalRoleContainer;
  final Color auxiliaryRole;
  final Color auxiliaryRoleContainer;
  final Color neutral;
  final Color neutralContainer;

  /// Material 3 dark slate console palette.
  static const InstrumentPalette dark = InstrumentPalette(
    canvas: Color(0xFF0F1115),
    surface: Color(0xFF171A21),
    elevatedSurface: Color(0xFF20242E),
    border: Color(0xFF2C313D),
    subtleText: Color(0xFFA0A8B8),
    mutedText: Color(0xFF6E7787),
    ok: Color(0xFF34D399),
    okContainer: Color(0xFF112E25),
    caution: Color(0xFFFBBF24),
    cautionContainer: Color(0xFF312510),
    danger: Color(0xFFF87171),
    dangerContainer: Color(0xFF331619),
    info: Color(0xFF38BDF8),
    infoContainer: Color(0xFF102A3A),
    operationalRole: Color(0xFF38BDF8),
    operationalRoleContainer: Color(0xFF102A3A),
    auxiliaryRole: Color(0xFFA0A8B8),
    auxiliaryRoleContainer: Color(0xFF20242E),
    neutral: Color(0xFF8B93A4),
    neutralContainer: Color(0xFF20242E),
  );

  /// Material 3 daylight architectural surface palette.
  static const InstrumentPalette light = InstrumentPalette(
    canvas: Color(0xFFF4F6F9),
    surface: Color(0xFFFFFFFF),
    elevatedSurface: Color(0xFFEEF1F6),
    border: Color(0xFFDCE1E9),
    subtleText: Color(0xFF4B5563),
    mutedText: Color(0xFF6B7280),
    ok: Color(0xFF059669),
    okContainer: Color(0xFFD1FAE5),
    caution: Color(0xFFD97706),
    cautionContainer: Color(0xFFFEF3C7),
    danger: Color(0xFFDC2626),
    dangerContainer: Color(0xFFFEE2E2),
    info: Color(0xFF0284C7),
    infoContainer: Color(0xFFE0F2FE),
    operationalRole: Color(0xFF0284C7),
    operationalRoleContainer: Color(0xFFE0F2FE),
    auxiliaryRole: Color(0xFF4B5563),
    auxiliaryRoleContainer: Color(0xFFEEF1F6),
    neutral: Color(0xFF5E6675),
    neutralContainer: Color(0xFFEEF1F6),
  );

  @override
  InstrumentPalette copyWith({
    Color? canvas,
    Color? surface,
    Color? elevatedSurface,
    Color? border,
    Color? subtleText,
    Color? mutedText,
    Color? ok,
    Color? okContainer,
    Color? caution,
    Color? cautionContainer,
    Color? danger,
    Color? dangerContainer,
    Color? info,
    Color? infoContainer,
    Color? operationalRole,
    Color? operationalRoleContainer,
    Color? auxiliaryRole,
    Color? auxiliaryRoleContainer,
    Color? neutral,
    Color? neutralContainer,
  }) {
    return InstrumentPalette(
      canvas: canvas ?? this.canvas,
      surface: surface ?? this.surface,
      elevatedSurface: elevatedSurface ?? this.elevatedSurface,
      border: border ?? this.border,
      subtleText: subtleText ?? this.subtleText,
      mutedText: mutedText ?? this.mutedText,
      ok: ok ?? this.ok,
      okContainer: okContainer ?? this.okContainer,
      caution: caution ?? this.caution,
      cautionContainer: cautionContainer ?? this.cautionContainer,
      danger: danger ?? this.danger,
      dangerContainer: dangerContainer ?? this.dangerContainer,
      info: info ?? this.info,
      infoContainer: infoContainer ?? this.infoContainer,
      operationalRole: operationalRole ?? this.operationalRole,
      operationalRoleContainer:
          operationalRoleContainer ?? this.operationalRoleContainer,
      auxiliaryRole: auxiliaryRole ?? this.auxiliaryRole,
      auxiliaryRoleContainer:
          auxiliaryRoleContainer ?? this.auxiliaryRoleContainer,
      neutral: neutral ?? this.neutral,
      neutralContainer: neutralContainer ?? this.neutralContainer,
    );
  }

  @override
  InstrumentPalette lerp(ThemeExtension<InstrumentPalette>? other, double t) {
    if (other is! InstrumentPalette) {
      return this;
    }
    return InstrumentPalette(
      canvas: Color.lerp(canvas, other.canvas, t)!,
      surface: Color.lerp(surface, other.surface, t)!,
      elevatedSurface: Color.lerp(elevatedSurface, other.elevatedSurface, t)!,
      border: Color.lerp(border, other.border, t)!,
      subtleText: Color.lerp(subtleText, other.subtleText, t)!,
      mutedText: Color.lerp(mutedText, other.mutedText, t)!,
      ok: Color.lerp(ok, other.ok, t)!,
      okContainer: Color.lerp(okContainer, other.okContainer, t)!,
      caution: Color.lerp(caution, other.caution, t)!,
      cautionContainer: Color.lerp(
        cautionContainer,
        other.cautionContainer,
        t,
      )!,
      danger: Color.lerp(danger, other.danger, t)!,
      dangerContainer: Color.lerp(dangerContainer, other.dangerContainer, t)!,
      info: Color.lerp(info, other.info, t)!,
      infoContainer: Color.lerp(infoContainer, other.infoContainer, t)!,
      operationalRole: Color.lerp(operationalRole, other.operationalRole, t)!,
      operationalRoleContainer: Color.lerp(
        operationalRoleContainer,
        other.operationalRoleContainer,
        t,
      )!,
      auxiliaryRole: Color.lerp(auxiliaryRole, other.auxiliaryRole, t)!,
      auxiliaryRoleContainer: Color.lerp(
        auxiliaryRoleContainer,
        other.auxiliaryRoleContainer,
        t,
      )!,
      neutral: Color.lerp(neutral, other.neutral, t)!,
      neutralContainer: Color.lerp(
        neutralContainer,
        other.neutralContainer,
        t,
      )!,
    );
  }
}

extension InstrumentThemeContext on BuildContext {
  InstrumentPalette get palette =>
      Theme.of(this).extension<InstrumentPalette>() ?? InstrumentPalette.dark;
}

/// Clean sans-serif typography with tabular figures for numbers, IDs, and badges.
TextStyle instrumentTabularStyle(
  BuildContext context, {
  double fontSize = 12.5,
  FontWeight fontWeight = FontWeight.w600,
  Color? color,
  double? height,
  double? letterSpacing,
}) {
  return TextStyle(
    fontSize: fontSize,
    fontWeight: fontWeight,
    height: height,
    letterSpacing: letterSpacing,
    color: color ?? Theme.of(context).colorScheme.onSurface,
    fontFeatures: const <FontFeature>[FontFeature.tabularFigures()],
  );
}

TextStyle instrumentMonoStyle(
  BuildContext context, {
  double fontSize = 12.5,
  FontWeight fontWeight = FontWeight.w600,
  Color? color,
  double? height,
}) {
  return instrumentTabularStyle(
    context,
    fontSize: fontSize,
    fontWeight: fontWeight,
    color: color,
    height: height,
  );
}

ThemeData buildInstrumentTheme(Brightness brightness) {
  final bool isDark = brightness == Brightness.dark;
  final InstrumentPalette palette = isDark
      ? InstrumentPalette.dark
      : InstrumentPalette.light;

  final Color onSurface = isDark
      ? const Color(0xFFF1F5F9)
      : const Color(0xFF0F172A);

  final ColorScheme colorScheme =
      ColorScheme.fromSeed(
        seedColor: palette.info,
        brightness: brightness,
        surface: palette.surface,
      ).copyWith(
        primary: palette.info,
        onPrimary: isDark ? const Color(0xFF061B26) : Colors.white,
        primaryContainer: palette.infoContainer,
        onPrimaryContainer: palette.info,
        secondaryContainer: palette.elevatedSurface,
        onSecondaryContainer: onSurface,
        surface: palette.surface,
        onSurface: onSurface,
        surfaceContainerHighest: palette.elevatedSurface,
        outline: palette.border,
        outlineVariant: palette.border,
      );

  return ThemeData(
    useMaterial3: true,
    brightness: brightness,
    colorScheme: colorScheme,
    scaffoldBackgroundColor: palette.canvas,
    dividerColor: palette.border,
    dividerTheme: DividerThemeData(
      color: palette.border,
      thickness: 1,
      space: 1,
    ),
    appBarTheme: AppBarTheme(
      backgroundColor: palette.surface,
      foregroundColor: onSurface,
      surfaceTintColor: Colors.transparent,
      elevation: 0,
      scrolledUnderElevation: 0,
      centerTitle: false,
    ),
    cardTheme: CardThemeData(
      color: palette.surface,
      surfaceTintColor: Colors.transparent,
      elevation: 0,
      margin: EdgeInsets.zero,
      clipBehavior: Clip.antiAlias,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(12),
        side: BorderSide(color: palette.border),
      ),
    ),
    chipTheme: ChipThemeData(
      backgroundColor: palette.elevatedSurface,
      selectedColor: palette.infoContainer,
      side: BorderSide(color: palette.border),
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(8)),
      labelStyle: TextStyle(
        fontSize: 12,
        fontWeight: FontWeight.w600,
        color: onSurface,
        fontFeatures: const <FontFeature>[FontFeature.tabularFigures()],
      ),
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 2),
    ),
    tabBarTheme: TabBarThemeData(
      labelColor: palette.info,
      unselectedLabelColor: palette.subtleText,
      indicatorColor: palette.info,
      dividerColor: palette.border,
      labelStyle: const TextStyle(fontSize: 13, fontWeight: FontWeight.w700),
      unselectedLabelStyle: const TextStyle(
        fontSize: 13,
        fontWeight: FontWeight.w500,
      ),
    ),
    dialogTheme: DialogThemeData(
      backgroundColor: palette.surface,
      surfaceTintColor: Colors.transparent,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(16),
        side: BorderSide(color: palette.border),
      ),
    ),
    bottomSheetTheme: BottomSheetThemeData(
      backgroundColor: palette.surface,
      surfaceTintColor: Colors.transparent,
      shape: const RoundedRectangleBorder(
        borderRadius: BorderRadius.vertical(top: Radius.circular(20)),
      ),
    ),
    filledButtonTheme: FilledButtonThemeData(
      style: FilledButton.styleFrom(
        elevation: 0,
        padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 10),
        minimumSize: const Size(0, 36),
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(8)),
        textStyle: const TextStyle(fontSize: 13, fontWeight: FontWeight.w600),
      ),
    ),
    outlinedButtonTheme: OutlinedButtonThemeData(
      style: OutlinedButton.styleFrom(
        padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
        minimumSize: const Size(0, 36),
        side: BorderSide(color: palette.border),
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(8)),
        textStyle: const TextStyle(fontSize: 13, fontWeight: FontWeight.w600),
      ),
    ),
    inputDecorationTheme: InputDecorationTheme(
      filled: true,
      fillColor: palette.elevatedSurface,
      isDense: true,
      contentPadding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
      border: OutlineInputBorder(
        borderRadius: BorderRadius.circular(10),
        borderSide: BorderSide(color: palette.border),
      ),
      enabledBorder: OutlineInputBorder(
        borderRadius: BorderRadius.circular(10),
        borderSide: BorderSide(color: palette.border),
      ),
      focusedBorder: OutlineInputBorder(
        borderRadius: BorderRadius.circular(10),
        borderSide: BorderSide(color: palette.info, width: 1.4),
      ),
    ),
    extensions: <ThemeExtension<dynamic>>[palette],
  );
}
