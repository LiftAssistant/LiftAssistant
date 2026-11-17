import 'package:flutter/material.dart';

import '../api/read_model.dart';
import '../state/client_controller.dart';
import '../state/client_state.dart';
import 'theme/instrument_theme.dart';
import 'widgets/car_card.dart';
import 'widgets/diagnostic_panel.dart';
import 'widgets/host_connection_dialog.dart';
import 'widgets/semantic_badge.dart';
import 'widgets/source_card.dart';
import 'widgets/system_status_strip.dart';

enum CompactSection { cars, sources, diagnostics }

class ClientShellPage extends StatefulWidget {
  const ClientShellPage({
    super.key,
    required this.controller,
    required this.themeMode,
    required this.onToggleThemeMode,
  });

  final ClientController controller;
  final ThemeMode themeMode;
  final VoidCallback onToggleThemeMode;

  @override
  State<ClientShellPage> createState() => _ClientShellPageState();
}

class _ClientShellPageState extends State<ClientShellPage> {
  CompactSection _compactSection = CompactSection.cars;

  void _openHostConfig(ClientState state) {
    showHostConnectionModal(
      context: context,
      currentUrl: state.endpoint?.origin ?? state.rawUrlInput,
      isConnectedOrConnecting: state.isActivelyManaged,
      onConnect: (String url) async {
        await widget.controller.connectToUrl(url);
      },
      onDisconnect: () async {
        await widget.controller.disconnect();
      },
    );
  }

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: widget.controller,
      builder: (BuildContext context, _) {
        final ClientState state = widget.controller.state;
        final Size viewport = MediaQuery.sizeOf(context);
        final bool isSplitDesktop =
            viewport.width >= 920 && viewport.height >= 460;

        return Scaffold(
          appBar: _buildTopBar(context, state, viewport.width),
          body: SafeArea(
            child: state.snapshot == null
                ? _buildNoSnapshotWorkspace(context, state)
                : _buildConnectedWorkspace(
                    context,
                    state,
                    state.snapshot!,
                    isSplitDesktop: isSplitDesktop,
                  ),
          ),
        );
      },
    );
  }

  PreferredSizeWidget _buildTopBar(
    BuildContext context,
    ClientState state,
    double screenWidth,
  ) {
    final InstrumentPalette palette = context.palette;
    final bool showReadOnlyTag = screenWidth >= 620;
    final bool showEndpointOriginInline = screenWidth >= 540;
    final (String phaseLabel, Color phaseColor, Color _, bool phaseHollow) =
        describeConnectionPhase(state.phase, palette);

    return PreferredSize(
      preferredSize: const Size.fromHeight(46),
      child: DecoratedBox(
        decoration: BoxDecoration(
          color: palette.surface,
          border: Border(bottom: BorderSide(color: palette.border)),
        ),
        child: SafeArea(
          bottom: false,
          child: SizedBox(
            height: 46,
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 12),
              child: Row(
                children: <Widget>[
                  Expanded(
                    child: Row(
                      children: <Widget>[
                        const _HoistwayBrandMark(),
                        const SizedBox(width: 8),
                        const Flexible(
                          child: Text(
                            'LiftAssistant',
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: TextStyle(
                              fontSize: 14.5,
                              fontWeight: FontWeight.w600,
                            ),
                          ),
                        ),
                        if (showReadOnlyTag) ...<Widget>[
                          const SizedBox(width: 8),
                          Tooltip(
                            message: 'Strictly read-only experimental/v0 client; zero elevator write controls.',
                            child: Container(
                              padding: const EdgeInsets.symmetric(
                                horizontal: 6,
                                vertical: 2,
                              ),
                              decoration: BoxDecoration(
                                borderRadius: BorderRadius.circular(4),
                                border: Border.all(color: palette.border),
                              ),
                              child: Text(
                                'read-only',
                                style: instrumentMonoStyle(
                                  context,
                                  fontSize: 10.5,
                                  fontWeight: FontWeight.w500,
                                  color: palette.mutedText,
                                ),
                              ),
                            ),
                          ),
                        ],
                      ],
                    ),
                  ),
                  const SizedBox(width: 8),

                  // Unified Host Endpoint + Connection Phase Control
                  ConstrainedBox(
                    constraints: BoxConstraints(
                      maxWidth: showEndpointOriginInline
                          ? 340
                          : (screenWidth * 0.42).clamp(110.0, 168.0),
                    ),
                    child: InkWell(
                      key: const Key('topbar-host-button'),
                      borderRadius: BorderRadius.circular(5),
                      onTap: () => _openHostConfig(state),
                      child: Container(
                        height: 30,
                        padding: const EdgeInsets.symmetric(horizontal: 8),
                        decoration: BoxDecoration(
                          color: palette.elevatedSurface,
                          borderRadius: BorderRadius.circular(5),
                          border: Border.all(color: palette.border),
                        ),
                        child: Row(
                          mainAxisSize: MainAxisSize.min,
                          children: <Widget>[
                            Flexible(
                              child: Row(
                                key: const Key('topbar-connection-phase-badge'),
                                mainAxisSize: MainAxisSize.min,
                                children: <Widget>[
                                  StatusDot(
                                    color: phaseColor,
                                    hollow: phaseHollow,
                                    size: 6.5,
                                  ),
                                  const SizedBox(width: 5),
                                  Flexible(
                                    child: Text(
                                      phaseLabel,
                                      maxLines: 1,
                                      overflow: TextOverflow.ellipsis,
                                      style: instrumentMonoStyle(
                                        context,
                                        fontSize: 10.5,
                                        fontWeight: FontWeight.w700,
                                        color: phaseColor,
                                      ),
                                    ),
                                  ),
                                ],
                              ),
                            ),
                            Container(
                              width: 1,
                              height: 13,
                              margin: const EdgeInsets.symmetric(horizontal: 7),
                              color: palette.border,
                            ),
                            if (showEndpointOriginInline)
                              ConstrainedBox(
                                constraints: const BoxConstraints(
                                  maxWidth: 190,
                                ),
                                child: Text(
                                  state.endpoint?.origin ?? 'No host',
                                  maxLines: 1,
                                  overflow: TextOverflow.ellipsis,
                                  style: instrumentMonoStyle(
                                    context,
                                    fontSize: 11.5,
                                    fontWeight: FontWeight.w500,
                                    color: palette.subtleText,
                                  ),
                                ),
                              )
                            else
                              Icon(
                                Icons.dns_outlined,
                                size: 13.5,
                                color: palette.subtleText,
                              ),
                          ],
                        ),
                      ),
                    ),
                  ),
                  const SizedBox(width: 4),
                  if (state.endpoint != null)
                    _TopBarIconButton(
                      key: const Key('topbar-reconnect-button'),
                      tooltip: 'Reconnect & refresh snapshot',
                      onPressed: widget.controller.reconnectNow,
                      icon: Icons.refresh_rounded,
                    ),
                  _TopBarIconButton(
                    key: const Key('topbar-semantics-button'),
                    tooltip: 'experimental/v0 semantic reference',
                    onPressed: () => showSemanticsReferenceModal(context),
                    icon: Icons.help_outline_rounded,
                  ),
                  _TopBarIconButton(
                    key: const Key('topbar-theme-toggle'),
                    tooltip: widget.themeMode == ThemeMode.dark
                        ? 'Switch to daylight theme'
                        : 'Switch to carbon theme',
                    onPressed: widget.onToggleThemeMode,
                    icon: widget.themeMode == ThemeMode.dark
                        ? Icons.light_mode_outlined
                        : Icons.dark_mode_outlined,
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }

  Widget _buildNoSnapshotWorkspace(BuildContext context, ClientState state) {
    final InstrumentPalette palette = context.palette;

    return LayoutBuilder(
      builder: (BuildContext context, BoxConstraints viewportConstraints) {
        final double minCenteredHeight = (viewportConstraints.maxHeight - 32)
            .clamp(0.0, double.infinity);
        return SingleChildScrollView(
          padding: const EdgeInsets.all(16),
          child: ConstrainedBox(
            constraints: BoxConstraints(minHeight: minCenteredHeight),
            child: Center(
              child: ConstrainedBox(
                constraints: const BoxConstraints(maxWidth: 460),
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: <Widget>[
                    if (state.phase == ConnectionPhase.connecting) ...<Widget>[
                      DecoratedBox(
                        decoration: BoxDecoration(
                          color: palette.surface,
                          borderRadius: BorderRadius.circular(6),
                          border: Border.all(color: palette.border),
                        ),
                        child: Padding(
                          padding: const EdgeInsets.symmetric(
                            horizontal: 12,
                            vertical: 10,
                          ),
                          child: Row(
                            children: <Widget>[
                              const SizedBox(
                                width: 15,
                                height: 15,
                                child: CircularProgressIndicator(
                                  strokeWidth: 2,
                                ),
                              ),
                              const SizedBox(width: 10),
                              Expanded(
                                child: Text(
                                  'Requesting snapshot from ${state.endpoint?.origin ?? "Host"}...',
                                  maxLines: 1,
                                  overflow: TextOverflow.ellipsis,
                                  style: const TextStyle(
                                    fontSize: 12.5,
                                    fontWeight: FontWeight.w600,
                                  ),
                                ),
                              ),
                              TextButton(
                                onPressed: widget.controller.disconnect,
                                child: const Text('Cancel'),
                              ),
                            ],
                          ),
                        ),
                      ),
                      const SizedBox(height: 12),
                    ] else if (state.error != null) ...<Widget>[
                      ClientAlertBanners(
                        state: state,
                        onReconnectNow: widget.controller.reconnectNow,
                        onConfigureHost: () => _openHostConfig(state),
                      ),
                      const SizedBox(height: 12),
                    ],
                    DecoratedBox(
                      decoration: BoxDecoration(
                        color: palette.surface,
                        borderRadius: BorderRadius.circular(6),
                        border: Border.all(color: palette.border),
                      ),
                      child: Padding(
                        padding: const EdgeInsets.all(18),
                        child: HostConnectionForm(
                          initialUrl:
                              state.endpoint?.origin ?? state.rawUrlInput,
                          isConnectedOrConnecting: state.isActivelyManaged,
                          onConnect: (String url) async {
                            await widget.controller.connectToUrl(url);
                          },
                          onDisconnect: () async {
                            await widget.controller.disconnect();
                          },
                        ),
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ),
        );
      },
    );
  }

  Widget _buildConnectedWorkspace(
    BuildContext context,
    ClientState state,
    ReadModelSnapshot snapshot, {
    required bool isSplitDesktop,
  }) {
    final InstrumentPalette palette = context.palette;
    final bool hasAlerts =
        state.hasRetainedStaleSnapshot || state.error != null;

    if (isSplitDesktop) {
      return Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: <Widget>[
          HostTelemetryBar(
            snapshot: snapshot,
            isLive: state.isSnapshotLive,
            revisionReset: state.lastRevisionReset,
          ),
          if (hasAlerts)
            Padding(
              padding: const EdgeInsets.fromLTRB(16, 12, 16, 0),
              child: ClientAlertBanners(
                state: state,
                onReconnectNow: widget.controller.reconnectNow,
                onConfigureHost: () => _openHostConfig(state),
              ),
            ),
          Expanded(
            child: AnimatedOpacity(
              duration: const Duration(milliseconds: 150),
              opacity: state.isSnapshotLive ? 1.0 : 0.72,
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: <Widget>[
                  // Left primary stage: Operational Cars + Observation Sources
                  Expanded(
                    flex: 6,
                    child: SingleChildScrollView(
                      padding: const EdgeInsets.all(16),
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.stretch,
                        children: <Widget>[
                          SectionHeader(
                            title: 'Operational Cars',
                            count: snapshot.operationalCars.length,
                            trailingNote: 'Engine operational truth',
                          ),
                          const SizedBox(height: 10),
                          _buildCarsGridOrEmpty(context, snapshot),
                          const SizedBox(height: 20),
                          SectionHeader(
                            title: 'Observation Sources',
                            count: snapshot.sources.length,
                            trailingNote:
                                'Session · transport · payload · role',
                          ),
                          const SizedBox(height: 10),
                          _buildSourcesListOrEmpty(context, snapshot),
                        ],
                      ),
                    ),
                  ),
                  VerticalDivider(
                    width: 1,
                    thickness: 1,
                    color: palette.border,
                  ),
                  // Right inspector pane: Full Host Diagnostics log
                  Expanded(
                    flex: 5,
                    child: SingleChildScrollView(
                      padding: const EdgeInsets.all(16),
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.stretch,
                        children: <Widget>[
                          SectionHeader(
                            title: 'Host Diagnostics',
                            count: snapshot.recentDiagnostics.length,
                          ),
                          const SizedBox(height: 10),
                          DiagnosticEvidencePanel(
                            diagnostics: snapshot.recentDiagnostics,
                          ),
                        ],
                      ),
                    ),
                  ),
                ],
              ),
            ),
          ),
        ],
      );
    }

    // Compact / Mobile / Landscape layout: pinned telemetry + slim 3-tab bar (zero duplicate Overview!)
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: <Widget>[
        HostTelemetryBar(
          snapshot: snapshot,
          isLive: state.isSnapshotLive,
          revisionReset: state.lastRevisionReset,
        ),
        _buildCompactTabStrip(context, snapshot),
        Expanded(
          child: SingleChildScrollView(
            padding: const EdgeInsets.all(12),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: <Widget>[
                if (hasAlerts) ...<Widget>[
                  ClientAlertBanners(
                    state: state,
                    onReconnectNow: widget.controller.reconnectNow,
                    onConfigureHost: () => _openHostConfig(state),
                  ),
                  const SizedBox(height: 12),
                ],
                AnimatedOpacity(
                  duration: const Duration(milliseconds: 150),
                  opacity: state.isSnapshotLive ? 1.0 : 0.72,
                  child: switch (_compactSection) {
                    CompactSection.cars => _buildCarsGridOrEmpty(
                      context,
                      snapshot,
                    ),
                    CompactSection.sources => _buildSourcesListOrEmpty(
                      context,
                      snapshot,
                    ),
                    CompactSection.diagnostics => DiagnosticEvidencePanel(
                      diagnostics: snapshot.recentDiagnostics,
                    ),
                  },
                ),
              ],
            ),
          ),
        ),
      ],
    );
  }

  Widget _buildCompactTabStrip(
    BuildContext context,
    ReadModelSnapshot snapshot,
  ) {
    final InstrumentPalette palette = context.palette;
    return DecoratedBox(
      key: const Key('compact-section-strip'),
      decoration: BoxDecoration(
        color: palette.surface,
        border: Border(bottom: BorderSide(color: palette.border)),
      ),
      child: SizedBox(
        height: 36,
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: <Widget>[
            _CompactTabButton(
              key: const Key('compact-tab-cars'),
              label: 'Cars',
              count: snapshot.operationalCars.length,
              selected: _compactSection == CompactSection.cars,
              onTap: () =>
                  setState(() => _compactSection = CompactSection.cars),
            ),
            _CompactTabButton(
              key: const Key('compact-tab-sources'),
              label: 'Sources',
              count: snapshot.sources.length,
              selected: _compactSection == CompactSection.sources,
              onTap: () =>
                  setState(() => _compactSection = CompactSection.sources),
            ),
            _CompactTabButton(
              key: const Key('compact-tab-diagnostics'),
              label: 'Diagnostics',
              count: snapshot.recentDiagnostics.length,
              selected: _compactSection == CompactSection.diagnostics,
              onTap: () =>
                  setState(() => _compactSection = CompactSection.diagnostics),
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildCarsGridOrEmpty(
    BuildContext context,
    ReadModelSnapshot snapshot,
  ) {
    if (snapshot.operationalCars.isEmpty) {
      return _EmptyOperationalCarsPanel(sources: snapshot.sources);
    }

    return LayoutBuilder(
      builder: (BuildContext context, BoxConstraints constraints) {
        final int columns =
            (constraints.maxWidth >= 540 &&
                snapshot.operationalCars.length >= 2)
            ? 2
            : 1;
        const double spacing = 10;
        final double cardWidth =
            (constraints.maxWidth - spacing * (columns - 1)) / columns;

        return Wrap(
          spacing: spacing,
          runSpacing: spacing,
          children: <Widget>[
            for (final OperationalCar car in snapshot.operationalCars)
              SizedBox(
                width: cardWidth,
                child: OperationalCarCard(car: car),
              ),
          ],
        );
      },
    );
  }

  Widget _buildSourcesListOrEmpty(
    BuildContext context,
    ReadModelSnapshot snapshot,
  ) {
    final InstrumentPalette palette = context.palette;
    if (snapshot.sources.isEmpty) {
      return DecoratedBox(
        decoration: BoxDecoration(
          color: palette.surface,
          borderRadius: BorderRadius.circular(6),
          border: Border.all(color: palette.border),
        ),
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 12),
          child: Text(
            'No observation sources configured on this Host.',
            style: TextStyle(fontSize: 12, color: palette.mutedText),
          ),
        ),
      );
    }

    return Column(
      children: <Widget>[
        for (int i = 0; i < snapshot.sources.length; i++) ...<Widget>[
          if (i > 0) const SizedBox(height: 8),
          SourceHealthCard(source: snapshot.sources[i]),
        ],
      ],
    );
  }
}

class _HoistwayBrandMark extends StatelessWidget {
  const _HoistwayBrandMark();

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    return Container(
      width: 18,
      height: 18,
      decoration: BoxDecoration(
        borderRadius: BorderRadius.circular(3.5),
        border: Border.all(color: palette.info, width: 1.4),
      ),
      child: Center(
        child: Container(
          width: 7,
          height: 8,
          decoration: BoxDecoration(
            color: palette.info,
            borderRadius: BorderRadius.circular(1.2),
          ),
        ),
      ),
    );
  }
}

class _TopBarIconButton extends StatelessWidget {
  const _TopBarIconButton({
    super.key,
    required this.tooltip,
    required this.onPressed,
    required this.icon,
  });

  final String tooltip;
  final VoidCallback onPressed;
  final IconData icon;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    return IconButton(
      tooltip: tooltip,
      onPressed: onPressed,
      style: IconButton.styleFrom(
        minimumSize: const Size(30, 30),
        padding: const EdgeInsets.all(6),
        tapTargetSize: MaterialTapTargetSize.shrinkWrap,
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(5)),
      ),
      icon: Icon(icon, size: 16.5, color: palette.subtleText),
    );
  }
}

class _CompactTabButton extends StatelessWidget {
  const _CompactTabButton({
    super.key,
    required this.label,
    required this.count,
    required this.selected,
    required this.onTap,
  });

  final String label;
  final int count;
  final bool selected;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    final Color fg = selected
        ? Theme.of(context).colorScheme.onSurface
        : palette.subtleText;

    return Expanded(
      child: InkWell(
        onTap: onTap,
        child: DecoratedBox(
          decoration: BoxDecoration(
            border: selected
                ? Border(bottom: BorderSide(color: palette.info, width: 2))
                : null,
          ),
          child: Center(
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: <Widget>[
                Flexible(
                  child: Text(
                    label,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(
                      fontSize: 12,
                      fontWeight: selected ? FontWeight.w700 : FontWeight.w500,
                      color: fg,
                    ),
                  ),
                ),
                const SizedBox(width: 4),
                Text(
                  '$count',
                  style: instrumentMonoStyle(
                    context,
                    fontSize: 11,
                    fontWeight: FontWeight.w600,
                    color: selected ? palette.info : palette.mutedText,
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _EmptyOperationalCarsPanel extends StatelessWidget {
  const _EmptyOperationalCarsPanel({required this.sources});

  final List<SourceHealth> sources;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    final List<
      (String sourceId, SourceObservation obs, PayloadActivity payload)
    >
    pendingOperational = <(String, SourceObservation, PayloadActivity)>[
      for (final SourceHealth src in sources)
        for (final SourceObservation obs in src.observations)
          if (obs.role == BindingRole.operational)
            (src.source, obs, src.payload),
    ];

    return DecoratedBox(
      key: const Key('empty-operational-cars-card'),
      decoration: BoxDecoration(
        color: palette.surface,
        borderRadius: BorderRadius.circular(6),
        border: Border.all(color: palette.border),
      ),
      child: Padding(
        padding: const EdgeInsets.all(14),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: <Widget>[
            Row(
              children: <Widget>[
                StatusDot(color: palette.caution, hollow: true, size: 7),
                const SizedBox(width: 7),
                const Expanded(
                  child: Text(
                    'No resolved operational cars',
                    style: TextStyle(fontSize: 13, fontWeight: FontWeight.w700),
                  ),
                ),
              ],
            ),
            const SizedBox(height: 4),
            Text(
              'Requires complete location, motion, directed door, and mode from an Operational authority (Auxiliary bindings never enter operational truth).',
              style: TextStyle(
                fontSize: 11.5,
                height: 1.35,
                color: palette.subtleText,
              ),
            ),
            if (pendingOperational.isNotEmpty) ...<Widget>[
              const SizedBox(height: 10),
              Wrap(
                spacing: 6,
                runSpacing: 6,
                children: <Widget>[
                  for (final (
                        String sourceId,
                        SourceObservation obs,
                        PayloadActivity payload,
                      )
                      in pendingOperational)
                    Container(
                      padding: const EdgeInsets.symmetric(
                        horizontal: 8,
                        vertical: 4,
                      ),
                      decoration: BoxDecoration(
                        color: palette.elevatedSurface,
                        borderRadius: BorderRadius.circular(4),
                        border: Border.all(color: palette.border),
                      ),
                      child: Text(
                        '${obs.car} · ${obs.availability.wireValue} ($sourceId, payload:${payload.wireValue})',
                        style: instrumentMonoStyle(
                          context,
                          fontSize: 11,
                          fontWeight: FontWeight.w500,
                          color: palette.subtleText,
                        ),
                      ),
                    ),
                ],
              ),
            ],
          ],
        ),
      ),
    );
  }
}
