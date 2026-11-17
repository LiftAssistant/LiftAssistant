import 'package:flutter/material.dart';

import '../../api/read_model.dart';
import '../theme/instrument_theme.dart';
import 'semantic_badge.dart';

class DiagnosticsFilterController extends ChangeNotifier {
  DiagnosticsFilterController() {
    searchController.addListener(_onSearchChanged);
  }

  final TextEditingController searchController = TextEditingController();
  DiagnosticLayer? _selectedLayer;
  BindingRole? _selectedRole;
  bool _newestFirst = true;

  DiagnosticLayer? get selectedLayer => _selectedLayer;
  BindingRole? get selectedRole => _selectedRole;
  bool get newestFirst => _newestFirst;
  String get searchQuery => searchController.text;

  bool get hasActiveFilter =>
      _selectedLayer != null ||
      _selectedRole != null ||
      searchController.text.trim().isNotEmpty;

  void _onSearchChanged() {
    notifyListeners();
  }

  void toggleLayer(DiagnosticLayer layer) {
    _selectedLayer = _selectedLayer == layer ? null : layer;
    notifyListeners();
  }

  void toggleRole(BindingRole role) {
    _selectedRole = _selectedRole == role ? null : role;
    notifyListeners();
  }

  void clearCategoryFilters() {
    if (_selectedLayer == null && _selectedRole == null) {
      return;
    }
    _selectedLayer = null;
    _selectedRole = null;
    notifyListeners();
  }

  void clearSearch() {
    if (searchController.text.isEmpty) {
      return;
    }
    searchController.clear();
  }

  void resetAll() {
    _selectedLayer = null;
    _selectedRole = null;
    _newestFirst = true;
    if (searchController.text.isNotEmpty) {
      searchController.clear();
    } else {
      notifyListeners();
    }
  }

  void toggleSortOrder() {
    _newestFirst = !_newestFirst;
    notifyListeners();
  }

  List<DiagnosticEntry> filterAndSort(List<DiagnosticEntry> diagnostics) {
    Iterable<DiagnosticEntry> filtered = diagnostics;
    if (_selectedLayer != null) {
      filtered = filtered.where(
        (DiagnosticEntry d) => d.layer == _selectedLayer,
      );
    }
    if (_selectedRole != null) {
      filtered = filtered.where((DiagnosticEntry d) => d.role == _selectedRole);
    }
    final String q = searchController.text.trim().toLowerCase();
    if (q.isNotEmpty) {
      filtered = filtered.where((DiagnosticEntry d) {
        return d.id.toLowerCase().contains(q) ||
            (d.source?.toLowerCase().contains(q) ?? false) ||
            (d.car?.toLowerCase().contains(q) ?? false) ||
            (d.nativeEntity?.toLowerCase().contains(q) ?? false) ||
            (d.session?.toLowerCase().contains(q) ?? false);
      });
    }

    final List<DiagnosticEntry> ordered = filtered.toList()
      ..sort(
        (DiagnosticEntry a, DiagnosticEntry b) => _newestFirst
            ? b.sequence.compareTo(a.sequence)
            : a.sequence.compareTo(b.sequence),
      );
    return ordered;
  }

  @override
  void dispose() {
    searchController.removeListener(_onSearchChanged);
    searchController.dispose();
    super.dispose();
  }
}

/// Pinned-toolbar + virtualized [ListView.builder] diagnostics inspector for
/// bounded-height desktop panes (or standalone shrinking panel when [expandList] is false).
class DiagnosticEvidencePanel extends StatefulWidget {
  const DiagnosticEvidencePanel({
    super.key,
    required this.diagnostics,
    this.filterController,
    this.expandList = false,
  });

  final List<DiagnosticEntry> diagnostics;
  final DiagnosticsFilterController? filterController;
  final bool expandList;

  @override
  State<DiagnosticEvidencePanel> createState() =>
      _DiagnosticEvidencePanelState();
}

class _DiagnosticEvidencePanelState extends State<DiagnosticEvidencePanel> {
  DiagnosticsFilterController? _internalController;

  DiagnosticsFilterController get _effectiveController =>
      widget.filterController ??
      (_internalController ??= DiagnosticsFilterController());

  @override
  void didUpdateWidget(covariant DiagnosticEvidencePanel oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.filterController != null && _internalController != null) {
      _internalController!.dispose();
      _internalController = null;
    }
  }

  @override
  void dispose() {
    _internalController?.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    final DiagnosticsFilterController controller = _effectiveController;

    return ListenableBuilder(
      listenable: controller,
      builder: (BuildContext context, _) {
        final List<DiagnosticEntry> ordered = controller.filterAndSort(
          widget.diagnostics,
        );

        final Widget logSurface;
        if (ordered.isEmpty) {
          logSurface = _EmptyDiagnosticsBox(
            hasSourceDiagnostics: widget.diagnostics.isNotEmpty,
          );
        } else if (widget.expandList) {
          logSurface = DecoratedBox(
            decoration: BoxDecoration(
              color: palette.surface,
              borderRadius: BorderRadius.circular(6),
              border: Border.all(color: palette.border),
            ),
            child: ClipRRect(
              borderRadius: BorderRadius.circular(5),
              child: ListView.builder(
                padding: EdgeInsets.zero,
                shrinkWrap: ordered.length <= 30,
                itemCount: ordered.length,
                itemBuilder: (BuildContext context, int index) {
                  return _DiagnosticLogRow(
                    entry: ordered[index],
                    showTopDivider: index > 0,
                  );
                },
              ),
            ),
          );
        } else {
          logSurface = DecoratedBox(
            decoration: BoxDecoration(
              color: palette.surface,
              borderRadius: BorderRadius.circular(6),
              border: Border.all(color: palette.border),
            ),
            child: ClipRRect(
              borderRadius: BorderRadius.circular(5),
              child: ListView.builder(
                padding: EdgeInsets.zero,
                shrinkWrap: true,
                physics: const NeverScrollableScrollPhysics(),
                itemCount: ordered.length,
                itemBuilder: (BuildContext context, int index) {
                  return _DiagnosticLogRow(
                    entry: ordered[index],
                    showTopDivider: index > 0,
                  );
                },
              ),
            ),
          );
        }

        return Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          mainAxisSize: widget.expandList ? MainAxisSize.max : MainAxisSize.min,
          children: <Widget>[
            DiagnosticsFilterToolbar(controller: controller),
            const SizedBox(height: 8),
            if (widget.expandList) Flexible(child: logSurface) else logSurface,
          ],
        );
      },
    );
  }
}

/// Builds lazy slivers (`Toolbar` + virtualized `SliverList.builder`) for use
/// inside a [CustomScrollView] so large diagnostic logs never materialize offscreen rows.
List<Widget> buildDiagnosticSlivers({
  required BuildContext context,
  required List<DiagnosticEntry> diagnostics,
  required DiagnosticsFilterController controller,
  bool showCompactCue = false,
}) {
  final InstrumentPalette palette = context.palette;
  final List<DiagnosticEntry> ordered = controller.filterAndSort(diagnostics);

  return <Widget>[
    SliverToBoxAdapter(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: <Widget>[
          if (showCompactCue) ...<Widget>[
            Align(
              alignment: Alignment.centerRight,
              child: Tooltip(
                message:
                    'Diagnostic entries are machine-readable Host evidence only — NOT confirmed elevator equipment faults, root causes, or repair instructions.',
                child: Text(
                  'Host evidence · non-fault',
                  style: TextStyle(fontSize: 11, color: palette.mutedText),
                ),
              ),
            ),
            const SizedBox(height: 6),
          ],
          DiagnosticsFilterToolbar(controller: controller),
          const SizedBox(height: 8),
        ],
      ),
    ),
    if (ordered.isEmpty)
      SliverToBoxAdapter(
        child: _EmptyDiagnosticsBox(
          hasSourceDiagnostics: diagnostics.isNotEmpty,
        ),
      )
    else
      SliverList.builder(
        itemCount: ordered.length,
        itemBuilder: (BuildContext context, int index) {
          final bool isFirst = index == 0;
          final bool isLast = index == ordered.length - 1;
          final BorderRadius radius = BorderRadius.vertical(
            top: isFirst ? const Radius.circular(6) : Radius.zero,
            bottom: isLast ? const Radius.circular(6) : Radius.zero,
          );

          return DecoratedBox(
            decoration: BoxDecoration(
              color: palette.surface,
              borderRadius: radius,
              border: Border(
                top: BorderSide(color: palette.border),
                left: BorderSide(color: palette.border),
                right: BorderSide(color: palette.border),
                bottom: isLast
                    ? BorderSide(color: palette.border)
                    : BorderSide.none,
              ),
            ),
            child: _DiagnosticLogRow(
              entry: ordered[index],
              showTopDivider: false,
            ),
          );
        },
      ),
  ];
}

class _EmptyDiagnosticsBox extends StatelessWidget {
  const _EmptyDiagnosticsBox({required this.hasSourceDiagnostics});

  final bool hasSourceDiagnostics;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    return DecoratedBox(
      decoration: BoxDecoration(
        color: palette.surface,
        borderRadius: BorderRadius.circular(6),
        border: Border.all(color: palette.border),
      ),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 14),
        child: Text(
          hasSourceDiagnostics
              ? 'No diagnostic entries match active filter.'
              : 'No recent diagnostic entries recorded by Host.',
          style: TextStyle(fontSize: 12, color: palette.mutedText),
        ),
      ),
    );
  }
}

class DiagnosticsFilterToolbar extends StatelessWidget {
  const DiagnosticsFilterToolbar({super.key, required this.controller});

  final DiagnosticsFilterController controller;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      mainAxisSize: MainAxisSize.min,
      children: <Widget>[
        Row(
          children: <Widget>[
            Expanded(
              child: ConstrainedBox(
                constraints: const BoxConstraints(minHeight: 32),
                child: TextField(
                  key: const Key('diagnostics-search-input'),
                  controller: controller.searchController,
                  style: const TextStyle(fontSize: 12.5),
                  decoration: InputDecoration(
                    hintText: 'Filter id, source, car, entity...',
                    hintStyle: TextStyle(
                      fontSize: 12,
                      color: palette.mutedText,
                    ),
                    prefixIcon: Icon(
                      Icons.search,
                      size: 15,
                      color: palette.mutedText,
                    ),
                    prefixIconConstraints: const BoxConstraints(
                      minWidth: 32,
                      minHeight: 30,
                    ),
                    suffixIcon: controller.searchQuery.isEmpty
                        ? null
                        : IconButton(
                            key: const Key('diagnostics-search-clear'),
                            tooltip: 'Clear filter query',
                            onPressed: controller.clearSearch,
                            padding: EdgeInsets.zero,
                            visualDensity: VisualDensity.compact,
                            icon: Icon(
                              Icons.close_rounded,
                              size: 14,
                              color: palette.subtleText,
                            ),
                          ),
                    suffixIconConstraints: const BoxConstraints(
                      minWidth: 28,
                      minHeight: 28,
                    ),
                    contentPadding: const EdgeInsets.symmetric(
                      horizontal: 8,
                      vertical: 6,
                    ),
                  ),
                ),
              ),
            ),
            const SizedBox(width: 6),
            Flexible(
              child: Tooltip(
                message: controller.newestFirst
                    ? 'Sorted newest sequence first (tap to reverse)'
                    : 'Sorted oldest sequence first (tap to reverse)',
                child: InkWell(
                  key: const Key('diagnostics-sort-button'),
                  borderRadius: BorderRadius.circular(5),
                  onTap: controller.toggleSortOrder,
                  child: ConstrainedBox(
                    constraints: const BoxConstraints(minHeight: 32),
                    child: DecoratedBox(
                      decoration: BoxDecoration(
                        color: palette.elevatedSurface,
                        borderRadius: BorderRadius.circular(5),
                        border: Border.all(color: palette.border),
                      ),
                      child: Padding(
                        padding: const EdgeInsets.symmetric(
                          horizontal: 9,
                          vertical: 5,
                        ),
                        child: Row(
                          mainAxisSize: MainAxisSize.min,
                          children: <Widget>[
                            Icon(
                              controller.newestFirst
                                  ? Icons.south
                                  : Icons.north,
                              size: 13,
                              color: palette.subtleText,
                            ),
                            const SizedBox(width: 4),
                            Flexible(
                              child: Text(
                                '#seq',
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                                style: instrumentMonoStyle(
                                  context,
                                  fontSize: 11,
                                  color: palette.subtleText,
                                ),
                              ),
                            ),
                          ],
                        ),
                      ),
                    ),
                  ),
                ),
              ),
            ),
          ],
        ),
        const SizedBox(height: 6),
        Wrap(
          spacing: 4.5,
          runSpacing: 4.5,
          crossAxisAlignment: WrapCrossAlignment.center,
          children: <Widget>[
            _FilterPill(
              key: const Key('diag-filter-all'),
              label: 'All',
              selected:
                  controller.selectedLayer == null &&
                  controller.selectedRole == null,
              onTap: controller.clearCategoryFilters,
            ),
            for (final DiagnosticLayer layer in DiagnosticLayer.values)
              _FilterPill(
                key: Key('diag-filter-layer-${layer.wireValue}'),
                label: layer.wireValue,
                monospace: true,
                selected: controller.selectedLayer == layer,
                onTap: () => controller.toggleLayer(layer),
              ),
            _FilterPill(
              key: const Key('diag-filter-role-operational'),
              label: 'Operational',
              selected: controller.selectedRole == BindingRole.operational,
              onTap: () => controller.toggleRole(BindingRole.operational),
            ),
            _FilterPill(
              key: const Key('diag-filter-role-auxiliary'),
              label: 'Auxiliary',
              selected: controller.selectedRole == BindingRole.auxiliary,
              onTap: () => controller.toggleRole(BindingRole.auxiliary),
            ),
          ],
        ),
      ],
    );
  }
}

class _FilterPill extends StatelessWidget {
  const _FilterPill({
    super.key,
    required this.label,
    required this.selected,
    required this.onTap,
    this.monospace = false,
  });

  final String label;
  final bool selected;
  final VoidCallback onTap;
  final bool monospace;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    final Color fg = selected ? palette.info : palette.subtleText;
    final Color bg = selected ? palette.infoContainer : palette.surface;
    final Color borderColor = selected
        ? palette.info.withValues(alpha: 0.45)
        : palette.border;

    return InkWell(
      borderRadius: BorderRadius.circular(4),
      onTap: onTap,
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 6.5, vertical: 3),
        decoration: BoxDecoration(
          color: bg,
          borderRadius: BorderRadius.circular(4),
          border: Border.all(color: borderColor),
        ),
        child: Text(
          label,
          maxLines: 1,
          overflow: TextOverflow.ellipsis,
          style: monospace
              ? instrumentMonoStyle(
                  context,
                  fontSize: 10.5,
                  fontWeight: selected ? FontWeight.w700 : FontWeight.w500,
                  color: fg,
                )
              : TextStyle(
                  fontSize: 10.5,
                  fontWeight: selected ? FontWeight.w700 : FontWeight.w500,
                  color: fg,
                ),
        ),
      ),
    );
  }
}

class _DiagnosticLogRow extends StatelessWidget {
  const _DiagnosticLogRow({required this.entry, required this.showTopDivider});

  final DiagnosticEntry entry;
  final bool showTopDivider;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    final Color layerColor = _layerColor(entry.layer, palette);
    final bool hasContext =
        entry.source != null ||
        entry.session != null ||
        entry.car != null ||
        entry.nativeEntity != null;

    return DecoratedBox(
      decoration: BoxDecoration(
        border: showTopDivider
            ? Border(top: BorderSide(color: palette.border))
            : null,
      ),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 11, vertical: 7.5),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: <Widget>[
            Wrap(
              spacing: 7,
              runSpacing: 3,
              crossAxisAlignment: WrapCrossAlignment.center,
              children: <Widget>[
                Text(
                  '#${entry.sequence}',
                  style: instrumentMonoStyle(
                    context,
                    fontSize: 11,
                    fontWeight: FontWeight.w600,
                    color: palette.mutedText,
                  ),
                ),
                Row(
                  mainAxisSize: MainAxisSize.min,
                  children: <Widget>[
                    StatusDot(color: layerColor, size: 6),
                    const SizedBox(width: 4.5),
                    Flexible(
                      child: Text(
                        entry.layer.wireValue,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: instrumentMonoStyle(
                          context,
                          fontSize: 11.5,
                          fontWeight: FontWeight.w600,
                          color: layerColor,
                        ),
                      ),
                    ),
                  ],
                ),
                Text(
                  entry.id,
                  style: instrumentMonoStyle(
                    context,
                    fontSize: 12,
                    fontWeight: FontWeight.w700,
                  ),
                ),
                if (entry.role != null)
                  SemanticBadge(
                    label: entry.role!.wireValue,
                    foreground: entry.role == BindingRole.operational
                        ? palette.operationalRole
                        : palette.auxiliaryRole,
                    background: entry.role == BindingRole.operational
                        ? palette.operationalRoleContainer
                        : palette.auxiliaryRoleContainer,
                    outlined: entry.role == BindingRole.auxiliary,
                    monospace: true,
                  ),
              ],
            ),
            if (hasContext) ...<Widget>[
              const SizedBox(height: 3.5),
              Wrap(
                spacing: 10,
                runSpacing: 2.5,
                children: <Widget>[
                  if (entry.source != null)
                    _ContextPair(label: 'source', value: entry.source!),
                  if (entry.session != null)
                    _ContextPair(label: 'session', value: entry.session!),
                  if (entry.car != null)
                    _ContextPair(label: 'car', value: entry.car!),
                  if (entry.nativeEntity != null)
                    _ContextPair(
                      label: 'native_entity',
                      value: entry.nativeEntity!,
                    ),
                ],
              ),
            ],
          ],
        ),
      ),
    );
  }

  Color _layerColor(DiagnosticLayer layer, InstrumentPalette palette) {
    switch (layer) {
      case DiagnosticLayer.transport:
        return palette.info;
      case DiagnosticLayer.source:
        return palette.subtleText;
      case DiagnosticLayer.mapping:
        return palette.caution;
      case DiagnosticLayer.observation:
        return palette.info;
      case DiagnosticLayer.readiness:
        return palette.ok;
    }
  }
}

class _ContextPair extends StatelessWidget {
  const _ContextPair({required this.label, required this.value});

  final String label;
  final String value;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    final String displayValue = value.isEmpty ? '""' : value;
    // Use Text.rich rather than raw RichText so MediaQuery.textScalerOf(context) applies.
    return Text.rich(
      TextSpan(
        children: <InlineSpan>[
          TextSpan(
            text: '$label=',
            style: instrumentMonoStyle(
              context,
              fontSize: 11,
              fontWeight: FontWeight.w400,
              color: palette.mutedText,
            ),
          ),
          TextSpan(
            text: displayValue,
            style: instrumentMonoStyle(
              context,
              fontSize: 11,
              fontWeight: FontWeight.w600,
              color: palette.subtleText,
            ),
          ),
        ],
      ),
    );
  }
}
