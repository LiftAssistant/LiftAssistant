import 'package:flutter/material.dart';

@immutable
class InstrumentPalette extends ThemeExtension<InstrumentPalette> {
  const InstrumentPalette({
    required this.canvas,
    required this.surface,
    required this.elevatedSurface,
    required this.border,
    required this.subtleText,
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

  static const InstrumentPalette dark = InstrumentPalette(
    canvas: Color(0xFF0B0F15),
    surface: Color(0xFF121821),
    elevatedSurface: Color(0xFF192230),
    border: Color(0xFF273447),
    subtleText: Color(0xFF94A3B8),
    ok: Color(0xFF3FB950),
    okContainer: Color(0xFF122D1F),
    caution: Color(0xFFE3B341),
    cautionContainer: Color(0xFF31250E),
    danger: Color(0xFFF85149),
    dangerContainer: Color(0xFF361619),
    info: Color(0xFF58A6FF),
    infoContainer: Color(0xFF12263F),
    operationalRole: Color(0xFF388BFD),
    operationalRoleContainer: Color(0xFF112645),
    auxiliaryRole: Color(0xFFA371F7),
    auxiliaryRoleContainer: Color(0xFF241A3B),
    neutral: Color(0xFF8B949E),
    neutralContainer: Color(0xFF1C232E),
  );

  static const InstrumentPalette light = InstrumentPalette(
    canvas: Color(0xFFF3F6FA),
    surface: Color(0xFFFFFFFF),
    elevatedSurface: Color(0xFFEBF0F6),
    border: Color(0xFFD0DAE5),
    subtleText: Color(0xFF526275),
    ok: Color(0xFF1A7F37),
    okContainer: Color(0xFFE3F6E8),
    caution: Color(0xFF9A6700),
    cautionContainer: Color(0xFFFFF4D6),
    danger: Color(0xFFCF222E),
    dangerContainer: Color(0xFFFFE7E9),
    info: Color(0xFF0969DA),
    infoContainer: Color(0xFFE4F0FF),
    operationalRole: Color(0xFF0550AE),
    operationalRoleContainer: Color(0xFFDDF4FF),
    auxiliaryRole: Color(0xFF6639BA),
    auxiliaryRoleContainer: Color(0xFFF3ECFF),
    neutral: Color(0xFF57606A),
    neutralContainer: Color(0xFFEAEEF2),
  );

  @override
  InstrumentPalette copyWith({
    Color? canvas,
    Color? surface,
    Color? elevatedSurface,
    Color? border,
    Color? subtleText,
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
  double fontSize = 13,
  FontWeight fontWeight = FontWeight.w600,
  Color? color,
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
    color: color ?? Theme.of(context).colorScheme.onSurface,
    fontFeatures: const <FontFeature>[FontFeature.tabularFigures()],
  );
}

ThemeData buildInstrumentTheme(Brightness brightness) {
  final bool isDark = brightness == Brightness.dark;
  final InstrumentPalette palette = isDark
      ? InstrumentPalette.dark
      : InstrumentPalette.light;

  final ColorScheme colorScheme = ColorScheme.fromSeed(
    seedColor: const Color(0xFF2563EB),
    brightness: brightness,
    surface: palette.surface,
  );

  return ThemeData(
    useMaterial3: true,
    brightness: brightness,
    colorScheme: colorScheme,
    scaffoldBackgroundColor: palette.canvas,
    dividerColor: palette.border,
    cardTheme: CardThemeData(
      color: palette.surface,
      elevation: 0,
      margin: EdgeInsets.zero,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(10),
        side: BorderSide(color: palette.border),
      ),
    ),
    inputDecorationTheme: InputDecorationTheme(
      filled: true,
      fillColor: palette.elevatedSurface,
      isDense: true,
      border: OutlineInputBorder(
        borderRadius: BorderRadius.circular(8),
        borderSide: BorderSide(color: palette.border),
      ),
      enabledBorder: OutlineInputBorder(
        borderRadius: BorderRadius.circular(8),
        borderSide: BorderSide(color: palette.border),
      ),
      focusedBorder: OutlineInputBorder(
        borderRadius: BorderRadius.circular(8),
        borderSide: BorderSide(color: palette.info, width: 1.5),
      ),
    ),
    extensions: <ThemeExtension<dynamic>>[palette],
  );
}
