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

enum CompactSection { cars, sources }

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
  final DiagnosticsFilterController _diagnosticsFilter =
      DiagnosticsFilterController();
  HostEndpoint? _lastEndpoint;

  @override
  void initState() {
    super.initState();
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

  void _openDiagnosticsTerminal() {
    showDiagnosticsTerminalModal(
      context: context,
      controller: widget.controller,
      filterController: _diagnosticsFilter,
    );
  }

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: widget.controller,
      builder: (BuildContext context, _) {
        final ClientState state = widget.controller.state;

        final Size viewport = MediaQuery.sizeOf(context);
        final double textScale = MediaQuery.textScalerOf(context).scale(1.0);
        final bool isSupportingPaneDesktop =
            viewport.width >= 900 && viewport.height >= 440;
        final bool showInlineAppBarHostChips =
            state.snapshot != null &&
            isSupportingPaneDesktop &&
            textScale <= 1.25;

        return Scaffold(
          appBar: _buildTopBar(
            context,
            state,
            screenWidth: viewport.width,
            textScale: textScale,
            showInlineHostChips: showInlineAppBarHostChips,
          ),
          body: SafeArea(
            child: state.snapshot == null
                ? _buildNoSnapshotWorkspace(context, state)
                : _buildConnectedWorkspace(
                    context,
                    state,
                    state.snapshot!,
                    viewport: viewport,
                    textScale: textScale,
                    isSupportingPaneDesktop: isSupportingPaneDesktop,
                    showInlineAppBarHostChips: showInlineAppBarHostChips,
                  ),
          ),
        );
      },
    );
  }

  PreferredSizeWidget _buildTopBar(
    BuildContext context,
    ClientState state, {
    required double screenWidth,
    required double textScale,
    required bool showInlineHostChips,
  }) {
    final InstrumentPalette palette = context.palette;
    final double barHeight = (54.0 * textScale.clamp(1.0, 1.35))
        .roundToDouble();
    final bool isCompactTopBar = screenWidth < 420 || textScale > 1.35;
    final bool showReadOnlyChip = screenWidth >= 680 && textScale <= 1.3;
    final bool showEndpointOriginInline =
        screenWidth >= (560.0 * textScale.clamp(1.0, 1.35));
    final bool showDiagnosticsLabel = screenWidth >= 1080 && textScale <= 1.2;
    final (String phaseLabel, Color phaseColor, Color _, bool phaseHollow) =
        describeConnectionPhase(state.phase, palette);
    final int diagCount = state.snapshot?.recentDiagnostics.length ?? 0;

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
                  // Left Brand + Title + Read-Only M3 Chip
                  Expanded(
                    child: Row(
                      children: <Widget>[
                        const _HoistwayBrandMark(),
                        if (!isCompactTopBar || screenWidth >= 380) ...<Widget>[
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
                        if (showInlineHostChips &&
                            state.snapshot != null) ...<Widget>[
                          const SizedBox(width: 12),
                          Flexible(
                            flex: 3,
                            child: SingleChildScrollView(
                              scrollDirection: Axis.horizontal,
                              child: HostStatusChipGroup(
                                snapshot: state.snapshot!,
                                isLive: state.isSnapshotLive,
                                revisionReset: state.lastRevisionReset,
                              ),
                            ),
                          ),
                        ],
                      ],
                    ),
                  ),
                  SizedBox(width: isCompactTopBar ? 4 : 8),

                  // Pop-up Diagnostics Terminal Action Button with M3 Badge
                  if (state.snapshot != null) ...<Widget>[
                    _DiagnosticsTerminalButton(
                      key: const Key('topbar-diagnostics-button'),
                      count: diagCount,
                      showLabel: showDiagnosticsLabel,
                      compact: isCompactTopBar,
                      onPressed: _openDiagnosticsTerminal,
                    ),
                    SizedBox(width: isCompactTopBar ? 4 : 8),
                  ],

                  // Unified Host Endpoint + Connection Phase Control
                  ConstrainedBox(
                    constraints: BoxConstraints(
                      maxWidth: showEndpointOriginInline
                          ? (screenWidth * 0.34).clamp(200.0, 320.0)
                          : (isCompactTopBar
                                ? (screenWidth * 0.23).clamp(62.0, 116.0)
                                : (screenWidth * 0.38).clamp(108.0, 164.0)),
                    ),
                    child: Material(
                      color: palette.elevatedSurface,
                      borderRadius: BorderRadius.circular(10),
                      child: InkWell(
                        key: const Key('topbar-host-button'),
                        borderRadius: BorderRadius.circular(10),
                        onTap: () => _openHostConfig(state),
                        child: Container(
                          constraints: BoxConstraints(
                            minHeight: isCompactTopBar ? 30 : 34,
                          ),
                          padding: EdgeInsets.symmetric(
                            horizontal: isCompactTopBar ? 6 : 10,
                            vertical: isCompactTopBar ? 3 : 5,
                          ),
                          decoration: BoxDecoration(
                            borderRadius: BorderRadius.circular(10),
                            border: Border.all(color: palette.border),
                          ),
                          child: Row(
                            mainAxisSize: MainAxisSize.min,
                            children: <Widget>[
                              Flexible(
                                child: Row(
                                  key: const Key(
                                    'topbar-connection-phase-badge',
                                  ),
                                  mainAxisSize: MainAxisSize.min,
                                  children: <Widget>[
                                    StatusDot(
                                      color: phaseColor,
                                      hollow: phaseHollow,
                                      size: 7,
                                    ),
                                    const SizedBox(width: 5),
                                    Flexible(
                                      child: Text(
                                        phaseLabel,
                                        maxLines: 1,
                                        overflow: TextOverflow.ellipsis,
                                        style: instrumentTabularStyle(
                                          context,
                                          fontSize: 11.5,
                                          fontWeight: FontWeight.w700,
                                          color: phaseColor,
                                        ),
                                      ),
                                    ),
                                  ],
                                ),
                              ),
                              if (!isCompactTopBar) ...<Widget>[
                                Container(
                                  width: 1,
                                  height: 14,
                                  margin: const EdgeInsets.symmetric(
                                    horizontal: 8,
                                  ),
                                  color: palette.border,
                                ),
                                if (showEndpointOriginInline)
                                  Flexible(
                                    child: Text(
                                      state.endpoint?.origin ?? 'Configure',
                                      maxLines: 1,
                                      overflow: TextOverflow.ellipsis,
                                      style: instrumentTabularStyle(
                                        context,
                                        fontSize: 12,
                                        fontWeight: FontWeight.w500,
                                        color: palette.subtleText,
                                      ),
                                    ),
                                  )
                                else
                                  Icon(
                                    Icons.dns_outlined,
                                    size: 15,
                                    color: palette.subtleText,
                                  ),
                              ],
                            ],
                          ),
                        ),
                      ),
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
                      ClientAlertBanners(
                        state: state,
                        onReconnectNow: widget.controller.reconnectNow,
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
    required bool isSupportingPaneDesktop,
    required bool showInlineAppBarHostChips,
  }) {
    final InstrumentPalette palette = context.palette;
    final bool hasAlerts =
        state.hasRetainedStaleSnapshot || state.error != null;

    if (isSupportingPaneDesktop) {
      const double maxCanvasWidth = 1480.0;

      final Widget carsPane = CustomScrollView(
        key: const Key('desktop-cars-scroll'),
        slivers: <Widget>[
          SliverPadding(
            padding: const EdgeInsets.fromLTRB(24, 20, 12, 0),
            sliver: SliverToBoxAdapter(
              child: SectionHeader(
                title: 'Operational Cars',
                count: snapshot.operationalCars.length,
                icon: Icons.elevator_outlined,
              ),
            ),
          ),
          SliverPadding(
            padding: const EdgeInsets.fromLTRB(24, 14, 12, 24),
            sliver: _buildCarsSliver(context, snapshot, textScale: textScale),
          ),
        ],
      );

      final Widget sourcesPane = CustomScrollView(
        key: const Key('desktop-sources-scroll'),
        slivers: <Widget>[
          SliverPadding(
            padding: const EdgeInsets.fromLTRB(12, 20, 24, 0),
            sliver: SliverToBoxAdapter(
              child: SectionHeader(
                title: 'Observation Sources',
                count: snapshot.sources.length,
                icon: Icons.hub_outlined,
              ),
            ),
          ),
          SliverPadding(
            padding: const EdgeInsets.fromLTRB(12, 14, 24, 24),
            sliver: _buildSourcesSliver(
              context,
              snapshot,
              textScale: textScale,
            ),
          ),
        ],
      );

      final Widget supportingPaneBody = Align(
        alignment: Alignment.topCenter,
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: maxCanvasWidth),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: <Widget>[
              Expanded(flex: 62, child: carsPane),
              VerticalDivider(width: 1, thickness: 1, color: palette.border),
              Expanded(flex: 38, child: sourcesPane),
            ],
          ),
        ),
      );

      return Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: <Widget>[
          if (!showInlineAppBarHostChips)
            _buildCompactHostStatusStrip(context, state, snapshot),
          if (hasAlerts)
            Align(
              alignment: Alignment.topCenter,
              child: ConstrainedBox(
                constraints: const BoxConstraints(maxWidth: maxCanvasWidth),
                child: Padding(
                  padding: const EdgeInsets.fromLTRB(24, 14, 24, 0),
                  child: ClientAlertBanners(
                    state: state,
                    onReconnectNow: widget.controller.reconnectNow,
                    onConfigureHost: () => _openHostConfig(state),
                  ),
                ),
              ),
            ),
          Expanded(
            child: AnimatedOpacity(
              duration: const Duration(milliseconds: 150),
              opacity: state.isSnapshotLive ? 1.0 : 0.72,
              child: supportingPaneBody,
            ),
          ),
        ],
      );
    }

    // Mobile / Narrow layout: Host status chip strip + M3 2-tab bar (Cars / Sources)
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: <Widget>[
        _buildCompactHostStatusStrip(context, state, snapshot),
        _buildCompactTabStrip(context, snapshot),
        Expanded(
          child: AnimatedOpacity(
            duration: const Duration(milliseconds: 150),
            opacity: state.isSnapshotLive ? 1.0 : 0.72,
            child: CustomScrollView(
              slivers: <Widget>[
                if (hasAlerts)
                  SliverPadding(
                    padding: const EdgeInsets.fromLTRB(12, 12, 12, 0),
                    sliver: SliverToBoxAdapter(
                      child: ClientAlertBanners(
                        state: state,
                        onReconnectNow: widget.controller.reconnectNow,
                        onConfigureHost: () => _openHostConfig(state),
                      ),
                    ),
                  ),
                ...switch (_compactSection) {
                  CompactSection.cars => <Widget>[
                    SliverPadding(
                      padding: const EdgeInsets.all(12),
                      sliver: _buildCarsSliver(
                        context,
                        snapshot,
                        textScale: textScale,
                      ),
                    ),
                  ],
                  CompactSection.sources => <Widget>[
                    SliverPadding(
                      padding: const EdgeInsets.all(12),
                      sliver: _buildSourcesSliver(
                        context,
                        snapshot,
                        textScale: textScale,
                      ),
                    ),
                  ],
                },
              ],
            ),
          ),
        ),
      ],
    );
  }

  Widget _buildCompactHostStatusStrip(
    BuildContext context,
    ClientState state,
    ReadModelSnapshot snapshot,
  ) {
    final InstrumentPalette palette = context.palette;
    return DecoratedBox(
      decoration: BoxDecoration(
        color: palette.surface,
        border: Border(bottom: BorderSide(color: palette.border)),
      ),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
        child: HostStatusChipGroup(
          snapshot: snapshot,
          isLive: state.isSnapshotLive,
          revisionReset: state.lastRevisionReset,
        ),
      ),
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
      child: ConstrainedBox(
        constraints: const BoxConstraints(minHeight: 42),
        child: IntrinsicHeight(
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: <Widget>[
              _CompactTabButton(
                key: const Key('compact-tab-cars'),
                icon: Icons.elevator_outlined,
                label: 'Cars',
                count: snapshot.operationalCars.length,
                selected: _compactSection == CompactSection.cars,
                onTap: () =>
                    setState(() => _compactSection = CompactSection.cars),
              ),
              _CompactTabButton(
                key: const Key('compact-tab-sources'),
                icon: Icons.hub_outlined,
                label: 'Sources',
                count: snapshot.sources.length,
                selected: _compactSection == CompactSection.sources,
                onTap: () =>
                    setState(() => _compactSection = CompactSection.sources),
              ),
            ],
          ),
        ),
      ),
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
        if (cars.length == 1) {
          final bool useTabularFooter = width >= 280 && textScale <= 1.35;
          return SliverToBoxAdapter(
            child: OperationalCarCard(
              car: cars.first,
              useTabularFooter: useTabularFooter,
            ),
          );
        }

        if (width < 520 || textScale > 1.45) {
          final bool useTabularFooter = width >= 280 && textScale <= 1.35;
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
}

class _DiagnosticsTerminalButton extends StatelessWidget {
  const _DiagnosticsTerminalButton({
    super.key,
    required this.count,
    required this.showLabel,
    required this.onPressed,
    this.compact = false,
  });

  final int count;
  final bool showLabel;
  final VoidCallback onPressed;
  final bool compact;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;

    return Tooltip(
      message: 'Open Host Diagnostics Terminal ($count recent entries)',
      child: Material(
        color: palette.elevatedSurface,
        borderRadius: BorderRadius.circular(10),
        child: InkWell(
          borderRadius: BorderRadius.circular(10),
          onTap: onPressed,
          child: Container(
            constraints: BoxConstraints(minHeight: compact ? 30 : 34),
            padding: EdgeInsets.symmetric(
              horizontal: compact ? 6 : (showLabel ? 11 : 9),
              vertical: compact ? 3 : 5,
            ),
            decoration: BoxDecoration(
              borderRadius: BorderRadius.circular(10),
              border: Border.all(color: palette.border),
            ),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: <Widget>[
                Icon(
                  Icons.terminal_rounded,
                  size: compact ? 14.5 : 16,
                  color: count > 0 ? palette.info : palette.subtleText,
                ),
                if (showLabel) ...<Widget>[
                  const SizedBox(width: 6),
                  Text(
                    'Diagnostics',
                    style: instrumentTabularStyle(
                      context,
                      fontSize: 12,
                      fontWeight: FontWeight.w600,
                      color: Theme.of(context).colorScheme.onSurface,
                    ),
                  ),
                ],
                SizedBox(width: compact ? 4 : 6),
                Badge.count(
                  key: const Key('topbar-diagnostics-badge'),
                  count: count,
                  backgroundColor: count > 0
                      ? palette.infoContainer
                      : palette.surface,
                  textColor: count > 0 ? palette.info : palette.mutedText,
                  textStyle: instrumentTabularStyle(
                    context,
                    fontSize: compact ? 10 : 11,
                    fontWeight: FontWeight.w700,
                    color: count > 0 ? palette.info : palette.mutedText,
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

class _CompactTabButton extends StatelessWidget {
  const _CompactTabButton({
    super.key,
    required this.icon,
    required this.label,
    required this.count,
    required this.selected,
    required this.onTap,
  });

  final IconData icon;
  final String label;
  final int count;
  final bool selected;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final InstrumentPalette palette = context.palette;
    final double textScale = MediaQuery.textScalerOf(context).scale(1.0);
    final double screenWidth = MediaQuery.sizeOf(context).width;
    final bool showIcon = screenWidth >= 380 && textScale <= 1.35;
    final Color fg = selected
        ? Theme.of(context).colorScheme.onSurface
        : palette.subtleText;

    return Expanded(
      child: InkWell(
        onTap: onTap,
        child: DecoratedBox(
          decoration: BoxDecoration(
            border: selected
                ? Border(bottom: BorderSide(color: palette.info, width: 2.5))
                : null,
          ),
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 8),
            child: Center(
              child: Row(
                mainAxisSize: MainAxisSize.min,
                children: <Widget>[
                  if (showIcon) ...<Widget>[
                    Icon(
                      icon,
                      size: 16,
                      color: selected ? palette.info : palette.mutedText,
                    ),
                    const SizedBox(width: 6),
                  ],
                  Flexible(
                    child: Text(
                      label,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: TextStyle(
                        fontSize: 13,
                        fontWeight: selected
                            ? FontWeight.w700
                            : FontWeight.w500,
                        color: fg,
                      ),
                    ),
                  ),
                  const SizedBox(width: 6),
                  Badge.count(
                    count: count,
                    backgroundColor: selected
                        ? palette.infoContainer
                        : palette.elevatedSurface,
                    textColor: selected ? palette.info : palette.mutedText,
                    textStyle: instrumentTabularStyle(
                      context,
                      fontSize: 11,
                      fontWeight: FontWeight.w700,
                      color: selected ? palette.info : palette.mutedText,
                    ),
                  ),
                ],
              ),
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
