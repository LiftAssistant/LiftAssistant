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
  final DiagnosticsFilterController _diagnosticsFilter =
      DiagnosticsFilterController();
  HostEndpoint? _lastEndpoint;

  @override
  void dispose() {
    _diagnosticsFilter.dispose();
    super.dispose();
  }

  void _syncEndpointReset(HostEndpoint? currentEndpoint) {
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

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: widget.controller,
      builder: (BuildContext context, _) {
        final ClientState state = widget.controller.state;
        _syncEndpointReset(state.endpoint);

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
                    viewport: viewport,
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
    final double textScale = MediaQuery.textScalerOf(context).scale(1.0);
    final double barHeight = (44.0 * textScale.clamp(1.0, 1.45))
        .roundToDouble();
    final bool showReadOnlyTag = screenWidth >= 620 && textScale <= 1.35;
    final bool showEndpointOriginInline =
        screenWidth >= (540.0 * textScale.clamp(1.0, 1.35));
    final (String phaseLabel, Color phaseColor, Color _, bool phaseHollow) =
        describeConnectionPhase(state.phase, palette);

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
              padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 5),
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
                              fontSize: 14,
                              fontWeight: FontWeight.w600,
                              letterSpacing: -0.15,
                            ),
                          ),
                        ),
                        if (showReadOnlyTag) ...<Widget>[
                          const SizedBox(width: 8),
                          Tooltip(
                            message:
                                'Strictly read-only experimental/v0 client; zero elevator write controls.',
                            child: Container(
                              padding: const EdgeInsets.symmetric(
                                horizontal: 6,
                                vertical: 1.5,
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
                          ? (screenWidth * 0.46).clamp(220.0, 350.0)
                          : (screenWidth * 0.42).clamp(110.0, 168.0),
                    ),
                    child: InkWell(
                      key: const Key('topbar-host-button'),
                      borderRadius: BorderRadius.circular(5),
                      onTap: () => _openHostConfig(state),
                      child: ConstrainedBox(
                        constraints: const BoxConstraints(minHeight: 30),
                        child: DecoratedBox(
                          decoration: BoxDecoration(
                            color: palette.elevatedSurface,
                            borderRadius: BorderRadius.circular(5),
                            border: Border.all(color: palette.border),
                          ),
                          child: Padding(
                            padding: const EdgeInsets.symmetric(
                              horizontal: 8,
                              vertical: 4,
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
                                  margin: const EdgeInsets.symmetric(
                                    horizontal: 7,
                                  ),
                                  color: palette.border,
                                ),
                                if (showEndpointOriginInline)
                                  Flexible(
                                    child: Text(
                                      state.endpoint?.origin ?? 'Configure',
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
                                  maxLines: 2,
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
    required Size viewport,
    required bool isSplitDesktop,
  }) {
    final InstrumentPalette palette = context.palette;
    final double textScale = MediaQuery.textScalerOf(context).scale(1.0);
    final bool hasAlerts =
        state.hasRetainedStaleSnapshot || state.error != null;

    if (isSplitDesktop) {
      final double inspectorWidth = (viewport.width * 0.36).clamp(340.0, 460.0);
      final bool pinDiagnosticsToolbar =
          viewport.height >= 540 && textScale <= 1.4;

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
                  // Left primary stage: Sliver-virtualized Operational Cars + Observation Sources
                  Expanded(
                    child: CustomScrollView(
                      slivers: <Widget>[
                        SliverPadding(
                          padding: const EdgeInsets.fromLTRB(16, 14, 16, 0),
                          sliver: SliverToBoxAdapter(
                            child: SectionHeader(
                              title: 'Operational Cars',
                              count: snapshot.operationalCars.length,
                            ),
                          ),
                        ),
                        SliverPadding(
                          padding: const EdgeInsets.fromLTRB(16, 10, 16, 0),
                          sliver: _buildCarsSliver(
                            context,
                            snapshot,
                            textScale: textScale,
                          ),
                        ),
                        SliverPadding(
                          padding: const EdgeInsets.fromLTRB(16, 20, 16, 0),
                          sliver: SliverToBoxAdapter(
                            child: SectionHeader(
                              title: 'Observation Sources',
                              count: snapshot.sources.length,
                            ),
                          ),
                        ),
                        SliverPadding(
                          padding: const EdgeInsets.fromLTRB(16, 10, 16, 16),
                          sliver: _buildSourcesSliver(
                            context,
                            snapshot,
                            textScale: textScale,
                          ),
                        ),
                      ],
                    ),
                  ),
                  VerticalDivider(
                    width: 1,
                    thickness: 1,
                    color: palette.border,
                  ),
                  // Right focused inspector pane: Pinned toolbar + ListView.builder Host Diagnostics
                  SizedBox(
                    width: inspectorWidth,
                    child: pinDiagnosticsToolbar
                        ? Padding(
                            padding: const EdgeInsets.fromLTRB(16, 14, 16, 16),
                            child: Column(
                              crossAxisAlignment: CrossAxisAlignment.stretch,
                              children: <Widget>[
                                Tooltip(
                                  message:
                                      'Diagnostic entries are machine-readable Host evidence only — NOT confirmed elevator equipment faults, root causes, or repair instructions.',
                                  child: SectionHeader(
                                    title: 'Host Diagnostics',
                                    count: snapshot.recentDiagnostics.length,
                                    trailingNote: 'Host evidence · non-fault',
                                  ),
                                ),
                                const SizedBox(height: 10),
                                Expanded(
                                  child: DiagnosticEvidencePanel(
                                    diagnostics: snapshot.recentDiagnostics,
                                    filterController: _diagnosticsFilter,
                                    expandList: true,
                                  ),
                                ),
                              ],
                            ),
                          )
                        : ListenableBuilder(
                            listenable: _diagnosticsFilter,
                            builder: (BuildContext context, _) {
                              return CustomScrollView(
                                slivers: <Widget>[
                                  SliverPadding(
                                    padding: const EdgeInsets.fromLTRB(
                                      16,
                                      14,
                                      16,
                                      10,
                                    ),
                                    sliver: SliverToBoxAdapter(
                                      child: SectionHeader(
                                        title: 'Host Diagnostics',
                                        count:
                                            snapshot.recentDiagnostics.length,
                                        trailingNote:
                                            'Host evidence · non-fault',
                                      ),
                                    ),
                                  ),
                                  for (final Widget s in buildDiagnosticSlivers(
                                    context: context,
                                    diagnostics: snapshot.recentDiagnostics,
                                    controller: _diagnosticsFilter,
                                  ))
                                    SliverPadding(
                                      padding: const EdgeInsets.symmetric(
                                        horizontal: 16,
                                      ),
                                      sliver: s,
                                    ),
                                  const SliverToBoxAdapter(
                                    child: SizedBox(height: 16),
                                  ),
                                ],
                              );
                            },
                          ),
                  ),
                ],
              ),
            ),
          ),
        ],
      );
    }

    // Compact / Mobile / Short-Landscape layout: 1-line telemetry + slim 3-tab strip + lazy slivers
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
          child: AnimatedOpacity(
            duration: const Duration(milliseconds: 150),
            opacity: state.isSnapshotLive ? 1.0 : 0.72,
            child: ListenableBuilder(
              listenable: _diagnosticsFilter,
              builder: (BuildContext context, _) {
                return CustomScrollView(
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
                      CompactSection.diagnostics => <Widget>[
                        const SliverToBoxAdapter(child: SizedBox(height: 10)),
                        for (final Widget s in buildDiagnosticSlivers(
                          context: context,
                          diagnostics: snapshot.recentDiagnostics,
                          controller: _diagnosticsFilter,
                          showCompactCue: true,
                        ))
                          SliverPadding(
                            padding: const EdgeInsets.symmetric(horizontal: 12),
                            sliver: s,
                          ),
                        const SliverToBoxAdapter(child: SizedBox(height: 12)),
                      ],
                    },
                  ],
                );
              },
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
      child: ConstrainedBox(
        constraints: const BoxConstraints(minHeight: 36),
        child: IntrinsicHeight(
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
                onTap: () => setState(
                  () => _compactSection = CompactSection.diagnostics,
                ),
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
    final List<OperationalCar> cars = snapshot.operationalCars;
    if (cars.isEmpty) {
      return SliverToBoxAdapter(
        child: _EmptyOperationalCarsPanel(sources: snapshot.sources),
      );
    }

    return SliverLayoutBuilder(
      builder: (BuildContext context, SliverConstraints sliverConstraints) {
        final double width = sliverConstraints.crossAxisExtent;
        final int columns;
        if (cars.length == 1 || width < 520 || textScale > 1.45) {
          columns = 1;
        } else if (cars.length >= 3 && width >= 860 && textScale <= 1.2) {
          columns = 3;
        } else {
          columns = 2;
        }

        const double spacing = 10;
        final double cardWidth = (width - spacing * (columns - 1)) / columns;
        final bool useTabularFooter = cardWidth >= 250 && textScale <= 1.35;
        final int rowCount = (cars.length + columns - 1) ~/ columns;

        return SliverList.builder(
          itemCount: rowCount,
          itemBuilder: (BuildContext context, int rowIndex) {
            final Widget rowWidget;
            if (columns == 1) {
              rowWidget = OperationalCarCard(
                car: cars[rowIndex],
                useTabularFooter: useTabularFooter,
              );
            } else {
              final int start = rowIndex * columns;
              rowWidget = IntrinsicHeight(
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
    final InstrumentPalette palette = context.palette;
    final List<SourceHealth> sources = snapshot.sources;
    if (sources.isEmpty) {
      return SliverToBoxAdapter(
        child: DecoratedBox(
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
        ),
      );
    }

    return SliverLayoutBuilder(
      builder: (BuildContext context, SliverConstraints sliverConstraints) {
        final double width = sliverConstraints.crossAxisExtent;
        final int columns =
            (sources.length >= 2 && width >= 720 && textScale <= 1.25) ? 2 : 1;
        const double spacing = 8;
        final int rowCount = (sources.length + columns - 1) ~/ columns;

        return SliverList.builder(
          itemCount: rowCount,
          itemBuilder: (BuildContext context, int rowIndex) {
            final Widget rowWidget;
            if (columns == 1) {
              rowWidget = SourceHealthCard(source: sources[rowIndex]);
            } else {
              final int start = rowIndex * columns;
              rowWidget = IntrinsicHeight(
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
            }

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
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 6),
            child: Center(
              child: Row(
                mainAxisSize: MainAxisSize.min,
                children: <Widget>[
                  Flexible(
                    flex: 3,
                    child: Text(
                      label,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: TextStyle(
                        fontSize: 12,
                        fontWeight: selected
                            ? FontWeight.w700
                            : FontWeight.w500,
                        color: fg,
                      ),
                    ),
                  ),
                  const SizedBox(width: 4),
                  Flexible(
                    child: Text(
                      '$count',
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: instrumentMonoStyle(
                        context,
                        fontSize: 11,
                        fontWeight: FontWeight.w600,
                        color: selected ? palette.info : palette.mutedText,
                      ),
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
              'Awaiting complete Operational observation state (location, motion, door, mode). Auxiliary bindings remain diagnostic-only.',
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
