import 'package:flutter/material.dart';

import '../../api/read_model.dart';
import '../theme/instrument_theme.dart';
import 'semantic_badge.dart';

class DiagnosticEvidencePanel extends StatefulWidget {
  const DiagnosticEvidencePanel({
    super.key,
    required this.diagnostics,
    this.maxPreviewEntries,
    this.onViewAll,
  });

  final List<DiagnosticEntry> diagnostics;
  final int? maxPreviewEntries;
  final VoidCallback? onViewAll;

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
    final bool isPreview = widget.maxPreviewEntries != null;

    Iterable<DiagnosticEntry> filtered = widget.diagnostics;
    if (!isPreview) {
      if (_selectedLayer != null) {
        filtered = filtered.where(
          (DiagnosticEntry d) => d.layer == _selectedLayer,
        );
      }
      if (_selectedRole != null) {
        filtered = filtered.where(
          (DiagnosticEntry d) => d.role == _selectedRole,
        );
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
    }

    final List<DiagnosticEntry> ordered = filtered.toList()
      ..sort(
        (DiagnosticEntry a, DiagnosticEntry b) => _newestFirst
            ? b.sequence.compareTo(a.sequence)
            : a.sequence.compareTo(b.sequence),
      );

    final List<DiagnosticEntry> visible = isPreview
        ? ordered.take(widget.maxPreviewEntries!).toList()
        : ordered;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: <Widget>[
        // Non-fault Semantic Evidence Notice
        Container(
          padding: const EdgeInsets.all(11),
          decoration: BoxDecoration(
            color: palette.elevatedSurface,
            borderRadius: BorderRadius.circular(8),
            border: Border.all(color: palette.border),
          ),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: <Widget>[
              Icon(
                Icons.fact_check_outlined,
                size: 17,
                color: palette.info,
              ),
              const SizedBox(width: 9),
              Expanded(
                child: Text(
                  'Machine-readable Host evidence only: diagnostic entries (such as transport failure, '
                  'mapping rejection, or observation rejection) are NOT confirmed elevator equipment faults, '
                  'root causes, or repair instructions.',
                  style: TextStyle(
                    fontSize: 12,
                    height: 1.35,
                    color: palette.subtleText,
                  ),
                ),
              ),
            ],
          ),
        ),
        if (!isPreview) ...<Widget>[
          const SizedBox(height: 12),
          _buildFilterControls(context, palette),
        ],
        const SizedBox(height: 12),
        if (visible.isEmpty)
          Card(
            child: Padding(
              padding: const EdgeInsets.all(20),
              child: Text(
                widget.diagnostics.isEmpty
                    ? 'Host has recorded zero recent diagnostic entries.'
                    : 'No diagnostic entries match the active filter.',
                style: TextStyle(fontSize: 13, color: palette.subtleText),
              ),
            ),
          )
        else
          Column(
            children: <Widget>[
              for (final DiagnosticEntry entry in visible)
                Padding(
                  padding: const EdgeInsets.only(bottom: 8),
                  child: _DiagnosticEntryRow(entry: entry),
                ),
              if (isPreview &&
                  widget.diagnostics.length > widget.maxPreviewEntries! &&
                  widget.onViewAll != null)
                Align(
                  alignment: Alignment.centerRight,
                  child: TextButton.icon(
                    onPressed: widget.onViewAll,
                    icon: const Icon(Icons.arrow_forward, size: 16),
                    label: Text(
                      'Inspect all ${widget.diagnostics.length} recent diagnostics',
                    ),
                  ),
                ),
            ],
          ),
      ],
    );
  }

  Widget _buildFilterControls(
    BuildContext context,
    InstrumentPalette palette,
  ) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: <Widget>[
        Row(
          children: <Widget>[
            Expanded(
              child: TextField(
                decoration: const InputDecoration(
                  hintText: 'Filter by id, source, car, session, or native entity...',
                  prefixIcon: Icon(Icons.search, size: 18),
                ),
                onChanged: (String value) {
                  setState(() {
                    _searchQuery = value;
                  });
                },
              ),
            ),
            const SizedBox(width: 10),
            OutlinedButton.icon(
              onPressed: () {
                setState(() {
                  _newestFirst = !_newestFirst;
                });
              },
              icon: Icon(
                _newestFirst ? Icons.south : Icons.north,
                size: 16,
              ),
              label: Text(_newestFirst ? 'Newest First' : 'Oldest First'),
            ),
          ],
        ),
        const SizedBox(height: 10),
        Wrap(
          spacing: 8,
          runSpacing: 8,
          children: <Widget>[
            FilterChip(
              label: const Text('All Layers'),
              selected: _selectedLayer == null,
              onSelected: (_) => setState(() => _selectedLayer = null),
            ),
            for (final DiagnosticLayer layer in DiagnosticLayer.values)
              FilterChip(
                label: Text(layer.wireValue),
                selected: _selectedLayer == layer,
                onSelected: (bool selected) {
                  setState(() {
                    _selectedLayer = selected ? layer : null;
                  });
                },
              ),
            const SizedBox(width: 6),
            FilterChip(
              label: const Text('Operational Role'),
              selected: _selectedRole == BindingRole.operational,
              onSelected: (bool selected) {
                setState(() {
                  _selectedRole = selected ? BindingRole.operational : null;
                });
              },
            ),
            FilterChip(
              label: const Text('Auxiliary Role'),
              selected: _selectedRole == BindingRole.auxiliary,
              onSelected: (bool selected) {
                setState(() {
                  _selectedRole = selected ? BindingRole.auxiliary : null;
                });
              },
            ),
          ],
        ),
      ],
    );
  }
}

class _DiagnosticEntryRow extends StatelessWidget {
  const _DiagnosticEntryRow({required this.entry});

  final DiagnosticEntry entry;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    final (Color layerFg, Color layerBg) = _layerColors(entry.layer, palette);

    return Card(
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 11),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: <Widget>[
            Wrap(
              spacing: 8,
              runSpacing: 6,
              crossAxisAlignment: WrapCrossAlignment.center,
              children: <Widget>[
                Text(
                  '#${entry.sequence}',
                  style: instrumentMonoStyle(
                    context,
                    fontSize: 12.5,
                    fontWeight: FontWeight.w700,
                    color: palette.subtleText,
                  ),
                ),
                SemanticBadge(
                  label: entry.layer.wireValue,
                  foreground: layerFg,
                  background: layerBg,
                  monospace: true,
                ),
                Text(
                  entry.id,
                  style: instrumentMonoStyle(
                    context,
                    fontSize: 13.5,
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
                    monospace: true,
                  ),
              ],
            ),
            if (entry.source != null ||
                entry.session != null ||
                entry.car != null ||
                entry.nativeEntity != null) ...<Widget>[
              const SizedBox(height: 8),
              Wrap(
                spacing: 12,
                runSpacing: 4,
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

  (Color, Color) _layerColors(
    DiagnosticLayer layer,
    InstrumentPalette palette,
  ) {
    switch (layer) {
      case DiagnosticLayer.transport:
        return (palette.info, palette.infoContainer);
      case DiagnosticLayer.source:
        return (palette.operationalRole, palette.operationalRoleContainer);
      case DiagnosticLayer.mapping:
        return (palette.caution, palette.cautionContainer);
      case DiagnosticLayer.observation:
        return (palette.auxiliaryRole, palette.auxiliaryRoleContainer);
      case DiagnosticLayer.readiness:
        return (palette.ok, palette.okContainer);
    }
  }
}

class _ContextPair extends StatelessWidget {
  const _ContextPair({
    required this.label,
    required this.value,
  });

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
            style: TextStyle(
              fontSize: 11.5,
              color: palette.subtleText,
            ),
          ),
          TextSpan(
            text: displayValue,
            style: instrumentMonoStyle(
              context,
              fontSize: 11.5,
              fontWeight: FontWeight.w600,
            ),
          ),
        ],
      ),
    );
  }
}
