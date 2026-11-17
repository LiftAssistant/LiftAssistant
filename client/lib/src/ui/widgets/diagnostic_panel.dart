import 'package:flutter/material.dart';

import '../../api/read_model.dart';
import '../theme/instrument_theme.dart';
import 'semantic_badge.dart';

class DiagnosticEvidencePanel extends StatefulWidget {
  const DiagnosticEvidencePanel({super.key, required this.diagnostics});

  final List<DiagnosticEntry> diagnostics;

  @override
  State<DiagnosticEvidencePanel> createState() =>
      _DiagnosticEvidencePanelState();
}

class _DiagnosticEvidencePanelState extends State<DiagnosticEvidencePanel> {
  DiagnosticLayer? _selectedLayer;
  BindingRole? _selectedRole;
  bool _newestFirst = true;
  String _searchQuery = '';

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;

    Iterable<DiagnosticEntry> filtered = widget.diagnostics;
    if (_selectedLayer != null) {
      filtered = filtered.where(
        (DiagnosticEntry d) => d.layer == _selectedLayer,
      );
    }
    if (_selectedRole != null) {
      filtered = filtered.where((DiagnosticEntry d) => d.role == _selectedRole);
    }
    final String q = _searchQuery.trim().toLowerCase();
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

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      mainAxisSize: MainAxisSize.min,
      children: <Widget>[
        _buildToolbar(context, palette),
        const SizedBox(height: 8),
        DecoratedBox(
          decoration: BoxDecoration(
            color: palette.surface,
            borderRadius: BorderRadius.circular(6),
            border: Border.all(color: palette.border),
          ),
          child: ClipRRect(
            borderRadius: BorderRadius.circular(5),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              mainAxisSize: MainAxisSize.min,
              children: <Widget>[
                // Compact log table header + non-fault evidence boundary cue
                DecoratedBox(
                  decoration: BoxDecoration(
                    color: palette.elevatedSurface,
                    border: Border(bottom: BorderSide(color: palette.border)),
                  ),
                  child: Padding(
                    padding: const EdgeInsets.symmetric(
                      horizontal: 12,
                      vertical: 6.5,
                    ),
                    child: Wrap(
                      spacing: 8,
                      runSpacing: 2,
                      alignment: WrapAlignment.spaceBetween,
                      crossAxisAlignment: WrapCrossAlignment.center,
                      children: <Widget>[
                        Text(
                          'SEQ · LAYER · EVENT ID',
                          style: instrumentMonoStyle(
                            context,
                            fontSize: 10.5,
                            fontWeight: FontWeight.w600,
                            color: palette.mutedText,
                          ),
                        ),
                        Tooltip(
                          message: 'Diagnostic entries are machine-readable Host evidence only — NOT confirmed elevator equipment faults, root causes, or repair instructions.',
                          child: Text(
                            'Host evidence · non-fault',
                            style: TextStyle(
                              fontSize: 11,
                              color: palette.mutedText,
                            ),
                          ),
                        ),
                      ],
                    ),
                  ),
                ),

                if (ordered.isEmpty)
                  Padding(
                    padding: const EdgeInsets.symmetric(
                      horizontal: 12,
                      vertical: 16,
                    ),
                    child: Text(
                      widget.diagnostics.isEmpty
                          ? 'Zero recent diagnostic entries recorded by Host.'
                          : 'No entries match active filter.',
                      style: TextStyle(fontSize: 12, color: palette.mutedText),
                    ),
                  )
                else
                  for (int i = 0; i < ordered.length; i++)
                    _DiagnosticLogRow(entry: ordered[i], showTopDivider: i > 0),
              ],
            ),
          ),
        ),
      ],
    );
  }

  Widget _buildToolbar(BuildContext context, InstrumentPalette palette) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: <Widget>[
        Row(
          children: <Widget>[
            Expanded(
              child: SizedBox(
                height: 32,
                child: TextField(
                  key: const Key('diagnostics-search-input'),
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
                      minHeight: 32,
                    ),
                    contentPadding: const EdgeInsets.symmetric(
                      horizontal: 8,
                      vertical: 0,
                    ),
                  ),
                  onChanged: (String value) {
                    setState(() {
                      _searchQuery = value;
                    });
                  },
                ),
              ),
            ),
            const SizedBox(width: 6),
            Tooltip(
              message: _newestFirst
                  ? 'Sorted newest sequence first (tap to reverse)'
                  : 'Sorted oldest sequence first (tap to reverse)',
              child: InkWell(
                key: const Key('diagnostics-sort-button'),
                borderRadius: BorderRadius.circular(5),
                onTap: () {
                  setState(() {
                    _newestFirst = !_newestFirst;
                  });
                },
                child: Container(
                  height: 32,
                  padding: const EdgeInsets.symmetric(horizontal: 9),
                  decoration: BoxDecoration(
                    color: palette.elevatedSurface,
                    borderRadius: BorderRadius.circular(5),
                    border: Border.all(color: palette.border),
                  ),
                  child: Row(
                    mainAxisSize: MainAxisSize.min,
                    children: <Widget>[
                      Icon(
                        _newestFirst ? Icons.south : Icons.north,
                        size: 13,
                        color: palette.subtleText,
                      ),
                      const SizedBox(width: 4),
                      Text(
                        '#seq',
                        style: instrumentMonoStyle(
                          context,
                          fontSize: 11,
                          color: palette.subtleText,
                        ),
                      ),
                    ],
                  ),
                ),
              ),
            ),
          ],
        ),
        const SizedBox(height: 6),
        Wrap(
          spacing: 5,
          runSpacing: 5,
          children: <Widget>[
            _FilterPill(
              key: const Key('diag-filter-all'),
              label: 'All',
              selected: _selectedLayer == null && _selectedRole == null,
              onTap: () {
                setState(() {
                  _selectedLayer = null;
                  _selectedRole = null;
                });
              },
            ),
            for (final DiagnosticLayer layer in DiagnosticLayer.values)
              _FilterPill(
                key: Key('diag-filter-layer-${layer.wireValue}'),
                label: layer.wireValue,
                monospace: true,
                selected: _selectedLayer == layer,
                onTap: () {
                  setState(() {
                    _selectedLayer = _selectedLayer == layer ? null : layer;
                  });
                },
              ),
            _FilterPill(
              key: const Key('diag-filter-role-operational'),
              label: 'role:operational',
              monospace: true,
              selected: _selectedRole == BindingRole.operational,
              onTap: () {
                setState(() {
                  _selectedRole = _selectedRole == BindingRole.operational
                      ? null
                      : BindingRole.operational;
                });
              },
            ),
            _FilterPill(
              key: const Key('diag-filter-role-auxiliary'),
              label: 'role:auxiliary',
              monospace: true,
              selected: _selectedRole == BindingRole.auxiliary,
              onTap: () {
                setState(() {
                  _selectedRole = _selectedRole == BindingRole.auxiliary
                      ? null
                      : BindingRole.auxiliary;
                });
              },
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
        padding: const EdgeInsets.symmetric(horizontal: 7, vertical: 3.5),
        decoration: BoxDecoration(
          color: bg,
          borderRadius: BorderRadius.circular(4),
          border: Border.all(color: borderColor),
        ),
        child: Text(
          label,
          style: monospace
              ? instrumentMonoStyle(
                  context,
                  fontSize: 11,
                  fontWeight: selected ? FontWeight.w700 : FontWeight.w500,
                  color: fg,
                )
              : TextStyle(
                  fontSize: 11,
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
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: <Widget>[
            Wrap(
              spacing: 8,
              runSpacing: 4,
              crossAxisAlignment: WrapCrossAlignment.center,
              children: <Widget>[
                Text(
                  '#${entry.sequence}',
                  style: instrumentMonoStyle(
                    context,
                    fontSize: 11.5,
                    fontWeight: FontWeight.w600,
                    color: palette.mutedText,
                  ),
                ),
                Row(
                  mainAxisSize: MainAxisSize.min,
                  children: <Widget>[
                    StatusDot(color: layerColor, size: 6),
                    const SizedBox(width: 4.5),
                    Text(
                      entry.layer.wireValue,
                      style: instrumentMonoStyle(
                        context,
                        fontSize: 11.5,
                        fontWeight: FontWeight.w600,
                        color: layerColor,
                      ),
                    ),
                  ],
                ),
                Text(
                  entry.id,
                  style: instrumentMonoStyle(
                    context,
                    fontSize: 12.5,
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
              const SizedBox(height: 4),
              Wrap(
                spacing: 10,
                runSpacing: 3,
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
    return RichText(
      text: TextSpan(
        style: DefaultTextStyle.of(context).style,
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
