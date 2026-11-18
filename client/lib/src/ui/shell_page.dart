import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';

import '../api/host_url.dart';
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

enum ClientSection { overview, cars, sources, diagnostics }

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
  final DiagnosticsFilterController _diagnosticsFilter =
      DiagnosticsFilterController();
  HostEndpoint? _lastEndpoint;

  @override
  void initState() {
    super.initState();
    if (kIsWeb) {
      switch (Uri.base.queryParameters['section']) {
        case 'cars':
          _section = ClientSection.cars;
        case 'sources':
          _section = ClientSection.sources;
        case 'diagnostics':
          _section = ClientSection.diagnostics;
        case 'overview':
        default:
          break;
      }
    }
    _lastEndpoint = widget.controller.state.endpoint;
    widget.controller.addListener(_handleControllerChanged);
  }

  @override
  void didUpdateWidget(covariant ClientShellPage oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.controller != widget.controller) {
      oldWidget.controller.removeListener(_handleControllerChanged);
      widget.controller.addListener(_handleControllerChanged);
      _handleControllerChanged();
    }
  }

  @override
  void dispose() {
    widget.controller.removeListener(_handleControllerChanged);
    _diagnosticsFilter.dispose();
    super.dispose();
  }

  void _handleControllerChanged() {
    final HostEndpoint? currentEndpoint = widget.controller.state.endpoint;
    if (_lastEndpoint != null &&
        currentEndpoint != null &&
        _lastEndpoint != currentEndpoint) {
      _diagnosticsFilter.resetAll();
    }
    if (currentEndpoint != null) {
      _lastEndpoint = currentEndpoint;
    }
  }

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

  void _switchSection(ClientSection next) {
    if (_section == next) {
      return;
    }
    setState(() {
      _section = next;
    });
  }

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: widget.controller,
      builder: (BuildContext context, _) {
        final ClientState state = widget.controller.state;
        final Size viewport = MediaQuery.sizeOf(context);
        final double textScale = MediaQuery.textScalerOf(context).scale(1.0);
        final bool isWide = viewport.width >= 760;

        return Scaffold(
          appBar: _buildTopBar(
            context,
            state,
            screenWidth: viewport.width,
            textScale: textScale,
            isWide: isWide,
          ),
          bottomNavigationBar: isWide || state.snapshot == null
              ? null
              : _buildBottomNav(context, state.snapshot!),
          body: SafeArea(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: <Widget>[
                if (!isWide)
                  _buildCompactEndpointBar(
                    context,
                    state,
                    textScale: textScale,
                  ),
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
                        child: state.snapshot == null
                            ? _buildNoSnapshotWorkspace(context, state)
                            : _buildConnectedWorkspace(
                                context,
                                state,
                                state.snapshot!,
                                viewport: viewport,
                                textScale: textScale,
                                isWide: isWide,
                              ),
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

  /// Layer 1 — Clean, single-purpose AppBar:
  /// Left: Brand + LiftAssistant + subtle Read-Only chip.
  /// Right: Only Client-to-Host connection controls (Host address button,
  /// connection phase badge, Reconnect, Semantics `?`, Theme toggle).
  PreferredSizeWidget _buildTopBar(
    BuildContext context,
    ClientState state, {
    required double screenWidth,
    required double textScale,
    required bool isWide,
  }) {
    final InstrumentPalette palette = context.palette;
    final double barHeight = (54.0 * textScale.clamp(1.0, 1.35))
        .roundToDouble();
    final bool isCompactTopBar = screenWidth < 420 || textScale > 1.35;
    final bool showReadOnlyChip = screenWidth >= 620 && textScale <= 1.3;
    final (
      String phaseLabel,
      Color phaseColor,
      Color phaseBg,
      bool phaseHollow,
    ) = describeConnectionPhase(
      state.phase,
      palette,
    );

    return PreferredSize(
      preferredSize: Size.fromHeight(barHeight),
      child: DecoratedBox(
        decoration: BoxDecoration(
          color: palette.surface,
          border: Border(bottom: BorderSide(color: palette.border)),
        ),
        child: SafeArea(
          bottom: false,
          child: ConstrainedBox(
            constraints: BoxConstraints(minHeight: barHeight),
            child: Padding(
              padding: EdgeInsets.symmetric(
                horizontal: isCompactTopBar ? 8 : 16,
                vertical: 6,
              ),
              child: Row(
                children: <Widget>[
                  Expanded(
                    child: Row(
                      children: <Widget>[
                        const _HoistwayBrandMark(),
                        if (!isCompactTopBar || screenWidth >= 360) ...<Widget>[
                          const SizedBox(width: 8),
                          const Flexible(
                            child: Text(
                              'LiftAssistant',
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: TextStyle(
                                fontSize: 15.5,
                                fontWeight: FontWeight.w700,
                                letterSpacing: -0.25,
                              ),
                            ),
                          ),
                        ],
                        if (showReadOnlyChip) ...<Widget>[
                          const SizedBox(width: 10),
                          SemanticBadge(
                            label: 'Read-Only',
                            foreground: palette.subtleText,
                            background: palette.elevatedSurface,
                            icon: Icons.lock_outline_rounded,
                            outlined: true,
                            tooltip: 'Strictly read-only experimental/v0 client; zero elevator write controls.',
                          ),
                        ],
                      ],
                    ),
                  ),
                  SizedBox(width: isCompactTopBar ? 4 : 8),
                  if (isWide) ...<Widget>[
                    ConstrainedBox(
                      constraints: BoxConstraints(
                        maxWidth: textScale > 1.35 ? 190 : 240,
                      ),
                      child: Material(
                        color: palette.elevatedSurface,
                        borderRadius: BorderRadius.circular(10),
                        child: InkWell(
                          key: const Key('topbar-host-button'),
                          borderRadius: BorderRadius.circular(10),
                          onTap: () => _openHostConfig(state),
                          child: Container(
                            constraints: const BoxConstraints(minHeight: 34),
                            padding: const EdgeInsets.symmetric(
                              horizontal: 10,
                              vertical: 5,
                            ),
                            decoration: BoxDecoration(
                              borderRadius: BorderRadius.circular(10),
                              border: Border.all(color: palette.border),
                            ),
                            child: Row(
                              mainAxisSize: MainAxisSize.min,
                              children: <Widget>[
                                Icon(
                                  Icons.dns_outlined,
                                  size: 14.5,
                                  color: palette.subtleText,
                                ),
                                const SizedBox(width: 6),
                                Flexible(
                                  child: Text(
                                    state.endpoint?.origin ?? 'Configure Host',
                                    maxLines: 1,
                                    overflow: TextOverflow.ellipsis,
                                    style: instrumentTabularStyle(
                                      context,
                                      fontSize: 12,
                                      fontWeight: FontWeight.w600,
                                      color: Theme.of(context)
                                          .colorScheme
                                          .onSurface,
                                    ),
                                  ),
                                ),
                                const SizedBox(width: 5),
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
                    ),
                    const SizedBox(width: 8),
                  ] else
                    _TopBarIconButton(
                      key: const Key('topbar-host-button'),
                      tooltip: 'Configure Host endpoint',
                      onPressed: () => _openHostConfig(state),
                      icon: Icons.dns_outlined,
                      compact: isCompactTopBar,
                    ),
                  ConstrainedBox(
                    constraints: BoxConstraints(
                      maxWidth: isWide ? 180 : (isCompactTopBar ? 96 : 136),
                    ),
                    child: SemanticBadge(
                      key: const Key('topbar-connection-phase-badge'),
                      label: phaseLabel,
                      foreground: phaseColor,
                      background: phaseBg,
                      dotColor: phaseColor,
                      hollowDot: phaseHollow,
                    ),
                  ),
                  SizedBox(width: isCompactTopBar ? 2 : 4),
                  if (state.endpoint != null)
                    _TopBarIconButton(
                      key: const Key('topbar-reconnect-button'),
                      tooltip: 'Reconnect & refresh snapshot',
                      onPressed: widget.controller.reconnectNow,
                      icon: Icons.refresh_rounded,
                      compact: isCompactTopBar,
                    ),
                  _TopBarIconButton(
                    key: const Key('topbar-semantics-button'),
                    tooltip: 'experimental/v0 semantic reference',
                    onPressed: () => showSemanticsReferenceModal(context),
                    icon: Icons.help_outline_rounded,
                    compact: isCompactTopBar,
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
                    compact: isCompactTopBar,
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }

  Widget _buildCompactEndpointBar(
    BuildContext context,
    ClientState state, {
    required double textScale,
  }) {
    final InstrumentPalette palette = context.palette;
    return DecoratedBox(
      decoration: BoxDecoration(
        color: palette.elevatedSurface,
        border: Border(bottom: BorderSide(color: palette.border)),
      ),
      child: InkWell(
        key: const Key('compact-endpoint-bar'),
        onTap: () => _openHostConfig(state),
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 7),
          child: Row(
            children: <Widget>[
              if (textScale <= 1.5) ...<Widget>[
                SemanticBadge(
                  label: 'Read-Only',
                  foreground: palette.subtleText,
                  background: palette.surface,
                  icon: Icons.lock_outline_rounded,
                  outlined: true,
                ),
                const SizedBox(width: 8),
              ],
              Icon(Icons.dns_outlined, size: 13.5, color: palette.subtleText),
              const SizedBox(width: 6),
              Expanded(
                child: Text(
                  state.endpoint?.origin ?? 'Tap to configure Host URL',
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: instrumentTabularStyle(
                    context,
                    fontSize: 12,
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

  Widget _buildNavBadge(
    BuildContext context, {
    required int count,
    required bool selected,
    required Widget child,
  }) {
    final InstrumentPalette palette = context.palette;
    return Badge.count(
      count: count,
      isLabelVisible: count > 0,
      backgroundColor: selected
          ? palette.infoContainer
          : palette.elevatedSurface,
      textColor: selected ? palette.info : palette.subtleText,
      textStyle: instrumentTabularStyle(
        context,
        fontSize: 10.5,
        fontWeight: FontWeight.w700,
        color: selected ? palette.info : palette.subtleText,
      ),
      child: child,
    );
  }

  /// Layer 2 (Desktop `>= 760px`) — Left NavigationRail with M3 `Badge.count`
  /// on Cars, Sources, and Diagnostics. Wrapped in SingleChildScrollView +
  /// IntrinsicHeight so short landscape viewports (`800x300`) never overflow.
  Widget _buildSideRail(BuildContext context, ReadModelSnapshot snapshot) {
    final InstrumentPalette palette = context.palette;
    return MediaQuery(
      data: MediaQuery.of(context).copyWith(
        textScaler: MediaQuery.textScalerOf(context).clamp(maxScaleFactor: 1.2),
      ),
      child: LayoutBuilder(
        builder: (BuildContext context, BoxConstraints railConstraints) {
          return SingleChildScrollView(
            child: ConstrainedBox(
              constraints: BoxConstraints(minHeight: railConstraints.maxHeight),
              child: IntrinsicHeight(
                child: NavigationRail(
                  key: const Key('desktop-navigation-rail'),
                  backgroundColor: palette.surface,
                  selectedIndex: _section.index,
                  labelType: NavigationRailLabelType.all,
                  minWidth: 88,
                  onDestinationSelected: (int idx) {
                    _switchSection(ClientSection.values[idx]);
                  },
                  destinations: <NavigationRailDestination>[
                    const NavigationRailDestination(
                      icon: Icon(Icons.space_dashboard_outlined),
                      selectedIcon: Icon(Icons.space_dashboard),
                      label: Text('Overview'),
                    ),
                    NavigationRailDestination(
                      icon: _buildNavBadge(
                        context,
                        count: snapshot.operationalCars.length,
                        selected: false,
                        child: const Icon(Icons.elevator_outlined),
                      ),
                      selectedIcon: _buildNavBadge(
                        context,
                        count: snapshot.operationalCars.length,
                        selected: true,
                        child: const Icon(Icons.elevator),
                      ),
                      label: const Text('Cars'),
                    ),
                    NavigationRailDestination(
                      icon: _buildNavBadge(
                        context,
                        count: snapshot.sources.length,
                        selected: false,
                        child: const Icon(Icons.hub_outlined),
                      ),
                      selectedIcon: _buildNavBadge(
                        context,
                        count: snapshot.sources.length,
                        selected: true,
                        child: const Icon(Icons.hub),
                      ),
                      label: const Text('Sources'),
                    ),
                    NavigationRailDestination(
                      icon: _buildNavBadge(
                        context,
                        count: snapshot.recentDiagnostics.length,
                        selected: false,
                        child: const Icon(Icons.analytics_outlined),
                      ),
                      selectedIcon: _buildNavBadge(
                        context,
                        count: snapshot.recentDiagnostics.length,
                        selected: true,
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
      ),
    );
  }

  /// Layer 2 (Mobile `< 760px`) — Bottom NavigationBar with M3 `Badge.count`.
  Widget _buildBottomNav(BuildContext context, ReadModelSnapshot snapshot) {
    final InstrumentPalette palette = context.palette;
    return MediaQuery(
      data: MediaQuery.of(context).copyWith(
        textScaler: MediaQuery.textScalerOf(context).clamp(maxScaleFactor: 1.2),
      ),
      child: DecoratedBox(
        decoration: BoxDecoration(
          border: Border(top: BorderSide(color: palette.border)),
        ),
        child: NavigationBar(
          key: const Key('mobile-navigation-bar'),
          backgroundColor: palette.surface,
          selectedIndex: _section.index,
          onDestinationSelected: (int idx) {
            _switchSection(ClientSection.values[idx]);
          },
          destinations: <NavigationDestination>[
            const NavigationDestination(
              icon: Icon(Icons.space_dashboard_outlined),
              selectedIcon: Icon(Icons.space_dashboard),
              label: 'Overview',
            ),
            NavigationDestination(
              icon: _buildNavBadge(
                context,
                count: snapshot.operationalCars.length,
                selected: false,
                child: const Icon(Icons.elevator_outlined),
              ),
              selectedIcon: _buildNavBadge(
                context,
                count: snapshot.operationalCars.length,
                selected: true,
                child: const Icon(Icons.elevator),
              ),
              label: 'Cars',
            ),
            NavigationDestination(
              icon: _buildNavBadge(
                context,
                count: snapshot.sources.length,
                selected: false,
                child: const Icon(Icons.hub_outlined),
              ),
              selectedIcon: _buildNavBadge(
                context,
                count: snapshot.sources.length,
                selected: true,
                child: const Icon(Icons.hub),
              ),
              label: 'Sources',
            ),
            NavigationDestination(
              icon: _buildNavBadge(
                context,
                count: snapshot.recentDiagnostics.length,
                selected: false,
                child: const Icon(Icons.analytics_outlined),
              ),
              selectedIcon: _buildNavBadge(
                context,
                count: snapshot.recentDiagnostics.length,
                selected: true,
                child: const Icon(Icons.analytics),
              ),
              label: 'Diagnostics',
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildNoSnapshotWorkspace(BuildContext context, ClientState state) {
    return LayoutBuilder(
      builder: (BuildContext context, BoxConstraints viewportConstraints) {
        final double minCenteredHeight = (viewportConstraints.maxHeight - 32)
            .clamp(0.0, double.infinity);
        return SingleChildScrollView(
          padding: const EdgeInsets.all(16),
          child: ConstrainedBox(
            constraints: BoxConstraints(minHeight: minCenteredHeight),
            child: Align(
              alignment: const Alignment(0.0, -0.36),
              child: ConstrainedBox(
                constraints: const BoxConstraints(maxWidth: 480),
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: <Widget>[
                    if (state.phase == ConnectionPhase.connecting) ...<Widget>[
                      Card(
                        child: Padding(
                          padding: const EdgeInsets.symmetric(
                            horizontal: 14,
                            vertical: 12,
                          ),
                          child: Row(
                            children: <Widget>[
                              const SizedBox(
                                width: 16,
                                height: 16,
                                child: CircularProgressIndicator(
                                  strokeWidth: 2,
                                ),
                              ),
                              const SizedBox(width: 12),
                              Expanded(
                                child: Text(
                                  'Requesting snapshot from ${state.endpoint?.origin ?? "Host"}...',
                                  maxLines: 2,
                                  overflow: TextOverflow.ellipsis,
                                  style: const TextStyle(
                                    fontSize: 13,
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
                      SystemStatusStrip(
                        state: state,
                        onReconnectNow: widget.controller.reconnectNow,
                        onConfigureHost: () => _openHostConfig(state),
                      ),
                      const SizedBox(height: 12),
                    ],
                    Card(
                      child: Padding(
                        padding: const EdgeInsets.all(20),
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
    required Size viewport,
    required double textScale,
    required bool isWide,
  }) {
    final EdgeInsets pagePadding = EdgeInsets.all(isWide ? 20 : 14);
    final Widget statusStrip = SystemStatusStrip(
      state: state,
      onReconnectNow: widget.controller.reconnectNow,
      onConfigureHost: () => _openHostConfig(state),
    );

    final Widget content = switch (_section) {
      ClientSection.overview => _buildOverviewWorkspace(
        context,
        snapshot,
        statusStrip: statusStrip,
        pagePadding: pagePadding,
        isWide: isWide,
        textScale: textScale,
      ),
      ClientSection.cars => _buildCarsWorkspace(
        context,
        snapshot,
        statusStrip: statusStrip,
        pagePadding: pagePadding,
        textScale: textScale,
      ),
      ClientSection.sources => _buildSourcesWorkspace(
        context,
        snapshot,
        statusStrip: statusStrip,
        pagePadding: pagePadding,
        textScale: textScale,
      ),
      ClientSection.diagnostics => _buildDiagnosticsWorkspace(
        context,
        snapshot,
        statusStrip: statusStrip,
        pagePadding: pagePadding,
      ),
    };

    return AnimatedOpacity(
      duration: const Duration(milliseconds: 150),
      opacity: state.isSnapshotLive ? 1.0 : 0.72,
      child: content,
    );
  }

  Widget _buildOverviewDiagnosticsHeader(
    BuildContext context,
    ReadModelSnapshot snapshot,
  ) {
    return SectionHeader(
      title: 'Recent Diagnostics',
      count: snapshot.recentDiagnostics.length,
      icon: Icons.analytics_outlined,
    );
  }

  Widget _buildOverviewWorkspace(
    BuildContext context,
    ReadModelSnapshot snapshot, {
    required Widget statusStrip,
    required EdgeInsets pagePadding,
    required bool isWide,
    required double textScale,
  }) {
    // On desktop viewports, render the classic 2-column Overview (left flex: 6
    // Operational Cars + Observation Sources, right flex: 5 Recent Diagnostics preview).
    if (isWide && textScale <= 1.45) {
      return SingleChildScrollView(
        padding: pagePadding,
        child: Center(
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 1440),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: <Widget>[
                statusStrip,
                const SizedBox(height: 20),
                Row(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: <Widget>[
                    Expanded(
                      flex: 6,
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.stretch,
                        children: <Widget>[
                          SectionHeader(
                            title: 'Operational Cars',
                            count: snapshot.operationalCars.length,
                            icon: Icons.elevator_outlined,
                          ),
                          const SizedBox(height: 12),
                          _buildCarsGridBox(
                            context,
                            snapshot,
                            textScale: textScale,
                          ),
                          const SizedBox(height: 24),
                          SectionHeader(
                            title: 'Observation Sources',
                            count: snapshot.sources.length,
                            icon: Icons.hub_outlined,
                          ),
                          const SizedBox(height: 12),
                          _buildSourcesListBox(context, snapshot),
                        ],
                      ),
                    ),
                    const SizedBox(width: 20),
                    Expanded(
                      flex: 5,
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.stretch,
                        children: <Widget>[
                          _buildOverviewDiagnosticsHeader(context, snapshot),
                          const SizedBox(height: 12),
                          DiagnosticEvidencePanel(
                            diagnostics: snapshot.recentDiagnostics,
                            maxPreviewEntries: 6,
                            onViewAll: () =>
                                _switchSection(ClientSection.diagnostics),
                          ),
                        ],
                      ),
                    ),
                  ],
                ),
              ],
            ),
          ),
        ),
      );
    }

    // On mobile / narrow viewports (or large accessibility text scales), stack
    // sections vertically in a virtualized CustomScrollView.
    return Center(
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 1440),
        child: CustomScrollView(
          slivers: <Widget>[
            SliverPadding(
              padding: EdgeInsets.fromLTRB(
                pagePadding.left,
                pagePadding.top,
                pagePadding.right,
                12,
              ),
              sliver: SliverToBoxAdapter(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: <Widget>[
                    statusStrip,
                    const SizedBox(height: 18),
                    SectionHeader(
                      title: 'Operational Cars',
                      count: snapshot.operationalCars.length,
                      icon: Icons.elevator_outlined,
                    ),
                  ],
                ),
              ),
            ),
            SliverPadding(
              padding: EdgeInsets.symmetric(horizontal: pagePadding.left),
              sliver: _buildCarsSliver(context, snapshot, textScale: textScale),
            ),
            SliverPadding(
              padding: EdgeInsets.fromLTRB(
                pagePadding.left,
                22,
                pagePadding.right,
                12,
              ),
              sliver: SliverToBoxAdapter(
                child: SectionHeader(
                  title: 'Observation Sources',
                  count: snapshot.sources.length,
                  icon: Icons.hub_outlined,
                ),
              ),
            ),
            SliverPadding(
              padding: EdgeInsets.symmetric(horizontal: pagePadding.left),
              sliver: _buildSourcesSliver(
                context,
                snapshot,
                textScale: textScale,
                allowMultiColumn: false,
              ),
            ),
            SliverPadding(
              padding: EdgeInsets.fromLTRB(
                pagePadding.left,
                22,
                pagePadding.right,
                pagePadding.bottom,
              ),
              sliver: SliverToBoxAdapter(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: <Widget>[
                    _buildOverviewDiagnosticsHeader(context, snapshot),
                    const SizedBox(height: 12),
                    DiagnosticEvidencePanel(
                      diagnostics: snapshot.recentDiagnostics,
                      maxPreviewEntries: 6,
                      onViewAll: () =>
                          _switchSection(ClientSection.diagnostics),
                    ),
                  ],
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildCarsWorkspace(
    BuildContext context,
    ReadModelSnapshot snapshot, {
    required Widget statusStrip,
    required EdgeInsets pagePadding,
    required double textScale,
  }) {
    return Center(
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 1440),
        child: CustomScrollView(
          key: const Key('cars-section-scroll'),
          slivers: <Widget>[
            SliverPadding(
              padding: EdgeInsets.fromLTRB(
                pagePadding.left,
                pagePadding.top,
                pagePadding.right,
                14,
              ),
              sliver: SliverToBoxAdapter(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: <Widget>[
                    statusStrip,
                    const SizedBox(height: 20),
                    SectionHeader(
                      title: 'Operational Cars',
                      count: snapshot.operationalCars.length,
                      icon: Icons.elevator_outlined,
                    ),
                  ],
                ),
              ),
            ),
            SliverPadding(
              padding: EdgeInsets.fromLTRB(
                pagePadding.left,
                0,
                pagePadding.right,
                pagePadding.bottom,
              ),
              sliver: _buildCarsSliver(context, snapshot, textScale: textScale),
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildSourcesWorkspace(
    BuildContext context,
    ReadModelSnapshot snapshot, {
    required Widget statusStrip,
    required EdgeInsets pagePadding,
    required double textScale,
  }) {
    return Center(
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 1440),
        child: CustomScrollView(
          key: const Key('sources-section-scroll'),
          slivers: <Widget>[
            SliverPadding(
              padding: EdgeInsets.fromLTRB(
                pagePadding.left,
                pagePadding.top,
                pagePadding.right,
                14,
              ),
              sliver: SliverToBoxAdapter(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: <Widget>[
                    statusStrip,
                    const SizedBox(height: 20),
                    SectionHeader(
                      title: 'Observation Sources',
                      count: snapshot.sources.length,
                      icon: Icons.hub_outlined,
                    ),
                  ],
                ),
              ),
            ),
            SliverPadding(
              padding: EdgeInsets.fromLTRB(
                pagePadding.left,
                0,
                pagePadding.right,
                pagePadding.bottom,
              ),
              sliver: _buildSourcesSliver(
                context,
                snapshot,
                textScale: textScale,
                allowMultiColumn: true,
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildDiagnosticsWorkspace(
    BuildContext context,
    ReadModelSnapshot snapshot, {
    required Widget statusStrip,
    required EdgeInsets pagePadding,
  }) {
    return SingleChildScrollView(
      key: const Key('diagnostics-section-scroll'),
      padding: pagePadding,
      child: Center(
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 1440),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: <Widget>[
              statusStrip,
              const SizedBox(height: 20),
              SectionHeader(
                title: 'Host Diagnostics',
                count: snapshot.recentDiagnostics.length,
                icon: Icons.analytics_outlined,
              ),
              const SizedBox(height: 14),
              DiagnosticEvidencePanel(
                diagnostics: snapshot.recentDiagnostics,
                filterController: _diagnosticsFilter,
              ),
            ],
          ),
        ),
      ),
    );
  }

  Widget _buildCarsGridBox(
    BuildContext context,
    ReadModelSnapshot snapshot, {
    required double textScale,
  }) {
    const double spacing = 14;
    final List<OperationalCar> cars = snapshot.operationalCars;
    if (cars.isEmpty) {
      return _EmptyOperationalCarsPanel(sources: snapshot.sources);
    }

    return LayoutBuilder(
      builder: (BuildContext context, BoxConstraints constraints) {
        final double width = constraints.maxWidth;
        if (cars.length == 1) {
          final bool useTabularFooter = width >= 280 && textScale <= 1.35;
          return OperationalCarCard(
            car: cars.first,
            useTabularFooter: useTabularFooter,
          );
        }

        final int columns = (width >= 520 && textScale <= 1.45) ? 2 : 1;
        final double cardWidth = columns == 1
            ? width
            : (width - spacing) / columns;
        final bool useTabularFooter = cardWidth >= 280 && textScale <= 1.35;
        final int rowCount = (cars.length + columns - 1) ~/ columns;

        Widget buildRow(int rowIndex) {
          if (columns == 1) {
            return OperationalCarCard(
              car: cars[rowIndex],
              useTabularFooter: useTabularFooter,
            );
          }
          final int start = rowIndex * columns;
          return IntrinsicHeight(
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: <Widget>[
                for (int col = 0; col < columns; col++) ...<Widget>[
                  if (col > 0) const SizedBox(width: spacing),
                  Expanded(
                    child: (start + col < cars.length)
                        ? OperationalCarCard(
                            car: cars[start + col],
                            stretchToRowHeight: true,
                            useTabularFooter: useTabularFooter,
                          )
                        : const SizedBox.shrink(),
                  ),
                ],
              ],
            ),
          );
        }

        // Lazily virtualize when fleet size is large (> 12 cars).
        if (cars.length > 12) {
          return ConstrainedBox(
            constraints: const BoxConstraints(maxHeight: 540),
            child: ListView.builder(
              padding: EdgeInsets.zero,
              itemCount: rowCount,
              itemBuilder: (BuildContext context, int rowIndex) {
                final Widget rowWidget = buildRow(rowIndex);
                if (rowIndex == 0) {
                  return rowWidget;
                }
                return Padding(
                  padding: const EdgeInsets.only(top: spacing),
                  child: rowWidget,
                );
              },
            ),
          );
        }

        return Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: <Widget>[
            for (int rowIndex = 0; rowIndex < rowCount; rowIndex++) ...<Widget>[
              if (rowIndex > 0) const SizedBox(height: spacing),
              buildRow(rowIndex),
            ],
          ],
        );
      },
    );
  }

  Widget _buildSourcesListBox(
    BuildContext context,
    ReadModelSnapshot snapshot,
  ) {
    const double spacing = 14;
    final InstrumentPalette palette = context.palette;
    final List<SourceHealth> sources = snapshot.sources;
    if (sources.isEmpty) {
      return Card(
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 16),
          child: Text(
            'No observation sources configured on this Host.',
            style: TextStyle(fontSize: 13, color: palette.mutedText),
          ),
        ),
      );
    }

    if (sources.length > 8) {
      return ConstrainedBox(
        constraints: const BoxConstraints(maxHeight: 540),
        child: ListView.builder(
          padding: EdgeInsets.zero,
          itemCount: sources.length,
          itemBuilder: (BuildContext context, int i) {
            final Widget card = SourceHealthCard(source: sources[i]);
            if (i == 0) {
              return card;
            }
            return Padding(
              padding: const EdgeInsets.only(top: spacing),
              child: card,
            );
          },
        ),
      );
    }

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: <Widget>[
        for (int i = 0; i < sources.length; i++) ...<Widget>[
          if (i > 0) const SizedBox(height: spacing),
          SourceHealthCard(source: sources[i]),
        ],
      ],
    );
  }

  Widget _buildCarsSliver(
    BuildContext context,
    ReadModelSnapshot snapshot, {
    required double textScale,
  }) {
    const double spacing = 14;
    final List<OperationalCar> cars = snapshot.operationalCars;
    if (cars.isEmpty) {
      return SliverToBoxAdapter(
        child: _EmptyOperationalCarsPanel(sources: snapshot.sources),
      );
    }

    return SliverLayoutBuilder(
      builder: (BuildContext context, SliverConstraints sliverConstraints) {
        final double width = sliverConstraints.crossAxisExtent;
        final bool useTwoColumns =
            textScale <= 1.45 &&
            (width >= 680 || (cars.length > 1 && width >= 520));

        if (!useTwoColumns) {
          final bool useTabularFooter = width >= 280 && textScale <= 1.35;
          if (cars.length == 1) {
            return SliverToBoxAdapter(
              child: OperationalCarCard(
                car: cars.first,
                useTabularFooter: useTabularFooter,
              ),
            );
          }
          return SliverList.builder(
            itemCount: cars.length,
            itemBuilder: (BuildContext context, int index) {
              final Widget card = OperationalCarCard(
                car: cars[index],
                useTabularFooter: useTabularFooter,
              );
              if (index == 0) {
                return card;
              }
              return Padding(
                padding: const EdgeInsets.only(top: spacing),
                child: card,
              );
            },
          );
        }

        const int columns = 2;
        final double cardWidth = (width - spacing) / columns;
        final bool useTabularFooter = cardWidth >= 280 && textScale <= 1.35;
        final int rowCount = (cars.length + columns - 1) ~/ columns;

        return SliverList.builder(
          itemCount: rowCount,
          itemBuilder: (BuildContext context, int rowIndex) {
            final int start = rowIndex * columns;
            final Widget rowWidget = IntrinsicHeight(
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: <Widget>[
                  for (int col = 0; col < columns; col++) ...<Widget>[
                    if (col > 0) const SizedBox(width: spacing),
                    Expanded(
                      child: (start + col < cars.length)
                          ? OperationalCarCard(
                              car: cars[start + col],
                              stretchToRowHeight: true,
                              useTabularFooter: useTabularFooter,
                            )
                          : const SizedBox.shrink(),
                    ),
                  ],
                ],
              ),
            );

            if (rowIndex == 0) {
              return rowWidget;
            }
            return Padding(
              padding: const EdgeInsets.only(top: spacing),
              child: rowWidget,
            );
          },
        );
      },
    );
  }

  Widget _buildSourcesSliver(
    BuildContext context,
    ReadModelSnapshot snapshot, {
    required double textScale,
    required bool allowMultiColumn,
  }) {
    const double spacing = 14;
    final InstrumentPalette palette = context.palette;
    final List<SourceHealth> sources = snapshot.sources;
    if (sources.isEmpty) {
      return SliverToBoxAdapter(
        child: Card(
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 16),
            child: Text(
              'No observation sources configured on this Host.',
              style: TextStyle(fontSize: 13, color: palette.mutedText),
            ),
          ),
        ),
      );
    }

    return SliverLayoutBuilder(
      builder: (BuildContext context, SliverConstraints sliverConstraints) {
        final double width = sliverConstraints.crossAxisExtent;
        final bool useTwoColumns =
            allowMultiColumn && width >= 680 && textScale <= 1.35;

        if (!useTwoColumns) {
          return SliverList.builder(
            itemCount: sources.length,
            itemBuilder: (BuildContext context, int index) {
              final Widget card = SourceHealthCard(source: sources[index]);
              if (index == 0) {
                return card;
              }
              return Padding(
                padding: const EdgeInsets.only(top: spacing),
                child: card,
              );
            },
          );
        }

        const int columns = 2;
        final int rowCount = (sources.length + columns - 1) ~/ columns;
        return SliverList.builder(
          itemCount: rowCount,
          itemBuilder: (BuildContext context, int rowIndex) {
            final int start = rowIndex * columns;
            final Widget rowWidget = IntrinsicHeight(
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: <Widget>[
                  for (int col = 0; col < columns; col++) ...<Widget>[
                    if (col > 0) const SizedBox(width: spacing),
                    Expanded(
                      child: (start + col < sources.length)
                          ? SourceHealthCard(
                              source: sources[start + col],
                              fillRowHeight: true,
                            )
                          : const SizedBox.shrink(),
                    ),
                  ],
                ],
              ),
            );
            if (rowIndex == 0) {
              return rowWidget;
            }
            return Padding(
              padding: const EdgeInsets.only(top: spacing),
              child: rowWidget,
            );
          },
        );
      },
    );
  }
}

class _HoistwayBrandMark extends StatelessWidget {
  const _HoistwayBrandMark();

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    return Container(
      width: 28,
      height: 28,
      decoration: BoxDecoration(
        color: palette.infoContainer,
        borderRadius: BorderRadius.circular(8),
        border: Border.all(color: palette.info.withValues(alpha: 0.45)),
      ),
      child: Icon(Icons.elevator_rounded, size: 17, color: palette.info),
    );
  }
}

class _TopBarIconButton extends StatelessWidget {
  const _TopBarIconButton({
    super.key,
    required this.tooltip,
    required this.onPressed,
    required this.icon,
    this.compact = false,
  });

  final String tooltip;
  final VoidCallback onPressed;
  final IconData icon;
  final bool compact;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    return IconButton(
      tooltip: tooltip,
      onPressed: onPressed,
      style: IconButton.styleFrom(
        minimumSize: Size(compact ? 28 : 34, compact ? 28 : 34),
        padding: EdgeInsets.all(compact ? 4 : 7),
        tapTargetSize: MaterialTapTargetSize.shrinkWrap,
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(8)),
      ),
      icon: Icon(icon, size: compact ? 16 : 18, color: palette.subtleText),
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

    return Card(
      key: const Key('empty-operational-cars-card'),
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: <Widget>[
            Row(
              children: <Widget>[
                Container(
                  width: 32,
                  height: 32,
                  decoration: BoxDecoration(
                    color: palette.cautionContainer,
                    borderRadius: BorderRadius.circular(8),
                  ),
                  child: Icon(
                    Icons.hourglass_top_rounded,
                    size: 17,
                    color: palette.caution,
                  ),
                ),
                const SizedBox(width: 10),
                const Expanded(
                  child: Text(
                    'No resolved operational cars',
                    style: TextStyle(fontSize: 14, fontWeight: FontWeight.w700),
                  ),
                ),
              ],
            ),
            const SizedBox(height: 8),
            Text(
              'Awaiting complete Operational observation state (location, motion, door, mode). Auxiliary bindings remain diagnostic-only.',
              style: TextStyle(
                fontSize: 12.5,
                height: 1.35,
                color: palette.subtleText,
              ),
            ),
            if (pendingOperational.isNotEmpty) ...<Widget>[
              const SizedBox(height: 12),
              const Divider(height: 1),
              const SizedBox(height: 10),
              for (final (
                    String sourceId,
                    SourceObservation obs,
                    PayloadActivity payload,
                  )
                  in pendingOperational)
                Padding(
                  padding: const EdgeInsets.only(bottom: 6),
                  child: Wrap(
                    spacing: 8,
                    runSpacing: 6,
                    crossAxisAlignment: WrapCrossAlignment.center,
                    children: <Widget>[
                      SemanticBadge(
                        label: obs.car,
                        foreground: Theme.of(context).colorScheme.onSurface,
                        background: palette.elevatedSurface,
                        icon: Icons.elevator_outlined,
                      ),
                      SemanticBadge(
                        label: formatAvailabilityLabel(obs.availability),
                        foreground: palette.caution,
                        background: palette.cautionContainer,
                        dotColor: palette.caution,
                        hollowDot: true,
                      ),
                      SemanticBadge(
                        label: 'Source: $sourceId',
                        foreground: palette.subtleText,
                        background: palette.elevatedSurface,
                        outlined: true,
                      ),
                      SemanticBadge(
                        label:
                            'Payload: ${formatPayloadActivityLabel(payload)}',
                        foreground: palette.subtleText,
                        background: palette.elevatedSurface,
                        outlined: true,
                      ),
                    ],
                  ),
                ),
            ],
          ],
        ),
      ),
    );
  }
}
