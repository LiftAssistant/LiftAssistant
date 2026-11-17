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

enum ClientSection {
  overview,
  cars,
  sources,
  diagnostics,
}

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
  ClientSection _section = ClientSection.overview;

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
        final double width = MediaQuery.sizeOf(context).width;
        final bool isWide = width >= 760;

        return Scaffold(
          appBar: _buildTopBar(context, state, isWide),
          bottomNavigationBar: isWide || state.snapshot == null
              ? null
              : _buildBottomNav(context, state.snapshot!),
          body: SafeArea(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: <Widget>[
                if (!isWide) _buildCompactEndpointBar(context, state),
                Expanded(
                  child: Row(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: <Widget>[
                      if (isWide && state.snapshot != null) ...<Widget>[
                        _buildSideRail(context, state.snapshot!),
                        VerticalDivider(
                          width: 1,
                          thickness: 1,
                          color: context.palette.border,
                        ),
                      ],
                      Expanded(
                        child: _buildBodyContent(context, state, isWide),
                      ),
                    ],
                  ),
                ),
              ],
            ),
          ),
        );
      },
    );
  }

  Widget _buildCompactEndpointBar(BuildContext context, ClientState state) {
    final InstrumentPalette palette = context.palette;
    return DecoratedBox(
      decoration: BoxDecoration(
        color: palette.elevatedSurface,
        border: Border(bottom: BorderSide(color: palette.border)),
      ),
      child: InkWell(
        onTap: () => _openHostConfig(state),
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 7),
          child: Row(
            children: <Widget>[
              SemanticBadge(
                label: 'READ-ONLY',
                foreground: palette.info,
                background: palette.infoContainer,
              ),
              const SizedBox(width: 8),
              Icon(Icons.dns_outlined, size: 13, color: palette.subtleText),
              const SizedBox(width: 5),
              Expanded(
                child: Text(
                  state.endpoint?.origin ?? 'Tap to configure Host URL',
                  overflow: TextOverflow.ellipsis,
                  style: instrumentMonoStyle(
                    context,
                    fontSize: 11.5,
                    fontWeight: FontWeight.w600,
                  ),
                ),
              ),
              Icon(Icons.tune_rounded, size: 14, color: palette.subtleText),
            ],
          ),
        ),
      ),
    );
  }

  PreferredSizeWidget _buildTopBar(
    BuildContext context,
    ClientState state,
    bool isWide,
  ) {
    final InstrumentPalette palette = context.palette;
    final double screenWidth = MediaQuery.sizeOf(context).width;
    final bool showReadOnlyHeaderBadge = screenWidth >= 960;
    final (
      String phaseLabel,
      Color phaseFg,
      Color phaseBg,
      IconData phaseIcon,
    ) = describeConnectionPhase(state.phase, palette);

    return AppBar(
      backgroundColor: palette.surface,
      surfaceTintColor: Colors.transparent,
      elevation: 0,
      titleSpacing: isWide ? 16 : 12,
      bottom: PreferredSize(
        preferredSize: const Size.fromHeight(1),
        child: Divider(height: 1, thickness: 1, color: palette.border),
      ),
      title: Row(
        children: <Widget>[
          Icon(
            Icons.precision_manufacturing_outlined,
            size: 19,
            color: palette.info,
          ),
          const SizedBox(width: 8),
          const Flexible(
            child: Text(
              'LiftAssistant',
              overflow: TextOverflow.ellipsis,
              style: TextStyle(
                fontSize: 16,
                fontWeight: FontWeight.w800,
                letterSpacing: -0.3,
              ),
            ),
          ),
          if (showReadOnlyHeaderBadge) ...<Widget>[
            const SizedBox(width: 10),
            Flexible(
              child: SemanticBadge(
                label: 'READ-ONLY CLIENT',
                foreground: palette.info,
                background: palette.infoContainer,
                tooltip:
                    'Strictly read-only experimental/v0 client; exposes zero elevator control or write paths.',
              ),
            ),
          ],
        ],
      ),
      actions: <Widget>[
        if (isWide) ...<Widget>[
          Padding(
            padding: const EdgeInsets.symmetric(vertical: 8),
            child: InkWell(
              key: const Key('topbar-host-button'),
              borderRadius: BorderRadius.circular(8),
              onTap: () => _openHostConfig(state),
              child: Container(
                padding: const EdgeInsets.symmetric(
                  horizontal: 10,
                  vertical: 5,
                ),
                decoration: BoxDecoration(
                  color: palette.elevatedSurface,
                  borderRadius: BorderRadius.circular(8),
                  border: Border.all(color: palette.border),
                ),
                child: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: <Widget>[
                    Icon(
                      Icons.dns_outlined,
                      size: 14,
                      color: palette.subtleText,
                    ),
                    const SizedBox(width: 6),
                    ConstrainedBox(
                      constraints: const BoxConstraints(maxWidth: 220),
                      child: Text(
                        state.endpoint?.origin ?? 'Configure Host',
                        overflow: TextOverflow.ellipsis,
                        style: instrumentMonoStyle(
                          context,
                          fontSize: 12,
                          fontWeight: FontWeight.w600,
                        ),
                      ),
                    ),
                    const SizedBox(width: 4),
                    Icon(
                      Icons.tune_rounded,
                      size: 14,
                      color: palette.subtleText,
                    ),
                  ],
                ),
              ),
            ),
          ),
          const SizedBox(width: 8),
        ] else
          IconButton(
            key: const Key('topbar-host-button'),
            visualDensity: VisualDensity.compact,
            tooltip: 'Configure Host endpoint',
            onPressed: () => _openHostConfig(state),
            icon: const Icon(Icons.dns_outlined, size: 18),
          ),
        Center(
          child: ConstrainedBox(
            constraints: BoxConstraints(maxWidth: isWide ? 200 : 128),
            child: SemanticBadge(
              key: const Key('topbar-connection-phase-badge'),
              label: phaseLabel,
              foreground: phaseFg,
              background: phaseBg,
              icon: phaseIcon,
            ),
          ),
        ),
        if (state.endpoint != null)
          IconButton(
            key: const Key('topbar-reconnect-button'),
            visualDensity: isWide ? null : VisualDensity.compact,
            tooltip: 'Reconnect & refresh complete snapshot',
            onPressed: widget.controller.reconnectNow,
            icon: const Icon(Icons.refresh, size: 18),
          ),
        IconButton(
          key: const Key('topbar-theme-toggle'),
          visualDensity: isWide ? null : VisualDensity.compact,
          tooltip: widget.themeMode == ThemeMode.dark
              ? 'Switch to light field theme'
              : 'Switch to dark instrument theme',
          onPressed: widget.onToggleThemeMode,
          icon: Icon(
            widget.themeMode == ThemeMode.dark
                ? Icons.light_mode_outlined
                : Icons.dark_mode_outlined,
            size: 18,
          ),
        ),
        SizedBox(width: isWide ? 8 : 4),
      ],
    );
  }

  Widget _buildSideRail(BuildContext context, ReadModelSnapshot snapshot) {
    final InstrumentPalette palette = context.palette;
    return LayoutBuilder(
      builder: (BuildContext context, BoxConstraints railConstraints) {
        return SingleChildScrollView(
          child: ConstrainedBox(
            constraints: BoxConstraints(minHeight: railConstraints.maxHeight),
            child: IntrinsicHeight(
              child: NavigationRail(
                backgroundColor: palette.surface,
                selectedIndex: _section.index,
                labelType: NavigationRailLabelType.all,
                minWidth: 88,
                onDestinationSelected: (int idx) {
                  setState(() {
                    _section = ClientSection.values[idx];
                  });
                },
                destinations: <NavigationRailDestination>[
                  const NavigationRailDestination(
                    icon: Icon(Icons.space_dashboard_outlined),
                    selectedIcon: Icon(Icons.space_dashboard),
                    label: Text('Overview'),
                  ),
                  NavigationRailDestination(
                    icon: Badge.count(
                      count: snapshot.operationalCars.length,
                      isLabelVisible: snapshot.operationalCars.isNotEmpty,
                      child: const Icon(Icons.elevator_outlined),
                    ),
                    selectedIcon: Badge.count(
                      count: snapshot.operationalCars.length,
                      isLabelVisible: snapshot.operationalCars.isNotEmpty,
                      child: const Icon(Icons.elevator),
                    ),
                    label: const Text('Cars'),
                  ),
                  NavigationRailDestination(
                    icon: Badge.count(
                      count: snapshot.sources.length,
                      isLabelVisible: snapshot.sources.isNotEmpty,
                      child: const Icon(Icons.hub_outlined),
                    ),
                    selectedIcon: Badge.count(
                      count: snapshot.sources.length,
                      isLabelVisible: snapshot.sources.isNotEmpty,
                      child: const Icon(Icons.hub),
                    ),
                    label: const Text('Sources'),
                  ),
                  NavigationRailDestination(
                    icon: Badge.count(
                      count: snapshot.recentDiagnostics.length,
                      isLabelVisible: snapshot.recentDiagnostics.isNotEmpty,
                      child: const Icon(Icons.analytics_outlined),
                    ),
                    selectedIcon: Badge.count(
                      count: snapshot.recentDiagnostics.length,
                      isLabelVisible: snapshot.recentDiagnostics.isNotEmpty,
                      child: const Icon(Icons.analytics),
                    ),
                    label: const Text('Diagnostics'),
                  ),
                ],
              ),
            ),
          ),
        );
      },
    );
  }

  Widget _buildBottomNav(BuildContext context, ReadModelSnapshot snapshot) {
    final InstrumentPalette palette = context.palette;
    return DecoratedBox(
      decoration: BoxDecoration(
        border: Border(top: BorderSide(color: palette.border)),
      ),
      child: NavigationBar(
        backgroundColor: palette.surface,
        selectedIndex: _section.index,
        onDestinationSelected: (int idx) {
          setState(() {
            _section = ClientSection.values[idx];
          });
        },
        destinations: <NavigationDestination>[
          const NavigationDestination(
            icon: Icon(Icons.space_dashboard_outlined),
            selectedIcon: Icon(Icons.space_dashboard),
            label: 'Overview',
          ),
          NavigationDestination(
            icon: Badge.count(
              count: snapshot.operationalCars.length,
              isLabelVisible: snapshot.operationalCars.isNotEmpty,
              child: const Icon(Icons.elevator_outlined),
            ),
            label: 'Cars',
          ),
          NavigationDestination(
            icon: Badge.count(
              count: snapshot.sources.length,
              isLabelVisible: snapshot.sources.isNotEmpty,
              child: const Icon(Icons.hub_outlined),
            ),
            label: 'Sources',
          ),
          NavigationDestination(
            icon: Badge.count(
              count: snapshot.recentDiagnostics.length,
              isLabelVisible: snapshot.recentDiagnostics.isNotEmpty,
              child: const Icon(Icons.analytics_outlined),
            ),
            label: 'Diagnostics',
          ),
        ],
      ),
    );
  }

  Widget _buildBodyContent(
    BuildContext context,
    ClientState state,
    bool isWide,
  ) {
    if (state.snapshot == null) {
      return _buildNoSnapshotWorkspace(context, state);
    }

    final ReadModelSnapshot snapshot = state.snapshot!;
    return SingleChildScrollView(
      padding: EdgeInsets.all(isWide ? 20 : 14),
      child: Center(
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 1440),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: <Widget>[
              SystemStatusStrip(
                state: state,
                onReconnectNow: widget.controller.reconnectNow,
                onConfigureHost: () => _openHostConfig(state),
              ),
              const SizedBox(height: 20),
              switch (_section) {
                ClientSection.overview => _buildOverviewSection(
                  context,
                  snapshot,
                  isWide,
                ),
                ClientSection.cars => _buildCarsSection(
                  context,
                  snapshot,
                  isWide,
                ),
                ClientSection.sources => _buildSourcesSection(
                  context,
                  snapshot,
                ),
                ClientSection.diagnostics => _buildDiagnosticsSection(
                  context,
                  snapshot,
                ),
              },
            ],
          ),
        ),
      ),
    );
  }

  Widget _buildNoSnapshotWorkspace(BuildContext context, ClientState state) {
    final InstrumentPalette palette = context.palette;

    return SingleChildScrollView(
      padding: const EdgeInsets.all(20),
      child: Center(
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 580),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: <Widget>[
              if (state.phase == ConnectionPhase.connecting) ...<Widget>[
                Card(
                  child: Padding(
                    padding: const EdgeInsets.all(22),
                    child: Row(
                      children: <Widget>[
                        const SizedBox(
                          width: 22,
                          height: 22,
                          child: CircularProgressIndicator(strokeWidth: 2.5),
                        ),
                        const SizedBox(width: 16),
                        Expanded(
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: <Widget>[
                              const Text(
                                'Requesting initial complete snapshot...',
                                style: TextStyle(
                                  fontSize: 15,
                                  fontWeight: FontWeight.w700,
                                ),
                              ),
                              const SizedBox(height: 4),
                              Text(
                                state.endpoint?.snapshotUri.toString() ?? '',
                                style: instrumentMonoStyle(
                                  context,
                                  fontSize: 12,
                                  color: palette.subtleText,
                                ),
                              ),
                            ],
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
                const SizedBox(height: 16),
              ] else if (state.error != null) ...<Widget>[
                SystemStatusStrip(
                  state: state,
                  onReconnectNow: widget.controller.reconnectNow,
                  onConfigureHost: () => _openHostConfig(state),
                ),
                const SizedBox(height: 16),
              ],
              Card(
                child: Padding(
                  padding: const EdgeInsets.all(22),
                  child: HostConnectionForm(
                    initialUrl: state.endpoint?.origin ?? state.rawUrlInput,
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
    );
  }

  Widget _buildOverviewSection(
    BuildContext context,
    ReadModelSnapshot snapshot,
    bool isWide,
  ) {
    if (isWide) {
      return Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: <Widget>[
          Expanded(
            flex: 6,
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: <Widget>[
                SectionHeader(
                  title:
                      'Operational Cars (${snapshot.operationalCars.length})',
                  subtitle:
                      'Engine-owned complete operational state from Operational observation authorities',
                ),
                const SizedBox(height: 12),
                _buildCarsGridOrEmpty(context, snapshot),
                const SizedBox(height: 24),
                SectionHeader(
                  title: 'Observation Sources (${snapshot.sources.length})',
                  subtitle:
                      'Explicit session, transport discovery, payload activity, and Operational vs Auxiliary roles',
                ),
                const SizedBox(height: 12),
                _buildSourcesListOrEmpty(context, snapshot),
              ],
            ),
          ),
          const SizedBox(width: 20),
          Expanded(
            flex: 5,
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: <Widget>[
                SectionHeader(
                  title:
                      'Recent Host Diagnostics (${snapshot.recentDiagnostics.length})',
                  subtitle:
                      'Bounded machine-readable evidence across Host layers',
                ),
                const SizedBox(height: 12),
                DiagnosticEvidencePanel(
                  diagnostics: snapshot.recentDiagnostics,
                  maxPreviewEntries: 8,
                  onViewAll: () {
                    setState(() {
                      _section = ClientSection.diagnostics;
                    });
                  },
                ),
              ],
            ),
          ),
        ],
      );
    }

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: <Widget>[
        SectionHeader(
          title: 'Operational Cars (${snapshot.operationalCars.length})',
          subtitle:
              'Engine-owned complete operational state from Operational authorities',
        ),
        const SizedBox(height: 12),
        _buildCarsGridOrEmpty(context, snapshot),
        const SizedBox(height: 22),
        SectionHeader(
          title: 'Observation Sources (${snapshot.sources.length})',
          subtitle:
              'Session, transport, payload activity, and binding authority roles',
        ),
        const SizedBox(height: 12),
        _buildSourcesListOrEmpty(context, snapshot),
        const SizedBox(height: 22),
        SectionHeader(
          title:
              'Recent Host Diagnostics (${snapshot.recentDiagnostics.length})',
          subtitle: 'Bounded machine-readable Host evidence',
        ),
        const SizedBox(height: 12),
        DiagnosticEvidencePanel(
          diagnostics: snapshot.recentDiagnostics,
          maxPreviewEntries: 6,
          onViewAll: () {
            setState(() {
              _section = ClientSection.diagnostics;
            });
          },
        ),
      ],
    );
  }

  Widget _buildCarsSection(
    BuildContext context,
    ReadModelSnapshot snapshot,
    bool isWide,
  ) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: <Widget>[
        SectionHeader(
          title: 'Operational Cars (${snapshot.operationalCars.length})',
          subtitle:
              'Unique Engine-owned operational view keyed by logical CarId. '
              'Floor association and measured continuous position remain distinct.',
        ),
        const SizedBox(height: 14),
        _buildCarsGridOrEmpty(context, snapshot),
      ],
    );
  }

  Widget _buildSourcesSection(
    BuildContext context,
    ReadModelSnapshot snapshot,
  ) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: <Widget>[
        SectionHeader(
          title: 'Observation Sources (${snapshot.sources.length})',
          subtitle:
              'Transport discovery, payload activity, source session, and per-car observation availability are independent health layers.',
        ),
        const SizedBox(height: 14),
        _buildSourcesListOrEmpty(context, snapshot),
      ],
    );
  }

  Widget _buildDiagnosticsSection(
    BuildContext context,
    ReadModelSnapshot snapshot,
  ) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: <Widget>[
        SectionHeader(
          title:
              'Recent Host Diagnostics (${snapshot.recentDiagnostics.length})',
          subtitle:
              'Inspectable Host evidence stream partitioned by layer, source, session, logical car, binding role, and native entity.',
        ),
        const SizedBox(height: 14),
        DiagnosticEvidencePanel(diagnostics: snapshot.recentDiagnostics),
      ],
    );
  }

  Widget _buildCarsGridOrEmpty(
    BuildContext context,
    ReadModelSnapshot snapshot,
  ) {
    final InstrumentPalette palette = context.palette;
    if (snapshot.operationalCars.isEmpty) {
      return Card(
        key: const Key('empty-operational-cars-card'),
        child: Padding(
          padding: const EdgeInsets.all(18),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: <Widget>[
              Row(
                children: <Widget>[
                  Icon(
                    Icons.hourglass_empty_rounded,
                    size: 18,
                    color: palette.caution,
                  ),
                  const SizedBox(width: 8),
                  const Expanded(
                    child: Text(
                      'No Complete Operational Cars Currently Resolved',
                      style: TextStyle(
                        fontSize: 14,
                        fontWeight: FontWeight.w700,
                      ),
                    ),
                  ),
                ],
              ),
              const SizedBox(height: 6),
              Text(
                'LiftAssistant promotes a logical car into operational_cars only when its configured '
                'Operational authority has complete, resolved position, motion, directed door, and operating-mode evidence. '
                'Auxiliary sources never enter operational truth or fail over automatically. '
                'Inspect Observation Sources for awaiting, incomplete, unresolved, or revoked bindings.',
                style: TextStyle(
                  fontSize: 12.5,
                  height: 1.4,
                  color: palette.subtleText,
                ),
              ),
            ],
          ),
        ),
      );
    }

    return LayoutBuilder(
      builder: (BuildContext context, BoxConstraints constraints) {
        final int columns = constraints.maxWidth >= 720 ? 2 : 1;
        const double spacing = 12;
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
      return Card(
        child: Padding(
          padding: const EdgeInsets.all(18),
          child: Text(
            'No observation sources are configured on this Host.',
            style: TextStyle(fontSize: 13, color: palette.subtleText),
          ),
        ),
      );
    }

    return Column(
      children: <Widget>[
        for (final SourceHealth source in snapshot.sources)
          Padding(
            padding: const EdgeInsets.only(bottom: 12),
            child: SourceHealthCard(source: source),
          ),
      ],
    );
  }
}
