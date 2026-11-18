import 'package:flutter/material.dart';

import '../../api/read_model.dart';
import '../../state/client_controller.dart';
import '../theme/instrument_theme.dart';
import 'semantic_badge.dart';

String formatDiagnosticLayerLabel(DiagnosticLayer layer) {
  switch (layer) {
    case DiagnosticLayer.transport:
      return 'Transport';
    case DiagnosticLayer.source:
      return 'Source';
    case DiagnosticLayer.mapping:
      return 'Mapping';
    case DiagnosticLayer.observation:
      return 'Observation';
    case DiagnosticLayer.readiness:
      return 'Readiness';
  }
}

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

  void selectLayer(DiagnosticLayer? layer) {
    _selectedLayer = layer;
    notifyListeners();
  }

  void toggleLayer(DiagnosticLayer layer) {
    _selectedLayer = _selectedLayer == layer ? null : layer;
    notifyListeners();
  }

  void selectRole(BindingRole? role) {
    _selectedRole = role;
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
    final bool hadStateChange =
        _selectedLayer != null || _selectedRole != null || !_newestFirst;
    _selectedLayer = null;
    _selectedRole = null;
    _newestFirst = true;
    if (searchController.text.isNotEmpty) {
      searchController.clear();
    } else if (hadStateChange) {
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

/// Opens the Pop-up Diagnostics Terminal (`DiagnosticsTerminalModal`).
/// Uses a generous M3 Modal Dialog on desktop/wide viewports and a full-height
/// M3 Modal Bottom Sheet on mobile/compact viewports.
Future<void> showDiagnosticsTerminalModal({
  required BuildContext context,
  required ClientController controller,
  required DiagnosticsFilterController filterController,
}) async {
  final Size viewport = MediaQuery.sizeOf(context);
  final bool useDesktopDialog = viewport.width >= 720 && viewport.height >= 480;

  if (useDesktopDialog) {
    await showDialog<void>(
      context: context,
      builder: (BuildContext dialogContext) {
        final Size dialogViewport = MediaQuery.sizeOf(dialogContext);
        final double dialogWidth = (dialogViewport.width * 0.84).clamp(
          560.0,
          920.0,
        );
        final double dialogHeight = (dialogViewport.height * 0.84).clamp(
          420.0,
          720.0,
        );
        return Dialog(
          key: const Key('diagnostics-terminal-modal'),
          insetPadding: const EdgeInsets.symmetric(
            horizontal: 24,
            vertical: 24,
          ),
          clipBehavior: Clip.antiAlias,
          child: SizedBox(
            width: dialogWidth,
            height: dialogHeight,
            child: DiagnosticsTerminalModalContent(
              controller: controller,
              filterController: filterController,
              onClose: () => Navigator.of(dialogContext).pop(),
            ),
          ),
        );
      },
    );
    return;
  }

  await showModalBottomSheet<void>(
    context: context,
    isScrollControlled: true,
    useSafeArea: true,
    clipBehavior: Clip.antiAlias,
    builder: (BuildContext sheetContext) {
      return FractionallySizedBox(
        key: const Key('diagnostics-terminal-modal'),
        heightFactor: 0.92,
        child: DiagnosticsTerminalModalContent(
          controller: controller,
          filterController: filterController,
          onClose: () => Navigator.of(sheetContext).pop(),
        ),
      );
    },
  );
}

class DiagnosticsTerminalModalContent extends StatelessWidget {
  const DiagnosticsTerminalModalContent({
    super.key,
    required this.controller,
    required this.filterController,
    required this.onClose,
  });

  final ClientController controller;
  final DiagnosticsFilterController filterController;
  final VoidCallback onClose;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    final Size viewport = MediaQuery.sizeOf(context);
    final double textScale = MediaQuery.textScalerOf(context).scale(1.0);
    final bool compactHeader = viewport.height < 640 || textScale > 1.35;
    final bool showNonFaultBanner = viewport.height >= 420;

    return ListenableBuilder(
      listenable: controller,
      builder: (BuildContext context, _) {
        final List<DiagnosticEntry> diagnostics =
            controller.state.snapshot?.recentDiagnostics ??
            const <DiagnosticEntry>[];

        return Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: <Widget>[
            // 1. M3 Sheet Header + Non-fault Evidence Banner
            Padding(
              padding: EdgeInsets.fromLTRB(
                compactHeader ? 12 : 20,
                compactHeader ? 8 : 16,
                compactHeader ? 8 : 14,
                compactHeader ? 6 : 12,
              ),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: <Widget>[
                  Row(
                    children: <Widget>[
                      if (!compactHeader) ...<Widget>[
                        Container(
                          width: 34,
                          height: 34,
                          decoration: BoxDecoration(
                            color: palette.infoContainer,
                            borderRadius: BorderRadius.circular(10),
                          ),
                          child: Icon(
                            Icons.terminal_rounded,
                            size: 18,
                            color: palette.info,
                          ),
                        ),
                        const SizedBox(width: 12),
                      ],
                      Expanded(
                        child: Row(
                          children: <Widget>[
                            const Flexible(
                              child: Text(
                                'Host Diagnostics',
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                                style: TextStyle(
                                  fontSize: 16,
                                  fontWeight: FontWeight.w700,
                                  letterSpacing: -0.2,
                                ),
                              ),
                            ),
                            const SizedBox(width: 8),
                            Badge.count(
                              count: diagnostics.length,
                              backgroundColor: palette.infoContainer,
                              textColor: palette.info,
                              textStyle: instrumentTabularStyle(
                                context,
                                fontSize: 11.5,
                                fontWeight: FontWeight.w700,
                                color: palette.info,
                              ),
                            ),
                          ],
                        ),
                      ),
                      IconButton(
                        key: const Key('diagnostics-modal-close'),
                        tooltip: 'Close diagnostics terminal',
                        visualDensity: compactHeader
                            ? VisualDensity.compact
                            : VisualDensity.standard,
                        onPressed: onClose,
                        icon: const Icon(Icons.close_rounded, size: 20),
                      ),
                    ],
                  ),
                  if (showNonFaultBanner) ...<Widget>[
                    SizedBox(height: compactHeader ? 6 : 10),
                    Container(
                      padding: EdgeInsets.symmetric(
                        horizontal: compactHeader ? 10 : 12,
                        vertical: compactHeader ? 5 : 8,
                      ),
                      decoration: BoxDecoration(
                        color: palette.elevatedSurface,
                        borderRadius: BorderRadius.circular(10),
                        border: Border.all(color: palette.border),
                      ),
                      child: Row(
                        crossAxisAlignment: CrossAxisAlignment.center,
                        children: <Widget>[
                          Icon(
                            Icons.info_outline_rounded,
                            size: 15,
                            color: palette.info,
                          ),
                          const SizedBox(width: 8),
                          Expanded(
                            child: Text(
                              'Machine-readable Host evidence only — does not assert a confirmed elevator equipment fault or repair action.',
                              maxLines: compactHeader ? 1 : 2,
                              overflow: TextOverflow.ellipsis,
                              style: TextStyle(
                                fontSize: compactHeader ? 11 : 12,
                                height: 1.3,
                                color: palette.subtleText,
                              ),
                            ),
                          ),
                        ],
                      ),
                    ),
                  ],
                ],
              ),
            ),
            const Divider(height: 1),

            // 2. Filter Toolbar + Virtualized Diagnostic Event Stream
            Expanded(
              child: Padding(
                padding: EdgeInsets.fromLTRB(
                  compactHeader ? 12 : 20,
                  compactHeader ? 8 : 14,
                  compactHeader ? 12 : 20,
                  compactHeader ? 8 : 16,
                ),
                child: DiagnosticEvidencePanel(
                  diagnostics: diagnostics,
                  filterController: filterController,
                  expandList: true,
                ),
              ),
            ),
          ],
        );
      },
    );
  }
}

/// Pinned-toolbar + virtualized [ListView.builder] diagnostics inspector.
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

        Widget buildContent({required bool expand}) {
          final Widget logSurface;
          if (ordered.isEmpty) {
            final Widget emptyBox = _EmptyDiagnosticsBox(
              hasSourceDiagnostics: widget.diagnostics.isNotEmpty,
            );
            logSurface = expand
                ? Align(
                    alignment: Alignment.topCenter,
                    child: SingleChildScrollView(child: emptyBox),
                  )
                : emptyBox;
          } else if (expand) {
            logSurface = Card(
              color: palette.surface,
              child: ListView.builder(
                key: const Key('diagnostics-list-view'),
                padding: EdgeInsets.zero,
                itemCount: ordered.length,
                itemBuilder: (BuildContext context, int index) {
                  return _DiagnosticLogRow(
                    entry: ordered[index],
                    showTopDivider: index > 0,
                  );
                },
              ),
            );
          } else {
            logSurface = Card(
              color: palette.surface,
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
            );
          }

          return Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            mainAxisSize: expand ? MainAxisSize.max : MainAxisSize.min,
            children: <Widget>[
              DiagnosticsFilterToolbar(controller: controller),
              const SizedBox(height: 12),
              if (expand) Expanded(child: logSurface) else logSurface,
            ],
          );
        }

        if (!widget.expandList) {
          return buildContent(expand: false);
        }

        return LayoutBuilder(
          builder: (BuildContext context, BoxConstraints constraints) {
            if (constraints.maxHeight < 210) {
              return SingleChildScrollView(child: buildContent(expand: false));
            }
            return buildContent(expand: true);
          },
        );
      },
    );
  }
}

class _EmptyDiagnosticsBox extends StatelessWidget {
  const _EmptyDiagnosticsBox({required this.hasSourceDiagnostics});

  final bool hasSourceDiagnostics;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    return Card(
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 20),
        child: Row(
          children: <Widget>[
            Icon(
              Icons.check_circle_outline_rounded,
              size: 18,
              color: palette.mutedText,
            ),
            const SizedBox(width: 10),
            Expanded(
              child: Text(
                hasSourceDiagnostics
                    ? 'No diagnostic entries match the active filter.'
                    : 'No recent diagnostic entries recorded by Host.',
                style: TextStyle(fontSize: 13, color: palette.mutedText),
              ),
            ),
          ],
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
    final double textScale = MediaQuery.textScalerOf(context).scale(1.0);

    return LayoutBuilder(
      builder: (BuildContext context, BoxConstraints constraints) {
        final bool inlineControlsOnTopRow =
            constraints.maxWidth >= 560 && textScale <= 1.25;

        final Widget searchField = ConstrainedBox(
          constraints: const BoxConstraints(minHeight: 38),
          child: SearchBar(
            key: const Key('diagnostics-search-input'),
            controller: controller.searchController,
            hintText: 'Search event id, source, session, car, entity…',
            elevation: const WidgetStatePropertyAll<double>(0),
            backgroundColor: WidgetStatePropertyAll<Color>(
              palette.elevatedSurface,
            ),
            side: WidgetStatePropertyAll<BorderSide>(
              BorderSide(color: palette.border),
            ),
            shape: WidgetStatePropertyAll<OutlinedBorder>(
              RoundedRectangleBorder(borderRadius: BorderRadius.circular(10)),
            ),
            padding: const WidgetStatePropertyAll<EdgeInsets>(
              EdgeInsets.symmetric(horizontal: 12),
            ),
            constraints: const BoxConstraints(minHeight: 38, maxHeight: 42),
            textStyle: WidgetStatePropertyAll<TextStyle>(
              instrumentTabularStyle(
                context,
                fontSize: 13,
                fontWeight: FontWeight.w500,
              ),
            ),
            hintStyle: WidgetStatePropertyAll<TextStyle>(
              TextStyle(fontSize: 12.5, color: palette.mutedText),
            ),
            leading: Icon(
              Icons.search_rounded,
              size: 18,
              color: palette.mutedText,
            ),
            trailing: controller.searchQuery.isEmpty
                ? null
                : <Widget>[
                    IconButton(
                      key: const Key('diagnostics-search-clear'),
                      tooltip: 'Clear search query',
                      onPressed: controller.clearSearch,
                      visualDensity: VisualDensity.compact,
                      icon: Icon(
                        Icons.close_rounded,
                        size: 16,
                        color: palette.subtleText,
                      ),
                    ),
                  ],
          ),
        );

        final Widget sortButton = FilledButton.tonalIcon(
          key: const Key('diagnostics-sort-button'),
          onPressed: controller.toggleSortOrder,
          style: FilledButton.styleFrom(
            backgroundColor: palette.elevatedSurface,
            foregroundColor: palette.subtleText,
            side: BorderSide(color: palette.border),
            padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
            minimumSize: const Size(0, 38),
            shape: RoundedRectangleBorder(
              borderRadius: BorderRadius.circular(10),
            ),
          ),
          icon: Icon(
            controller.newestFirst ? Icons.south_rounded : Icons.north_rounded,
            size: 15,
          ),
          label: Text(
            controller.newestFirst ? 'Newest' : 'Oldest',
            style: instrumentTabularStyle(
              context,
              fontSize: 12,
              fontWeight: FontWeight.w600,
              color: palette.subtleText,
            ),
          ),
        );

        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          mainAxisSize: MainAxisSize.min,
          children: <Widget>[
            if (inlineControlsOnTopRow)
              Row(
                children: <Widget>[
                  Expanded(child: searchField),
                  const SizedBox(width: 10),
                  _RoleSegmentedButton(controller: controller),
                  const SizedBox(width: 10),
                  sortButton,
                ],
              )
            else ...<Widget>[
              Row(
                children: <Widget>[
                  Expanded(child: searchField),
                  const SizedBox(width: 8),
                  sortButton,
                ],
              ),
              const SizedBox(height: 6),
              _RoleSegmentedButton(controller: controller),
            ],
            const SizedBox(height: 8),
            if (textScale > 1.35)
              SingleChildScrollView(
                scrollDirection: Axis.horizontal,
                child: Row(
                  children: <Widget>[
                    FilterChip(
                      key: const Key('diag-filter-all'),
                      label: const Text('All'),
                      selected:
                          controller.selectedLayer == null &&
                          controller.selectedRole == null,
                      showCheckmark: false,
                      visualDensity: VisualDensity.compact,
                      onSelected: (_) => controller.clearCategoryFilters(),
                    ),
                    for (final DiagnosticLayer layer
                        in DiagnosticLayer.values) ...<Widget>[
                      const SizedBox(width: 6),
                      FilterChip(
                        key: Key('diag-filter-layer-${layer.wireValue}'),
                        label: Text(formatDiagnosticLayerLabel(layer)),
                        selected: controller.selectedLayer == layer,
                        showCheckmark: false,
                        visualDensity: VisualDensity.compact,
                        onSelected: (_) => controller.toggleLayer(layer),
                      ),
                    ],
                  ],
                ),
              )
            else
              Wrap(
                spacing: 6,
                runSpacing: 6,
                crossAxisAlignment: WrapCrossAlignment.center,
                children: <Widget>[
                  FilterChip(
                    key: const Key('diag-filter-all'),
                    label: const Text('All'),
                    selected:
                        controller.selectedLayer == null &&
                        controller.selectedRole == null,
                    showCheckmark: false,
                    visualDensity: VisualDensity.compact,
                    onSelected: (_) => controller.clearCategoryFilters(),
                  ),
                  for (final DiagnosticLayer layer in DiagnosticLayer.values)
                    FilterChip(
                      key: Key('diag-filter-layer-${layer.wireValue}'),
                      label: Text(formatDiagnosticLayerLabel(layer)),
                      selected: controller.selectedLayer == layer,
                      showCheckmark: false,
                      visualDensity: VisualDensity.compact,
                      onSelected: (_) => controller.toggleLayer(layer),
                    ),
                ],
              ),
          ],
        );
      },
    );
  }
}

enum _RoleFilterOption { all, operational, auxiliary }

class _RoleSegmentedButton extends StatelessWidget {
  const _RoleSegmentedButton({required this.controller});

  final DiagnosticsFilterController controller;

  _RoleFilterOption get _currentOption {
    switch (controller.selectedRole) {
      case null:
        return _RoleFilterOption.all;
      case BindingRole.operational:
        return _RoleFilterOption.operational;
      case BindingRole.auxiliary:
        return _RoleFilterOption.auxiliary;
    }
  }

  @override
  Widget build(BuildContext context) {
    final double textScale = MediaQuery.textScalerOf(context).scale(1.0);
    if (textScale > 1.35) {
      return SingleChildScrollView(
        scrollDirection: Axis.horizontal,
        child: Row(
          children: <Widget>[
            ChoiceChip(
              key: const Key('diag-filter-role-all'),
              label: const Text('All Roles'),
              selected: controller.selectedRole == null,
              showCheckmark: false,
              visualDensity: VisualDensity.compact,
              onSelected: (_) => controller.selectRole(null),
            ),
            const SizedBox(width: 6),
            ChoiceChip(
              key: const Key('diag-filter-role-operational'),
              label: const Text('Operational'),
              selected: controller.selectedRole == BindingRole.operational,
              showCheckmark: false,
              visualDensity: VisualDensity.compact,
              onSelected: (_) => controller.toggleRole(BindingRole.operational),
            ),
            const SizedBox(width: 6),
            ChoiceChip(
              key: const Key('diag-filter-role-auxiliary'),
              label: const Text('Auxiliary'),
              selected: controller.selectedRole == BindingRole.auxiliary,
              showCheckmark: false,
              visualDensity: VisualDensity.compact,
              onSelected: (_) => controller.toggleRole(BindingRole.auxiliary),
            ),
          ],
        ),
      );
    }

    return SegmentedButton<_RoleFilterOption>(
      showSelectedIcon: false,
      style: SegmentedButton.styleFrom(
        visualDensity: VisualDensity.compact,
        padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
        textStyle: const TextStyle(fontSize: 12, fontWeight: FontWeight.w600),
      ),
      segments: const <ButtonSegment<_RoleFilterOption>>[
        ButtonSegment<_RoleFilterOption>(
          value: _RoleFilterOption.all,
          label: Text('All Roles', key: Key('diag-filter-role-all')),
        ),
        ButtonSegment<_RoleFilterOption>(
          value: _RoleFilterOption.operational,
          label: Text('Operational', key: Key('diag-filter-role-operational')),
        ),
        ButtonSegment<_RoleFilterOption>(
          value: _RoleFilterOption.auxiliary,
          label: Text('Auxiliary', key: Key('diag-filter-role-auxiliary')),
        ),
      ],
      selected: <_RoleFilterOption>{_currentOption},
      onSelectionChanged: (Set<_RoleFilterOption> selection) {
        final _RoleFilterOption chosen = selection.first;
        switch (chosen) {
          case _RoleFilterOption.all:
            controller.selectRole(null);
          case _RoleFilterOption.operational:
            controller.selectRole(BindingRole.operational);
          case _RoleFilterOption.auxiliary:
            controller.selectRole(BindingRole.auxiliary);
        }
      },
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
    final (Color layerFg, Color layerBg) = _layerColors(entry.layer, palette);
    final bool hasContext =
        entry.source != null ||
        entry.session != null ||
        entry.car != null ||
        entry.role != null ||
        entry.nativeEntity != null;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      mainAxisSize: MainAxisSize.min,
      children: <Widget>[
        if (showTopDivider) const Divider(height: 1),
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 11),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: <Widget>[
              // Primary diagnostic identity line: Sequence Badge + Layer Chip + Event ID
              Wrap(
                spacing: 8,
                runSpacing: 6,
                crossAxisAlignment: WrapCrossAlignment.center,
                children: <Widget>[
                  Container(
                    padding: const EdgeInsets.symmetric(
                      horizontal: 8,
                      vertical: 3,
                    ),
                    decoration: BoxDecoration(
                      color: palette.elevatedSurface,
                      borderRadius: BorderRadius.circular(6),
                      border: Border.all(color: palette.border),
                    ),
                    child: Text(
                      '#${entry.sequence}',
                      style: instrumentTabularStyle(
                        context,
                        fontSize: 11.5,
                        fontWeight: FontWeight.w700,
                        color: palette.subtleText,
                      ),
                    ),
                  ),
                  SemanticBadge(
                    label: formatDiagnosticLayerLabel(entry.layer),
                    foreground: layerFg,
                    background: layerBg,
                    dotColor: layerFg,
                    tooltip: 'Diagnostic layer: ${entry.layer.wireValue}',
                  ),
                  Text(
                    entry.id,
                    style: instrumentTabularStyle(
                      context,
                      fontSize: 13.5,
                      fontWeight: FontWeight.w700,
                    ),
                  ),
                ],
              ),

              // Structured M3 context chips (no raw key=value concatenation)
              if (hasContext) ...<Widget>[
                const SizedBox(height: 8),
                Wrap(
                  spacing: 6,
                  runSpacing: 6,
                  children: <Widget>[
                    if (entry.source != null)
                      _DiagnosticContextChip(
                        icon: Icons.hub_outlined,
                        label: 'Source',
                        value: entry.source!,
                      ),
                    if (entry.session != null)
                      _DiagnosticContextChip(
                        icon: Icons.link_rounded,
                        label: 'Session',
                        value: entry.session!,
                      ),
                    if (entry.car != null)
                      _DiagnosticContextChip(
                        icon: Icons.elevator_outlined,
                        label: 'Car',
                        value: entry.car!,
                      ),
                    if (entry.role != null)
                      _DiagnosticContextChip(
                        icon: Icons.verified_user_outlined,
                        label: 'Role',
                        value: entry.role == BindingRole.operational
                            ? 'Operational'
                            : 'Auxiliary',
                        accentColor: entry.role == BindingRole.operational
                            ? palette.operationalRole
                            : null,
                      ),
                    if (entry.nativeEntity != null)
                      _DiagnosticContextChip(
                        icon: Icons.memory_rounded,
                        label: 'Entity',
                        value: entry.nativeEntity!.isEmpty
                            ? '""'
                            : entry.nativeEntity!,
                      ),
                  ],
                ),
              ],
            ],
          ),
        ),
      ],
    );
  }

  (Color fg, Color bg) _layerColors(
    DiagnosticLayer layer,
    InstrumentPalette palette,
  ) {
    switch (layer) {
      case DiagnosticLayer.transport:
        return (palette.info, palette.infoContainer);
      case DiagnosticLayer.source:
        return (palette.subtleText, palette.elevatedSurface);
      case DiagnosticLayer.mapping:
        return (palette.caution, palette.cautionContainer);
      case DiagnosticLayer.observation:
        return (palette.info, palette.infoContainer);
      case DiagnosticLayer.readiness:
        return (palette.ok, palette.okContainer);
    }
  }
}

class _DiagnosticContextChip extends StatelessWidget {
  const _DiagnosticContextChip({
    required this.icon,
    required this.label,
    required this.value,
    this.accentColor,
  });

  final IconData icon;
  final String label;
  final String value;
  final Color? accentColor;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    final Color effectiveValueColor =
        accentColor ?? Theme.of(context).colorScheme.onSurface;

    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
      decoration: BoxDecoration(
        color: palette.elevatedSurface,
        borderRadius: BorderRadius.circular(6),
        border: Border.all(color: palette.border),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: <Widget>[
          Icon(icon, size: 13, color: accentColor ?? palette.mutedText),
          const SizedBox(width: 5),
          Flexible(
            child: Text.rich(
              TextSpan(
                children: <InlineSpan>[
                  TextSpan(
                    text: '$label: ',
                    style: TextStyle(
                      fontSize: 11.5,
                      fontWeight: FontWeight.w500,
                      color: palette.mutedText,
                    ),
                  ),
                  TextSpan(
                    text: value,
                    style: instrumentTabularStyle(
                      context,
                      fontSize: 11.5,
                      fontWeight: FontWeight.w600,
                      color: effectiveValueColor,
                    ),
                  ),
                ],
              ),
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
            ),
          ),
        ],
      ),
    );
  }
}
