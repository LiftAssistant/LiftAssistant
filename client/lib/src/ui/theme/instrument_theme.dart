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

  /// Matte carbon / graphite console palette (ISA-101 restrained signal hierarchy).
  static const InstrumentPalette dark = InstrumentPalette(
    canvas: Color(0xFF111214),
    surface: Color(0xFF17191C),
    elevatedSurface: Color(0xFF1E2025),
    border: Color(0xFF2A2D34),
    subtleText: Color(0xFF939AA6),
    mutedText: Color(0xFF666D7A),
    ok: Color(0xFF2FA36B),
    okContainer: Color(0xFF14271F),
    caution: Color(0xFFE59934),
    cautionContainer: Color(0xFF2B2012),
    danger: Color(0xFFE5484D),
    dangerContainer: Color(0xFF2C1517),
    info: Color(0xFF32B8C6),
    infoContainer: Color(0xFF13262B),
    operationalRole: Color(0xFFD6DCE5),
    operationalRoleContainer: Color(0xFF23272F),
    auxiliaryRole: Color(0xFF8B929E),
    auxiliaryRoleContainer: Color(0xFF1A1C20),
    neutral: Color(0xFF8B929E),
    neutralContainer: Color(0xFF202329),
  );

  /// Warm architectural paper / daylight field palette.
  static const InstrumentPalette light = InstrumentPalette(
    canvas: Color(0xFFF3F3F0),
    surface: Color(0xFFFFFFFF),
    elevatedSurface: Color(0xFFEBEBE6),
    border: Color(0xFFD5D5CE),
    subtleText: Color(0xFF555B66),
    mutedText: Color(0xFF7A808C),
    ok: Color(0xFF1B7F4B),
    okContainer: Color(0xFFE4F4EB),
    caution: Color(0xFFB45309),
    cautionContainer: Color(0xFFFEF3C7),
    danger: Color(0xFFC92A2A),
    dangerContainer: Color(0xFFFEE2E2),
    info: Color(0xFF0B7285),
    infoContainer: Color(0xFFE3F5F8),
    operationalRole: Color(0xFF1E232B),
    operationalRoleContainer: Color(0xFFE6E7E2),
    auxiliaryRole: Color(0xFF5E6470),
    auxiliaryRoleContainer: Color(0xFFF5F5F2),
    neutral: Color(0xFF5E6470),
    neutralContainer: Color(0xFFEAEAE5),
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

TextStyle instrumentMonoStyle(
  BuildContext context, {
  double fontSize = 12.5,
  FontWeight fontWeight = FontWeight.w600,
  Color? color,
  double? height,
}) {
  return TextStyle(
    fontFamily: 'monospace',
    fontFamilyFallback: const <String>[
      'JetBrains Mono',
      'Fira Code',
      'DejaVu Sans Mono',
      'Liberation Mono',
      'monospace',
    ],
    fontSize: fontSize,
    fontWeight: fontWeight,
    height: height,
    color: color ?? Theme.of(context).colorScheme.onSurface,
    fontFeatures: const <FontFeature>[FontFeature.tabularFigures()],
  );
}

ThemeData buildInstrumentTheme(Brightness brightness) {
  final bool isDark = brightness == Brightness.dark;
  final InstrumentPalette palette = isDark
      ? InstrumentPalette.dark
      : InstrumentPalette.light;

  final Color onSurface = isDark
      ? const Color(0xFFECEEF2)
      : const Color(0xFF16181C);

  final ColorScheme colorScheme =
      ColorScheme.fromSeed(
        seedColor: palette.info,
        brightness: brightness,
        surface: palette.surface,
      ).copyWith(
        primary: palette.info,
        onPrimary: isDark ? const Color(0xFF07191C) : Colors.white,
        surface: palette.surface,
        onSurface: onSurface,
        outline: palette.border,
      );

  return ThemeData(
    useMaterial3: true,
    brightness: brightness,
    colorScheme: colorScheme,
    scaffoldBackgroundColor: palette.canvas,
    dividerColor: palette.border,
    cardTheme: CardThemeData(
      color: palette.surface,
      surfaceTintColor: Colors.transparent,
      elevation: 0,
      margin: EdgeInsets.zero,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(6),
        side: BorderSide(color: palette.border),
      ),
    ),
    filledButtonTheme: FilledButtonThemeData(
      style: FilledButton.styleFrom(
        elevation: 0,
        padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
        minimumSize: const Size(0, 34),
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(5)),
        textStyle: const TextStyle(fontSize: 12.5, fontWeight: FontWeight.w600),
      ),
    ),
    outlinedButtonTheme: OutlinedButtonThemeData(
      style: OutlinedButton.styleFrom(
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
        minimumSize: const Size(0, 34),
        side: BorderSide(color: palette.border),
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(5)),
        textStyle: const TextStyle(fontSize: 12.5, fontWeight: FontWeight.w600),
      ),
    ),
    inputDecorationTheme: InputDecorationTheme(
      filled: true,
      fillColor: palette.elevatedSurface,
      isDense: true,
      contentPadding: const EdgeInsets.symmetric(horizontal: 10, vertical: 9),
      border: OutlineInputBorder(
        borderRadius: BorderRadius.circular(5),
        borderSide: BorderSide(color: palette.border),
      ),
      enabledBorder: OutlineInputBorder(
        borderRadius: BorderRadius.circular(5),
        borderSide: BorderSide(color: palette.border),
      ),
      focusedBorder: OutlineInputBorder(
        borderRadius: BorderRadius.circular(5),
        borderSide: BorderSide(color: palette.info, width: 1.2),
      ),
    ),
    extensions: <ThemeExtension<dynamic>>[palette],
  );
}
