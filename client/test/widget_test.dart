import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:liftassistant_client/src/api/host_url.dart';
import 'package:liftassistant_client/src/api/read_model.dart';
import 'package:liftassistant_client/src/state/client_controller.dart';
import 'package:liftassistant_client/src/transport/host_transport.dart';
import 'package:liftassistant_client/src/ui/app.dart';
import 'package:liftassistant_client/src/ui/preview_fixtures.dart';
import 'package:liftassistant_client/src/ui/widgets/car_card.dart';
import 'package:liftassistant_client/src/ui/widgets/diagnostic_panel.dart';
import 'package:liftassistant_client/src/ui/widgets/host_connection_dialog.dart';
import 'package:liftassistant_client/src/ui/widgets/semantic_badge.dart';
import 'package:liftassistant_client/src/ui/widgets/source_card.dart';

const String _richFixtureJson = '''
{
  "api_version": "experimental/v0",
  "revision": 12,
  "lifecycle": "ready_read_only",
  "readiness": "read_only",
  "operational_cars": [
    {
      "id": "lift-A",
      "location": {"kind": "floor", "floor": 3},
      "motion": {"state": "stopped"},
      "door": "closed",
      "operating_mode": "normal",
      "reported_destination": {"kind": "known", "floor": 6}
    },
    {
      "id": "lift-B",
      "location": {"kind": "continuous", "floor_position": 2.25, "nearest_floor": 2},
      "motion": {"state": "moving", "direction": "up"},
      "door": "opening",
      "operating_mode": "inspection",
      "reported_destination": {"kind": "unknown"}
    }
  ],
  "sources": [
    {
      "source": "rmf/site-main",
      "session": {"state": "active", "session": "session-alpha"},
      "transport": "discovery_observed",
      "payload": "active",
      "observations": [
        {"car": "lift-A", "role": "operational", "availability": "resolved"},
        {"car": "lift-B", "role": "operational", "availability": "resolved"}
      ]
    },
    {
      "source": "bacnet/wing-west",
      "session": {"state": "active", "session": "session-beta"},
      "transport": "discovery_observed",
      "payload": "awaiting",
      "observations": [
        {"car": "lift-A", "role": "auxiliary", "availability": "unresolved"}
      ]
    }
  ],
  "recent_diagnostics": [
    {
      "sequence": 9,
      "layer": "readiness",
      "id": "readiness.changed",
      "source": null,
      "session": null,
      "car": null,
      "role": null,
      "native_entity": null
    },
    {
      "sequence": 10,
      "layer": "mapping",
      "id": "mapping.rejected",
      "source": "bacnet/wing-west",
      "session": "session-beta",
      "car": "lift-A",
      "role": "auxiliary",
      "native_entity": "LiftObj_1"
    }
  ]
}
''';

const String _restartedSynchronizingJson = '''
{
  "api_version": "experimental/v0",
  "revision": 1,
  "lifecycle": "synchronizing",
  "readiness": "not_ready",
  "operational_cars": [],
  "sources": [
    {
      "source": "rmf/site-main",
      "session": {"state": "active", "session": "session-restarted"},
      "transport": "subscriber_created",
      "payload": "awaiting",
      "observations": [
        {"car": "lift-A", "role": "operational", "availability": "awaiting"}
      ]
    }
  ],
  "recent_diagnostics": []
}
''';

final class _WidgetTestTransport implements HostTransport {
  String currentSnapshot = _richFixtureJson;
  StreamController<String> sseController = StreamController<String>(
    onCancel: () async {},
  );

  @override
  Future<String> fetchSnapshotJson(HostEndpoint endpoint) async {
    return currentSnapshot;
  }

  @override
  Future<HostSseConnection> openSnapshotEvents(HostEndpoint endpoint) async {
    sseController = StreamController<String>(onCancel: () async {});
    final StreamController<String> local = sseController;
    return HostSseConnection(
      snapshots: local.stream,
      onClose: () async {
        if (!local.isClosed) {
          unawaited(local.close());
        }
      },
    );
  }

  @override
  void dispose() {
    if (!sseController.isClosed) {
      unawaited(sseController.close());
    }
  }
}

void main() {
  testWidgets(
    'desktop 4-layer architecture renders single-purpose AppBar, left NavigationRail, 4-card SystemStatusStrip, 2-column Overview with Recent Diagnostics preview, and dedicated Cars/Sources/Diagnostics sections without write controls',
    (WidgetTester tester) async {
      tester.view.devicePixelRatio = 1.0;
      tester.view.physicalSize = const Size(1366, 900);
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);

      final _WidgetTestTransport transport = _WidgetTestTransport();
      final ClientController controller = ClientController(
        transport: transport,
        initialUrlInput: 'http://127.0.0.1:8080',
      );
      addTearDown(() {
        controller.dispose();
        transport.dispose();
      });

      await tester.pumpWidget(LiftAssistantClientApp(controller: controller));
      await tester.pumpAndSettle();

      // Onboarding panel sits above mathematical viewport midpoint (optical center).
      final Rect onboardingFormRect = tester.getRect(
        find.byType(HostConnectionForm),
      );
      expect(onboardingFormRect.top, greaterThan(100.0));
      expect(onboardingFormRect.center.dy, lessThan(440.0));

      // Rejects https:// URL in onboarding form.
      await tester.enterText(
        find.byKey(const Key('host-url-input')),
        'https://127.0.0.1:8080',
      );
      await tester.tap(find.byKey(const Key('connect-host-button')));
      await tester.pumpAndSettle();
      expect(find.textContaining('explicit http://'), findsWidgets);

      // Connect with valid http:// URL.
      await tester.enterText(
        find.byKey(const Key('host-url-input')),
        'http://127.0.0.1:8080',
      );
      await tester.tap(find.byKey(const Key('connect-host-button')));
      await tester.pumpAndSettle();

      // Layer 1: Clean single-purpose AppBar (Brand + Read-Only + Host connection controls only).
      expect(find.text('LiftAssistant'), findsOneWidget);
      expect(find.text('Read-Only'), findsOneWidget);
      expect(find.byKey(const Key('topbar-host-button')), findsOneWidget);
      expect(
        find.byKey(const Key('topbar-connection-phase-badge')),
        findsOneWidget,
      );
      expect(find.text('LIVE SSE'), findsOneWidget);
      expect(find.byKey(const Key('topbar-reconnect-button')), findsOneWidget);
      expect(find.byKey(const Key('topbar-semantics-button')), findsOneWidget);
      expect(find.byKey(const Key('topbar-theme-toggle')), findsOneWidget);

      // Layer 2: Left NavigationRail with Overview, Cars, Sources, Diagnostics.
      expect(find.byKey(const Key('desktop-navigation-rail')), findsOneWidget);
      expect(find.byType(NavigationRail), findsOneWidget);
      expect(find.text('Overview'), findsOneWidget);
      expect(find.text('Cars'), findsOneWidget);
      expect(find.text('Sources'), findsOneWidget);
      expect(find.text('Diagnostics'), findsOneWidget);

      // NavigationRail badges must use calm M3 tonal colors, never alarm-red colorScheme.error.
      final Iterable<Badge> railBadges = tester.widgetList<Badge>(
        find.descendant(
          of: find.byType(NavigationRail),
          matching: find.byType(Badge),
        ),
      );
      expect(railBadges, isNotEmpty);
      final Color errorColor = Theme.of(
        tester.element(find.byType(NavigationRail)),
      ).colorScheme.error;
      for (final Badge badge in railBadges) {
        expect(badge.backgroundColor, isNotNull);
        expect(badge.backgroundColor, isNot(equals(errorColor)));
      }

      // Layer 3: Compact 4-card SystemStatusStrip with explicit field labels,
      // equal row heights, and zero duplicate snake_case tags or AI essays.
      expect(find.byKey(const Key('system-status-strip-grid')), findsOneWidget);
      expect(find.text('Host Lifecycle'), findsOneWidget);
      expect(find.text('ReadyReadOnly'), findsOneWidget);
      expect(find.text('Read Readiness'), findsOneWidget);
      expect(find.text('ReadOnly'), findsOneWidget);
      expect(find.text('Host Revision'), findsOneWidget);
      expect(find.text('#12'), findsOneWidget);
      expect(find.text('Live'), findsOneWidget);
      expect(find.text('Snapshot Summary'), findsOneWidget);
      expect(find.text('2 Cars'), findsOneWidget);
      expect(find.text('2 Sources'), findsOneWidget);
      expect(find.text('2 Events'), findsOneWidget);

      // Verify zero duplicate snake_case wire tags or '·' cheap separators in the UI.
      expect(find.text('ready_read_only'), findsNothing);
      expect(find.text('read_only'), findsNothing);
      expect(find.text('live-snapshot'), findsNothing);
      expect(find.textContaining('·'), findsNothing);

      final Rect lifecycleCardRect = tester.getRect(
        find.byKey(const Key('status-card-lifecycle')),
      );
      final Rect readinessCardRect = tester.getRect(
        find.byKey(const Key('status-card-readiness')),
      );
      final Rect revisionCardRect = tester.getRect(
        find.byKey(const Key('status-card-revision')),
      );
      final Rect summaryCardRect = tester.getRect(
        find.byKey(const Key('status-card-summary')),
      );
      expect(lifecycleCardRect.top, equals(readinessCardRect.top));
      expect(lifecycleCardRect.height, equals(readinessCardRect.height));
      expect(lifecycleCardRect.height, equals(revisionCardRect.height));
      expect(lifecycleCardRect.height, equals(summaryCardRect.height));

      // Layer 4: Overview 2-column dashboard (Left: Operational Cars + Observation Sources;
      // Right: Recent Diagnostics lightweight preview without search/filter clutter).
      final Rect carsHeaderRect = tester.getRect(
        find.widgetWithText(SectionHeader, 'Operational Cars'),
      );
      final Rect diagHeaderRect = tester.getRect(
        find.widgetWithText(SectionHeader, 'Recent Diagnostics'),
      );
      expect(carsHeaderRect.top, equals(diagHeaderRect.top));
      expect(carsHeaderRect.left, lessThan(diagHeaderRect.left));

      // Overview preview mode does NOT render the search bar or filter chips,
      // and places the directional forward arrow at the trailing end of the CTA button.
      expect(find.byKey(const Key('diagnostics-search-input')), findsNothing);
      final Finder inspectAllFinder = find.byKey(
        const Key('overview-inspect-all-diagnostics-button'),
      );
      expect(inspectAllFinder, findsOneWidget);
      final Rect inspectLabelRect = tester.getRect(
        find.text('Inspect all 2 diagnostics'),
      );
      final Rect inspectArrowRect = tester.getRect(
        find.descendant(
          of: inspectAllFinder,
          matching: find.byIcon(Icons.arrow_forward_rounded),
        ),
      );
      expect(inspectArrowRect.left, greaterThan(inspectLabelRect.right));
      expect(find.text('readiness.changed'), findsOneWidget);
      expect(find.text('mapping.rejected'), findsOneWidget);

      // Side-by-side car cards share exact row height on 1366x900, and trailing mode chips align flush-right.
      final Rect car0 = tester.getRect(find.byType(OperationalCarCard).at(0));
      final Rect car1 = tester.getRect(find.byType(OperationalCarCard).at(1));
      expect(car0.top, equals(car1.top));
      expect(car0.height, equals(car1.height));

      final Rect car0ModeRect = tester.getRect(
        find.descendant(
          of: find.byType(OperationalCarCard).at(0),
          matching: find.widgetWithText(SemanticBadge, 'Normal'),
        ),
      );
      final Rect car1ModeRect = tester.getRect(
        find.descendant(
          of: find.byType(OperationalCarCard).at(1),
          matching: find.widgetWithText(SemanticBadge, 'Inspection'),
        ),
      );
      expect((car0.right - 16.0 - car0ModeRect.right).abs(), lessThan(2.0));
      expect((car1.right - 16.0 - car1ModeRect.right).abs(), lessThan(2.0));

      // Location precision distinction: Floor Reference vs Continuous position
      // (without redundant 'Discrete floor reference' subtitle under 3F).
      expect(find.text('Floor Reference'), findsOneWidget);
      expect(find.text('3F'), findsOneWidget);
      expect(find.text('Discrete floor reference'), findsNothing);
      expect(find.text('Continuous'), findsOneWidget);
      expect(find.text('2.25 F'), findsOneWidget);
      expect(find.text('Nearest Floor 2'), findsOneWidget);
      expect(find.text('+0.25'), findsOneWidget);

      // Source-reported destination is labeled clearly in the 3-column M3 footer.
      expect(find.text('Reported Destination'), findsNWidgets(2));
      expect(find.text('Floor 6'), findsOneWidget);
      expect(find.text('Unknown'), findsOneWidget);

      // Source Operational vs Auxiliary distinction, equal-width tonal health tiles, & discovery!=payload warning banner.
      expect(
        find.descendant(
          of: find.byType(SourceHealthCard),
          matching: find.text('Session: session-alpha'),
        ),
        findsOneWidget,
      );
      expect(
        find.descendant(
          of: find.byType(SourceHealthCard),
          matching: find.text('Session: session-beta'),
        ),
        findsOneWidget,
      );
      expect(find.text('Discovery Observed'), findsNWidgets(2));
      expect(
        find.descendant(
          of: find.byType(SourceHealthCard),
          matching: find.text('Operational'),
        ),
        findsNWidgets(2),
      );
      expect(
        find.descendant(
          of: find.byType(SourceHealthCard),
          matching: find.text('Auxiliary'),
        ),
        findsOneWidget,
      );
      expect(
        find.textContaining('Discovery observed without active payload'),
        findsOneWidget,
      );

      // Snapshot Summary chips sit on a single horizontal row on desktop.
      final Rect carsChipRect = tester.getRect(find.text('2 Cars'));
      final Rect sourcesChipRect = tester.getRect(find.text('2 Sources'));
      final Rect eventsChipRect = tester.getRect(find.text('2 Events'));
      expect(
        (carsChipRect.center.dy - sourcesChipRect.center.dy).abs(),
        lessThan(2.0),
      );
      expect(
        (carsChipRect.center.dy - eventsChipRect.center.dy).abs(),
        lessThan(2.0),
      );

      // Laptop / tiled desktop widths (1040x800 and 940x720) render without overflow
      // and keep all 3 Snapshot Summary chips on a single line.
      for (final Size compactDesktop in <Size>[
        const Size(1040, 800),
        const Size(940, 720),
      ]) {
        tester.view.physicalSize = compactDesktop;
        await tester.pumpAndSettle();
        expect(tester.takeException(), isNull);
        final Rect cRect = tester.getRect(find.text('2 Cars'));
        final Rect eRect = tester.getRect(find.text('2 Events'));
        expect((cRect.center.dy - eRect.center.dy).abs(), lessThan(2.0));
      }

      tester.view.physicalSize = const Size(1366, 900);
      await tester.pumpAndSettle();

      // Click 'Inspect all 2 diagnostics' in Overview preview to switch to full Diagnostics workbench section.
      await tester.tap(
        find.byKey(const Key('overview-inspect-all-diagnostics-button')),
      );
      await tester.pumpAndSettle();
      expect(find.byKey(const Key('diagnostics-page-header')), findsOneWidget);
      expect(find.byKey(const Key('system-status-strip-grid')), findsNothing);
      expect(find.text('Host Diagnostics'), findsOneWidget);
      expect(find.text('2 events'), findsOneWidget);
      expect(
        find.textContaining('Machine-readable Host evidence only'),
        findsOneWidget,
      );
      expect(find.byKey(const Key('diagnostics-search-input')), findsOneWidget);

      // Structured M3 context chips inside diagnostic entry row.
      final Finder diagPanelFinder = find.byType(DiagnosticEvidencePanel);
      expect(
        find.descendant(
          of: diagPanelFinder,
          matching: find.textContaining('Source: bacnet/wing-west'),
        ),
        findsOneWidget,
      );
      expect(
        find.descendant(
          of: diagPanelFinder,
          matching: find.textContaining('Session: session-beta'),
        ),
        findsOneWidget,
      );
      expect(
        find.descendant(
          of: diagPanelFinder,
          matching: find.textContaining('Car: lift-A'),
        ),
        findsOneWidget,
      );
      expect(
        find.descendant(
          of: diagPanelFinder,
          matching: find.textContaining('Role: Auxiliary'),
        ),
        findsOneWidget,
      );
      expect(
        find.descendant(
          of: diagPanelFinder,
          matching: find.textContaining('Entity: LiftObj_1'),
        ),
        findsOneWidget,
      );

      // Filter by Mapping layer FilterChip.
      await tester.tap(find.byKey(const Key('diag-filter-layer-mapping')));
      await tester.pumpAndSettle();
      expect(find.text('mapping.rejected'), findsOneWidget);
      expect(find.text('readiness.changed'), findsNothing);

      // Reset filter via All FilterChip, test Role SegmentedButton and SearchBar + clear button.
      await tester.tap(find.byKey(const Key('diag-filter-all')));
      await tester.pumpAndSettle();
      expect(find.text('readiness.changed'), findsOneWidget);

      await tester.tap(find.byKey(const Key('diag-filter-role-auxiliary')));
      await tester.pumpAndSettle();
      expect(find.text('mapping.rejected'), findsOneWidget);
      expect(find.text('readiness.changed'), findsNothing);

      await tester.tap(find.byKey(const Key('diag-filter-all')));
      await tester.pumpAndSettle();
      await tester.enterText(
        find.byKey(const Key('diagnostics-search-input')),
        'LiftObj',
      );
      await tester.pumpAndSettle();
      expect(find.text('mapping.rejected'), findsOneWidget);
      expect(find.text('readiness.changed'), findsNothing);

      // When search matches zero entries, empty diagnostics card stays compact.
      await tester.enterText(
        find.byKey(const Key('diagnostics-search-input')),
        'no-such-diagnostic-entry',
      );
      await tester.pumpAndSettle();
      final Finder emptyFilterText = find.text(
        'No diagnostic entries match the active filter.',
      );
      expect(emptyFilterText, findsOneWidget);
      final Rect emptyCardRect = tester.getRect(
        find.ancestor(of: emptyFilterText, matching: find.byType(Card)).first,
      );
      expect(emptyCardRect.height, lessThan(120.0));

      await tester.tap(find.byKey(const Key('diagnostics-search-clear')));
      await tester.pumpAndSettle();
      expect(find.text('readiness.changed'), findsOneWidget);

      // Navigate to dedicated Cars and Sources sections via left NavigationRail:
      // neither repeats the Overview 4-card dashboard strip, and both render DedicatedPageHeader.
      await tester.tap(
        find.descendant(
          of: find.byType(NavigationRail),
          matching: find.text('Cars'),
        ),
      );
      await tester.pumpAndSettle();
      expect(find.byKey(const Key('cars-section-scroll')), findsOneWidget);
      expect(find.byKey(const Key('cars-page-header')), findsOneWidget);
      expect(find.byKey(const Key('system-status-strip-grid')), findsNothing);
      expect(find.text('2 operational cars'), findsOneWidget);
      expect(find.byType(OperationalCarCard), findsNWidgets(2));

      await tester.tap(
        find.descendant(
          of: find.byType(NavigationRail),
          matching: find.text('Sources'),
        ),
      );
      await tester.pumpAndSettle();
      expect(find.byKey(const Key('sources-section-scroll')), findsOneWidget);
      expect(find.byKey(const Key('sources-page-header')), findsOneWidget);
      expect(find.byKey(const Key('system-status-strip-grid')), findsNothing);
      expect(find.text('2 sources'), findsOneWidget);
      expect(find.text('3 bindings'), findsOneWidget);
      expect(find.byType(SourceHealthCard), findsNWidgets(2));

      // Open and close on-demand semantic reference modal.
      await tester.tap(find.byKey(const Key('topbar-semantics-button')));
      await tester.pumpAndSettle();
      expect(find.text('experimental/v0 Semantic Reference'), findsOneWidget);
      await tester.tap(find.byIcon(Icons.close_rounded));
      await tester.pumpAndSettle();

      // Verify strict read-only boundary: zero elevator command actions.
      for (final String forbiddenControl in <String>[
        'Hall Call',
        'Serve Floor',
        'Park Car',
        'Open Door',
        'Close Door',
      ]) {
        expect(find.text(forbiddenControl), findsNothing);
      }
    },
  );

  testWidgets(
    'mobile layout uses compact endpoint bar + bottom NavigationBar across Overview/Cars/Sources/Diagnostics, preserves diagnostics filter across section switches, and surfaces disconnect + Host revision reset',
    (WidgetTester tester) async {
      tester.view.devicePixelRatio = 1.0;
      tester.view.physicalSize = const Size(360, 780);
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);

      final _WidgetTestTransport transport = _WidgetTestTransport();
      final ClientController controller = ClientController(
        transport: transport,
        initialUrlInput: 'http://10.0.2.2:8080',
        retryBackoff: (_) => const Duration(seconds: 30),
      );
      addTearDown(() {
        controller.dispose();
        transport.dispose();
      });

      await tester.pumpWidget(
        LiftAssistantClientApp(
          controller: controller,
          initialHostUrl: 'http://10.0.2.2:8080',
          autoConnect: true,
        ),
      );
      await tester.pumpAndSettle();

      // Mobile renders compact endpoint bar under AppBar and bottom NavigationBar.
      expect(find.byKey(const Key('compact-endpoint-bar')), findsOneWidget);
      expect(find.byKey(const Key('mobile-navigation-bar')), findsOneWidget);
      expect(find.byType(NavigationBar), findsOneWidget);
      expect(find.byType(OperationalCarCard), findsNWidgets(2));
      expect(tester.takeException(), isNull);

      // Switch to Sources section via bottom NavigationBar.
      await tester.tap(
        find.descendant(
          of: find.byType(NavigationBar),
          matching: find.text('Sources'),
        ),
      );
      await tester.pumpAndSettle();
      expect(find.text('rmf/site-main'), findsOneWidget);
      expect(tester.takeException(), isNull);

      // Switch to Diagnostics section via bottom NavigationBar, apply filter + search.
      await tester.tap(
        find.descendant(
          of: find.byType(NavigationBar),
          matching: find.text('Diagnostics'),
        ),
      );
      await tester.pumpAndSettle();
      expect(find.text('mapping.rejected'), findsOneWidget);
      expect(find.text('readiness.changed'), findsOneWidget);

      await tester.tap(find.byKey(const Key('diag-filter-layer-mapping')));
      await tester.enterText(
        find.byKey(const Key('diagnostics-search-input')),
        'LiftObj',
      );
      await tester.pumpAndSettle();
      expect(find.text('readiness.changed'), findsNothing);

      // Switch to Cars section, resize to desktop, and switch back to Diagnostics: filter & query remain intact!
      await tester.tap(
        find.descendant(
          of: find.byType(NavigationBar),
          matching: find.text('Cars'),
        ),
      );
      await tester.pumpAndSettle();

      tester.view.physicalSize = const Size(1280, 800);
      await tester.pumpAndSettle();
      await tester.tap(
        find.descendant(
          of: find.byType(NavigationRail),
          matching: find.text('Diagnostics'),
        ),
      );
      await tester.pumpAndSettle();
      expect(find.text('mapping.rejected'), findsOneWidget);
      expect(find.text('readiness.changed'), findsNothing);

      // Switch back to Overview on mobile.
      tester.view.physicalSize = const Size(360, 780);
      await tester.pumpAndSettle();
      await tester.tap(
        find.descendant(
          of: find.byType(NavigationBar),
          matching: find.text('Overview'),
        ),
      );
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);

      // Toggle daylight mode on mobile and verify clean render.
      await tester.tap(find.byKey(const Key('topbar-theme-toggle')));
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);

      // Simulate connection drop -> non-live retained warning banner appears.
      transport.sseController.addError(
        const HostStreamDisconnectedException('Socket closed'),
      );
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 50));

      expect(
        find.byKey(const Key('stale-retained-snapshot-banner')),
        findsOneWidget,
      );
      expect(find.text('RECONNECTING'), findsOneWidget);
      expect(find.text('Retained'), findsOneWidget);

      // Simulate Host restart (revision resets 12 -> 1, empty operational_cars) and tap Reconnect.
      transport.currentSnapshot = _restartedSynchronizingJson;
      await tester.tap(find.byKey(const Key('banner-reconnect-button')));
      await tester.pumpAndSettle();

      expect(find.text('LIVE SSE'), findsOneWidget);
      expect(find.text('Synchronizing'), findsOneWidget);
      expect(find.text('NotReady'), findsOneWidget);
      expect(find.text('#1'), findsOneWidget);
      expect(find.textContaining('New lifetime (#12 → #1)'), findsOneWidget);
      expect(
        find.byKey(const Key('empty-operational-cars-card')),
        findsOneWidget,
      );
      expect(find.text('Source: rmf/site-main'), findsOneWidget);
      expect(find.text('Payload: Awaiting'), findsOneWidget);
    },
  );

  testWidgets(
    'surfaces hard API/protocol errors alongside retained snapshots, normalizes -0.00 continuous positions, handles long CarIds and 800x300 short landscape viewports, and syncs inline URL form',
    (WidgetTester tester) async {
      tester.view.devicePixelRatio = 1.0;
      tester.view.physicalSize = const Size(1280, 800);
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);

      const String edgeCaseSnapshot = '''
      {
        "api_version": "experimental/v0",
        "revision": 5,
        "lifecycle": "ready_read_only",
        "readiness": "read_only",
        "operational_cars": [
          {
            "id": "building-north-tower-service-elevator-car-01",
            "location": {"kind": "continuous", "floor_position": 1.999, "nearest_floor": 2},
            "motion": {"state": "stopped"},
            "door": "closed",
            "operating_mode": "out_of_service",
            "reported_destination": {"kind": "none"}
          },
          {
            "id": "car-zero",
            "location": {"kind": "continuous", "floor_position": -0.0, "nearest_floor": 0},
            "motion": {"state": "stopped"},
            "door": "closed",
            "operating_mode": "normal",
            "reported_destination": {"kind": "none"}
          },
          {
            "id": "car-outlier",
            "location": {"kind": "continuous", "floor_position": 2.85, "nearest_floor": 2},
            "motion": {"state": "moving", "direction": "down"},
            "door": "obstructed",
            "operating_mode": "fire_service",
            "reported_destination": {"kind": "known", "floor": -1}
          }
        ],
        "sources": [],
        "recent_diagnostics": [
          {
            "sequence": 1,
            "layer": "mapping",
            "id": "mapping.rejected",
            "source": "rmf/site",
            "session": "s1",
            "car": null,
            "role": null,
            "native_entity": ""
          }
        ]
      }
      ''';

      final _WidgetTestTransport transport = _WidgetTestTransport()
        ..currentSnapshot = edgeCaseSnapshot;
      bool failConnect = true;
      final _FakeConditionalTransport conditional = _FakeConditionalTransport(
        onFetch: () async {
          if (failConnect) {
            throw const HostUnreachableException('Offline');
          }
          return edgeCaseSnapshot;
        },
        delegate: transport,
      );
      final ClientController syncController = ClientController(
        transport: conditional,
        initialUrlInput: 'http://127.0.0.1:8080',
        retryBackoff: (_) => const Duration(seconds: 60),
      );
      addTearDown(() {
        syncController.dispose();
        transport.dispose();
      });

      await tester.pumpWidget(
        LiftAssistantClientApp(controller: syncController),
      );
      await tester.pumpAndSettle();

      // Update Host URL via topbar modal while snapshot == null and verify inline form syncs.
      await tester.tap(find.byKey(const Key('topbar-host-button')));
      await tester.pumpAndSettle();
      await tester.enterText(
        find.byKey(const Key('host-url-input')).last,
        'http://192.168.1.99:9090',
      );
      await tester.tap(find.byKey(const Key('connect-host-button')).last);
      await tester.pumpAndSettle();

      final TextField inlineField = tester.widget<TextField>(
        find.byKey(const Key('host-url-input')),
      );
      expect(inlineField.controller?.text, 'http://192.168.1.99:9090');

      // Now bring Host online and connect.
      failConnect = false;
      await tester.tap(find.byKey(const Key('error-retry-button')));
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);

      // Verify -0.00 / +-0.00 NEVER appear anywhere, and +0.85 outlier delta renders accurately.
      expect(find.textContaining('-0.00'), findsNothing);
      expect(find.textContaining('+-0.00'), findsNothing);
      expect(find.text('0.00 F'), findsOneWidget);
      expect(find.text('Nearest Floor 2'), findsNWidgets(2));
      expect(find.text('Nearest Floor 0'), findsOneWidget);
      expect(find.text('+0.00'), findsNWidgets(2));
      expect(find.text('+0.85'), findsOneWidget);
      expect(find.text('Floor -1'), findsOneWidget);

      // Verify long CarId does not overflow on 360x780 mobile portrait or 800x300 short landscape.
      tester.view.physicalSize = const Size(360, 780);
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);

      tester.view.physicalSize = const Size(800, 300);
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);

      await tester.tap(find.byKey(const Key('topbar-host-button')));
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
      Navigator.of(tester.element(find.byType(Dialog))).pop();
      await tester.pumpAndSettle();

      // Switch to Diagnostics section on 800x300 short landscape, filter, and switch Host endpoint.
      await tester.tap(
        find.descendant(
          of: find.byType(NavigationRail),
          matching: find.text('Diagnostics'),
        ),
      );
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
      await tester.enterText(
        find.byKey(const Key('diagnostics-search-input')),
        'unmatched-filter',
      );
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
      expect(find.text('mapping.rejected'), findsNothing);

      // Switching Host endpoint resets filter outside build() without error.
      unawaited(syncController.connectToUrl('http://192.168.1.100:9090'));
      await tester.pump();
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
      expect(find.text('mapping.rejected'), findsOneWidget);

      // Emit unsupported API version while snapshot #5 is retained: BOTH error banner and stale banner must show.
      tester.view.physicalSize = const Size(1280, 800);
      await tester.pumpAndSettle();
      transport.sseController.add('{"api_version": "experimental/v1"}');
      await tester.pumpAndSettle();

      expect(find.byKey(const Key('connection-error-banner')), findsOneWidget);
      expect(
        find.textContaining('Unsupported Host API version (experimental/v1)'),
        findsOneWidget,
      );
      expect(
        find.byKey(const Key('stale-retained-snapshot-banner')),
        findsOneWidget,
      );

      // Manual disconnect with retained snapshot shows Client disconnected rather than Connection lost.
      await syncController.disconnect();
      await tester.pumpAndSettle();
      expect(
        find.text('Client disconnected — retained snapshot (#5) is NOT live'),
        findsOneWidget,
      );
    },
  );

  testWidgets(
    'handles 2.0x OS accessibility text scaling without overflow and lazily virtualizes large car/diagnostic fleets',
    (WidgetTester tester) async {
      tester.view.devicePixelRatio = 1.0;
      tester.view.physicalSize = const Size(320, 568);
      tester.platformDispatcher.textScaleFactorTestValue = 2.0;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      addTearDown(tester.platformDispatcher.clearTextScaleFactorTestValue);

      final StringBuffer carsBuf = StringBuffer();
      for (int i = 0; i < 60; i++) {
        if (i > 0) carsBuf.write(',');
        carsBuf.write('''
        {
          "id": "fleet-car-$i",
          "location": {"kind": "floor", "floor": ${i % 20}},
          "motion": {"state": "moving", "direction": "up"},
          "door": "closed",
          "operating_mode": "normal",
          "reported_destination": {"kind": "known", "floor": ${(i + 3) % 20}}
        }
        ''');
      }

      final StringBuffer diagBuf = StringBuffer();
      for (int i = 1; i <= 200; i++) {
        if (i > 1) diagBuf.write(',');
        diagBuf.write('''
        {
          "sequence": $i,
          "layer": "observation",
          "id": "observation.updated.$i",
          "source": "rmf/site-main",
          "session": "session-alpha",
          "car": "fleet-car-${i % 60}",
          "role": "operational",
          "native_entity": "Lift_$i"
        }
        ''');
      }

      final String largeFleetJson =
          '''
      {
        "api_version": "experimental/v0",
        "revision": 42,
        "lifecycle": "ready_read_only",
        "readiness": "read_only",
        "operational_cars": [$carsBuf],
        "sources": [
          {
            "source": "rmf/site-main",
            "session": {"state": "active", "session": "session-alpha"},
            "transport": "discovery_observed",
            "payload": "awaiting",
            "observations": [
              {"car": "fleet-car-0", "role": "operational", "availability": "resolved"},
              {"car": "fleet-car-1", "role": "auxiliary", "availability": "unresolved"}
            ]
          }
        ],
        "recent_diagnostics": [$diagBuf]
      }
      ''';

      final _WidgetTestTransport transport = _WidgetTestTransport()
        ..currentSnapshot = largeFleetJson;
      final ClientController controller = ClientController(
        transport: transport,
        initialUrlInput: 'http://127.0.0.1:8080',
      );
      addTearDown(() {
        controller.dispose();
        transport.dispose();
      });

      await tester.pumpWidget(
        LiftAssistantClientApp(
          controller: controller,
          initialHostUrl: 'http://127.0.0.1:8080',
          autoConnect: true,
        ),
      );
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);

      // SliverList.builder lazy virtualization: far fewer than 60 OperationalCarCards materialized!
      expect(
        tester.widgetList(find.byType(OperationalCarCard)).length,
        lessThan(15),
      );

      // Check Cars, Sources, and Diagnostics sections at 2.0x text scale on 320x568 without overflow!
      await tester.tap(
        find.descendant(
          of: find.byType(NavigationBar),
          matching: find.text('Cars'),
        ),
      );
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
      expect(
        tester.widgetList(find.byType(OperationalCarCard)).length,
        lessThan(15),
      );

      await tester.tap(
        find.descendant(
          of: find.byType(NavigationBar),
          matching: find.text('Sources'),
        ),
      );
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);

      await tester.tap(
        find.descendant(
          of: find.byType(NavigationBar),
          matching: find.text('Diagnostics'),
        ),
      );
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
      expect(find.text('observation.updated.200'), findsOneWidget);
      expect(find.text('observation.updated.1'), findsNothing);

      // Return to Overview and check desktop 1366x900 and 800x300 at 2.0x text scale,
      // plus 320x568 at intermediate 1.3x text scale, then 1366x900 at 1.0x text scale.
      await tester.tap(
        find.descendant(
          of: find.byType(NavigationBar),
          matching: find.text('Overview'),
        ),
      );
      await tester.pumpAndSettle();

      tester.view.physicalSize = const Size(1366, 900);
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);

      tester.view.physicalSize = const Size(800, 300);
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);

      tester.view.physicalSize = const Size(320, 568);
      tester.platformDispatcher.textScaleFactorTestValue = 1.3;
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
      await tester.tap(
        find.descendant(
          of: find.byType(NavigationBar),
          matching: find.text('Diagnostics'),
        ),
      );
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
      await tester.tap(
        find.descendant(
          of: find.byType(NavigationBar),
          matching: find.text('Overview'),
        ),
      );
      await tester.pumpAndSettle();

      // Intermediate tablet/laptop widths (760px–1000px) with 1.25x–1.35x text scaling
      // and multi-digit counts (60 Cars, 1 Source, 200 Events) render without overflow.
      for (final double scale in <double>[1.25, 1.35]) {
        tester.platformDispatcher.textScaleFactorTestValue = scale;
        for (final Size tabletLaptopSize in <Size>[
          const Size(760, 800),
          const Size(880, 800),
          const Size(1000, 800),
        ]) {
          tester.view.physicalSize = tabletLaptopSize;
          await tester.pumpAndSettle();
          expect(tester.takeException(), isNull);
        }
      }

      tester.view.physicalSize = const Size(1366, 900);
      tester.platformDispatcher.clearTextScaleFactorTestValue();
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
      expect(
        tester.widgetList(find.byType(OperationalCarCard)).length,
        lessThan(20),
      );

      await tester.tap(
        find.descendant(
          of: find.byType(NavigationRail),
          matching: find.text('Diagnostics'),
        ),
      );
      await tester.pumpAndSettle();
      expect(find.text('observation.updated.200'), findsOneWidget);
      expect(find.text('observation.updated.1'), findsNothing);
    },
  );

  testWidgets(
    'balances single-car, single-source, and Recent Diagnostics side-by-side in desktop Overview and bounds single cards to 1 grid column in dedicated Cars/Sources views',
    (WidgetTester tester) async {
      tester.view.devicePixelRatio = 1.0;
      tester.view.physicalSize = const Size(1366, 900);
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);

      const String singleCarFixtureJson = '''
      {
        "api_version": "experimental/v0",
        "revision": 7,
        "lifecycle": "ready_read_only",
        "readiness": "read_only",
        "operational_cars": [
          {
            "id": "lift-A",
            "location": {"kind": "continuous", "floor_position": 4.0, "nearest_floor": 4},
            "motion": {"state": "stopped"},
            "door": "open",
            "operating_mode": "fire_service",
            "reported_destination": {"kind": "none"}
          }
        ],
        "sources": [
          {
            "source": "fixture/headless-recorded",
            "session": {"state": "active", "session": "headless-fixture-session-A"},
            "transport": "unknown",
            "payload": "active",
            "observations": [
              {"car": "lift-A", "role": "operational", "availability": "resolved"}
            ]
          }
        ],
        "recent_diagnostics": [
          {
            "sequence": 6,
            "layer": "readiness",
            "id": "host.readiness.ready_read_only",
            "source": null,
            "session": null,
            "car": null,
            "role": null,
            "native_entity": null
          }
        ]
      }
      ''';

      final _WidgetTestTransport transport = _WidgetTestTransport()
        ..currentSnapshot = singleCarFixtureJson;
      final ClientController controller = ClientController(
        transport: transport,
        initialUrlInput: 'http://127.0.0.1:8080',
      );
      addTearDown(() {
        controller.dispose();
        transport.dispose();
      });

      await tester.pumpWidget(
        LiftAssistantClientApp(
          controller: controller,
          initialHostUrl: 'http://127.0.0.1:8080',
          autoConnect: true,
        ),
      );
      await tester.pumpAndSettle();

      // In desktop Overview, OperationalCarCard and SourceHealthCard stack cleanly
      // in the left column while Recent Diagnostics sits side-by-side in the right column.
      final Rect singleCarRect = tester.getRect(
        find.byType(OperationalCarCard),
      );
      final Rect singleSrcRect = tester.getRect(find.byType(SourceHealthCard));
      final Rect diagPanelRect = tester.getRect(
        find.byType(DiagnosticEvidencePanel),
      );
      expect(singleCarRect.left, equals(singleSrcRect.left));
      expect(singleCarRect.width, equals(singleSrcRect.width));
      expect((singleCarRect.top - diagPanelRect.top).abs(), lessThan(2.0));
      expect(singleCarRect.right, lessThan(diagPanelRect.left));

      // In dedicated Cars and Sources sections on desktop, a single card occupies
      // 1 column of the adaptive grid rather than stretching across the full workspace.
      await tester.tap(
        find.descendant(
          of: find.byType(NavigationRail),
          matching: find.text('Cars'),
        ),
      );
      await tester.pumpAndSettle();
      final Rect carsTabSingleCardRect = tester.getRect(
        find.byType(OperationalCarCard),
      );
      expect(carsTabSingleCardRect.width, lessThan(700.0));

      await tester.tap(
        find.descendant(
          of: find.byType(NavigationRail),
          matching: find.text('Sources'),
        ),
      );
      await tester.pumpAndSettle();
      final Rect sourcesTabSingleCardRect = tester.getRect(
        find.byType(SourceHealthCard),
      );
      expect(sourcesTabSingleCardRect.width, lessThan(700.0));
      expect(
        sourcesTabSingleCardRect.width,
        equals(carsTabSingleCardRect.width),
      );
    },
  );

  testWidgets(
    'stress-tests synthetic multi-car (4) / multi-source (3) / 24-diagnostic / NotReady / Reconnecting state across wide (1560px), medium (900px), and narrow/mobile (390px) viewports',
    (WidgetTester tester) async {
      tester.view.devicePixelRatio = 1.0;
      tester.view.physicalSize = const Size(1560, 960);
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);

      final PreviewFixtureTransport transport = PreviewFixtureTransport(
        simulateReconnectingAfterInitialSnapshot: true,
      );
      final ClientController controller = ClientController(
        transport: transport,
        initialUrlInput: 'http://127.0.0.1:8080',
        retryBackoff: (_) => const Duration(seconds: 30),
      );
      addTearDown(() {
        controller.dispose();
        transport.dispose();
      });

      await tester.pumpWidget(
        LiftAssistantClientApp(
          controller: controller,
          initialHostUrl: 'http://127.0.0.1:8080',
          autoConnect: true,
        ),
      );
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 50));
      expect(tester.takeException(), isNull);

      // 1. Wide Desktop (1560x960) — Overview:
      // Shows Global Summary (Synchronizing / NotReady / #84 Retained / 4 Cars, 3 Sources, 24 Events),
      // Reconnecting retained-snapshot alert banner, 2-column dashboard layout, and 6-item preview.
      expect(find.byKey(const Key('system-status-strip-grid')), findsOneWidget);
      expect(
        find.byKey(const Key('stale-retained-snapshot-banner')),
        findsOneWidget,
      );
      expect(find.text('RECONNECTING'), findsOneWidget);
      expect(find.text('Synchronizing'), findsOneWidget);
      expect(find.text('NotReady'), findsOneWidget);
      expect(find.text('4 Cars'), findsOneWidget);
      expect(find.text('3 Sources'), findsOneWidget);
      expect(find.text('24 Events'), findsOneWidget);
      expect(find.text('Showing 6 of 24 recent events'), findsOneWidget);

      final Rect wideOvCarsHeader = tester.getRect(
        find.widgetWithText(SectionHeader, 'Operational Cars'),
      );
      final Rect wideOvDiagHeader = tester.getRect(
        find.widgetWithText(SectionHeader, 'Recent Diagnostics'),
      );
      expect(wideOvCarsHeader.top, equals(wideOvDiagHeader.top));
      expect(wideOvCarsHeader.left, lessThan(wideOvDiagHeader.left));

      // 2. Wide Desktop (1560x960) — Dedicated Cars page:
      // No Overview summary cards; renders DedicatedPageHeader + 3-column adaptive grid for 4 cars,
      // with zero horizontal layout jump from Overview's left content margin.
      await tester.tap(
        find.descendant(
          of: find.byType(NavigationRail),
          matching: find.text('Cars'),
        ),
      );
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
      expect(find.byKey(const Key('system-status-strip-grid')), findsNothing);
      expect(find.byKey(const Key('cars-page-header')), findsOneWidget);
      final Rect carsPageHeaderRect = tester.getRect(
        find.byKey(const Key('cars-page-header')),
      );
      expect(carsPageHeaderRect.left, equals(wideOvCarsHeader.left));
      expect(find.text('4 operational cars'), findsOneWidget);
      expect(
        find.byKey(const Key('cars-page-non-resolved-badge')),
        findsOneWidget,
      );
      expect(find.text('1 non-resolved binding'), findsOneWidget);
      expect(
        find.byKey(const Key('stale-retained-snapshot-banner')),
        findsOneWidget,
      );
      expect(find.byType(OperationalCarCard), findsNWidgets(4));

      final Rect wideCar0 = tester.getRect(
        find.byType(OperationalCarCard).at(0),
      );
      final Rect wideCar1 = tester.getRect(
        find.byType(OperationalCarCard).at(1),
      );
      final Rect wideCar2 = tester.getRect(
        find.byType(OperationalCarCard).at(2),
      );
      final Rect wideCar3 = tester.getRect(
        find.byType(OperationalCarCard).at(3),
      );
      expect(wideCar0.top, equals(wideCar1.top));
      expect(wideCar1.top, equals(wideCar2.top));
      expect(wideCar0.left, lessThan(wideCar1.left));
      expect(wideCar1.left, lessThan(wideCar2.left));
      expect(wideCar3.top, greaterThan(wideCar0.bottom));
      expect((wideCar3.left - wideCar0.left).abs(), lessThan(1.0));
      expect((wideCar3.width - wideCar0.width).abs(), lessThan(1.0));

      // 3. Wide Desktop (1560x960) — Dedicated Sources page:
      // No Overview summary cards; renders DedicatedPageHeader + 3-column adaptive grid for 3 sources.
      await tester.tap(
        find.descendant(
          of: find.byType(NavigationRail),
          matching: find.text('Sources'),
        ),
      );
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
      expect(find.byKey(const Key('system-status-strip-grid')), findsNothing);
      expect(find.byKey(const Key('sources-page-header')), findsOneWidget);
      final Rect sourcesPageHeaderRect = tester.getRect(
        find.byKey(const Key('sources-page-header')),
      );
      expect(sourcesPageHeaderRect.left, equals(wideOvCarsHeader.left));
      expect(find.text('3 sources'), findsOneWidget);
      expect(find.text('9 bindings'), findsOneWidget);
      expect(find.text('1 stale'), findsOneWidget);
      expect(find.byType(SourceHealthCard), findsNWidgets(3));

      final Rect wideSrc0 = tester.getRect(find.byType(SourceHealthCard).at(0));
      final Rect wideSrc1 = tester.getRect(find.byType(SourceHealthCard).at(1));
      final Rect wideSrc2 = tester.getRect(find.byType(SourceHealthCard).at(2));
      expect(wideSrc0.top, equals(wideSrc1.top));
      expect(wideSrc1.top, equals(wideSrc2.top));
      expect(wideSrc0.left, lessThan(wideSrc1.left));
      expect(wideSrc1.left, lessThan(wideSrc2.left));
      expect(wideSrc0.height, equals(wideSrc1.height));
      expect(wideSrc1.height, equals(wideSrc2.height));

      // Verify first binding row in shorter Card 2 (2 bindings) is top-aligned at the exact
      // same vertical position as the first binding row in Card 0 (4 bindings) without a mid-card Spacer gap.
      final Rect src0FirstBindingText = tester.getRect(
        find.descendant(
          of: find.byType(SourceHealthCard).at(0),
          matching: find.text('lift-north-A'),
        ),
      );
      final Rect src2FirstBindingText = tester.getRect(
        find.descendant(
          of: find.byType(SourceHealthCard).at(2),
          matching: find.text('lift-west-express-C'),
        ),
      );
      expect(
        (src0FirstBindingText.top - src2FirstBindingText.top).abs(),
        lessThan(2.0),
      );

      // Verify long CarId binding row in Card 0 stays on a single horizontal line
      // rather than wrapping its badges to a second line.
      final Rect src0LongCarText = tester.getRect(
        find.descendant(
          of: find.byType(SourceHealthCard).at(0),
          matching: find.text(
            'building-north-tower-high-rise-service-elevator-car-02',
          ),
        ),
      );
      expect(
        (src0LongCarText.top - src0FirstBindingText.bottom).abs(),
        lessThan(30.0),
      );

      // 4. Wide Desktop (1560x960) — Dedicated Diagnostics page:
      await tester.tap(
        find.descendant(
          of: find.byType(NavigationRail),
          matching: find.text('Diagnostics'),
        ),
      );
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
      expect(find.byKey(const Key('system-status-strip-grid')), findsNothing);
      expect(find.byKey(const Key('diagnostics-page-header')), findsOneWidget);
      final Rect diagPageHeaderRect = tester.getRect(
        find.byKey(const Key('diagnostics-page-header')),
      );
      expect(diagPageHeaderRect.left, equals(wideOvCarsHeader.left));
      expect(find.text('24 events'), findsOneWidget);
      expect(find.byKey(const Key('diagnostics-list-view')), findsOneWidget);

      // Filtering updates DedicatedPageHeader with matching badge + Reset filters action.
      await tester.tap(find.byKey(const Key('diag-filter-layer-mapping')));
      await tester.pumpAndSettle();
      expect(
        find.byKey(const Key('diagnostics-page-filtered-badge')),
        findsOneWidget,
      );
      expect(find.text('3 matching'), findsOneWidget);
      expect(
        find.byKey(const Key('diagnostics-header-reset-button')),
        findsOneWidget,
      );
      await tester.tap(
        find.byKey(const Key('diagnostics-header-reset-button')),
      );
      await tester.pumpAndSettle();
      expect(
        find.byKey(const Key('diagnostics-page-filtered-badge')),
        findsNothing,
      );

      // 5. Medium Layout (900x800 tablet / small desktop):
      // Overview reflows vertically (Operational Cars above Observation Sources above Recent Diagnostics)
      // while Cars and Sources use a 2-column adaptive grid.
      tester.view.physicalSize = const Size(900, 800);
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);

      await tester.tap(
        find.descendant(
          of: find.byType(NavigationRail),
          matching: find.text('Overview'),
        ),
      );
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
      expect(find.byType(CustomScrollView), findsOneWidget);
      final Rect ovMedCar0 = tester.getRect(
        find.byType(OperationalCarCard).at(0),
      );
      final Rect ovMedCar1 = tester.getRect(
        find.byType(OperationalCarCard).at(1),
      );
      expect(ovMedCar0.top, equals(ovMedCar1.top));
      expect(ovMedCar0.left, lessThan(ovMedCar1.left));
      // Recent Diagnostics is stacked below in CustomScrollView rather than side-by-side at the top.
      expect(
        find.widgetWithText(SectionHeader, 'Recent Diagnostics'),
        findsNothing,
      );

      await tester.tap(
        find.descendant(
          of: find.byType(NavigationRail),
          matching: find.text('Cars'),
        ),
      );
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
      final Rect medCar0 = tester.getRect(
        find.byType(OperationalCarCard).at(0),
      );
      final Rect medCar1 = tester.getRect(
        find.byType(OperationalCarCard).at(1),
      );
      final Rect medCar2 = tester.getRect(
        find.byType(OperationalCarCard).at(2),
      );
      expect(medCar0.top, equals(medCar1.top));
      expect(medCar0.left, lessThan(medCar1.left));
      expect(medCar2.top, greaterThan(medCar0.bottom));

      // 6. Narrow / Mobile Layout (390x844):
      // Cars, Sources, and Diagnostics stack in 1 column with bottom NavigationBar and zero overflow.
      tester.view.physicalSize = const Size(390, 844);
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
      expect(find.byType(NavigationBar), findsOneWidget);

      final Rect mobCar0 = tester.getRect(
        find.byType(OperationalCarCard, skipOffstage: false).at(0),
      );
      final Rect mobCar1 = tester.getRect(
        find.byType(OperationalCarCard, skipOffstage: false).at(1),
      );
      expect(mobCar1.top, greaterThan(mobCar0.bottom));
      expect(mobCar0.left, equals(mobCar1.left));

      await tester.tap(
        find.descendant(
          of: find.byType(NavigationBar),
          matching: find.text('Sources'),
        ),
      );
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
      await tester.drag(
        find.byKey(const Key('sources-section-scroll')),
        const Offset(0, -380),
      );
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
      final Rect mobSrc0 = tester.getRect(
        find.byType(SourceHealthCard, skipOffstage: false).at(0),
      );
      final Rect mobSrc1 = tester.getRect(
        find.byType(SourceHealthCard, skipOffstage: false).at(1),
      );
      expect(mobSrc1.top, greaterThan(mobSrc0.bottom));
      expect(mobSrc0.left, equals(mobSrc1.left));

      await tester.tap(
        find.descendant(
          of: find.byType(NavigationBar),
          matching: find.text('Diagnostics'),
        ),
      );
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
      expect(find.byKey(const Key('diagnostics-page-header')), findsOneWidget);
      expect(find.byKey(const Key('system-status-strip-grid')), findsNothing);

      await controller.disconnect();
      await tester.pumpAndSettle();
    },
  );

  test('projectConfiguredOperationalCars separates resolved vs unavailable Operational cars, deduplicates by logical CarId, and excludes Auxiliary bindings', () {
    const OperationalCar carA = OperationalCar(
      id: 'lift-A',
      location: FloorLocation(floor: 3),
      motion: StoppedMotion(),
      door: CarDoorState.closed,
      operatingMode: CarOperatingMode.normal,
      reportedDestination: NoReportedDestination(),
    );

    final ReadModelSnapshot snapshot = ReadModelSnapshot(
      apiVersion: 'experimental/v0',
      revision: 7,
      lifecycle: HostLifecycle.synchronizing,
      readiness: HostReadiness.notReady,
      operationalCars: const <OperationalCar>[carA, carA],
      sources: const <SourceHealth>[
        SourceHealth(
          source: 'rmf/site-main',
          session: ActiveSourceSession(session: 's1'),
          transport: TransportState.discoveryObserved,
          payload: PayloadActivity.active,
          observations: <SourceObservation>[
            SourceObservation(
              car: 'lift-A',
              role: BindingRole.operational,
              availability: ObservationAvailability.resolved,
            ),
            SourceObservation(
              car: 'lift-B',
              role: BindingRole.operational,
              availability: ObservationAvailability.revoked,
            ),
            SourceObservation(
              car: 'lift-D',
              role: BindingRole.operational,
              availability: ObservationAvailability.awaiting,
            ),
            // Defensive case: Operational binding marked resolved in sources
            // while absent from operational_cars.
            SourceObservation(
              car: 'lift-E',
              role: BindingRole.operational,
              availability: ObservationAvailability.resolved,
            ),
          ],
        ),
        // Defensive duplicate Operational entry for lift-B plus resolved Auxiliary entries
        // for lift-B and Auxiliary-only lift-C.
        SourceHealth(
          source: 'bacnet/wing-west',
          session: ActiveSourceSession(session: 's2'),
          transport: TransportState.discoveryObserved,
          payload: PayloadActivity.active,
          observations: <SourceObservation>[
            SourceObservation(
              car: 'lift-B',
              role: BindingRole.auxiliary,
              availability: ObservationAvailability.resolved,
            ),
            SourceObservation(
              car: 'lift-B',
              role: BindingRole.operational,
              availability: ObservationAvailability.unresolved,
            ),
            SourceObservation(
              car: 'lift-C',
              role: BindingRole.auxiliary,
              availability: ObservationAvailability.resolved,
            ),
          ],
        ),
      ],
      recentDiagnostics: const <DiagnosticEntry>[],
    );

    final ConfiguredOperationalCarsView view = projectConfiguredOperationalCars(
      snapshot,
    );

    expect(view.resolvedCars, equals(const <OperationalCar>[carA]));
    expect(view.unavailableCars.length, equals(3));
    expect(
      view.unavailableCars.map((UnavailableOperationalCar c) => c.carId),
      equals(<String>['lift-B', 'lift-D', 'lift-E']),
    );
    expect(view.unavailableCars[0].source, equals('rmf/site-main'));
    expect(
      view.unavailableCars[0].availability,
      equals(ObservationAvailability.revoked),
    );
    expect(
      view.unavailableCars[1].availability,
      equals(ObservationAvailability.awaiting),
    );
    expect(
      view.unavailableCars[2].availability,
      equals(ObservationAvailability.resolved),
    );
    expect(
      view.slots.map((OperationalCarSlot s) => s.carId),
      equals(<String>['lift-A', 'lift-B', 'lift-D', 'lift-E']),
    );
    expect(view.slots[0], isA<ResolvedCarSlot>());
    expect(view.slots[1], isA<UnavailableCarSlot>());
    expect(view.slots[2], isA<UnavailableCarSlot>());
    expect(view.slots[3], isA<UnavailableCarSlot>());
  });

  testWidgets(
    'preserves configured Operational cars across 2->1->2 transitions without stale values, Auxiliary fallback, Auxiliary-only leakage, or duplicates',
    (WidgetTester tester) async {
      tester.view.devicePixelRatio = 1.0;
      tester.view.physicalSize = const Size(1366, 900);
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);

      // Case B + Case C + Case G:
      // - lift-A: Operational resolved
      // - lift-B: Operational revoked (rmf/site-main) AND Auxiliary resolved (bacnet/wing-west)
      // - lift-C: Auxiliary-only resolved (bacnet/wing-west, NO Operational binding)
      const String partialRevokedWithAuxJson = '''
      {
        "api_version": "experimental/v0",
        "revision": 13,
        "lifecycle": "synchronizing",
        "readiness": "not_ready",
        "operational_cars": [
          {
            "id": "lift-A",
            "location": {"kind": "floor", "floor": 3},
            "motion": {"state": "stopped"},
            "door": "closed",
            "operating_mode": "normal",
            "reported_destination": {"kind": "known", "floor": 6}
          }
        ],
        "sources": [
          {
            "source": "rmf/site-main",
            "session": {"state": "active", "session": "session-alpha"},
            "transport": "discovery_observed",
            "payload": "active",
            "observations": [
              {"car": "lift-A", "role": "operational", "availability": "resolved"},
              {"car": "lift-B", "role": "operational", "availability": "revoked"}
            ]
          },
          {
            "source": "bacnet/wing-west",
            "session": {"state": "active", "session": "session-beta"},
            "transport": "discovery_observed",
            "payload": "active",
            "observations": [
              {"car": "lift-B", "role": "auxiliary", "availability": "resolved"},
              {"car": "lift-C", "role": "auxiliary", "availability": "resolved"}
            ]
          }
        ],
        "recent_diagnostics": []
      }
      ''';

      String partialAvailabilityJson(
        int revision,
        String availability, {
        String payload = 'active',
      }) =>
          '''
      {
        "api_version": "experimental/v0",
        "revision": $revision,
        "lifecycle": "synchronizing",
        "readiness": "not_ready",
        "operational_cars": [
          {
            "id": "lift-A",
            "location": {"kind": "floor", "floor": 3},
            "motion": {"state": "stopped"},
            "door": "closed",
            "operating_mode": "normal",
            "reported_destination": {"kind": "known", "floor": 6}
          }
        ],
        "sources": [
          {
            "source": "rmf/site-main",
            "session": {"state": "active", "session": "session-alpha"},
            "transport": "discovery_observed",
            "payload": "$payload",
            "observations": [
              {"car": "lift-A", "role": "operational", "availability": "resolved"},
              {"car": "lift-B", "role": "operational", "availability": "$availability"}
            ]
          }
        ],
        "recent_diagnostics": []
      }
      ''';

      // Case E: lift-B recovers with fresh Operational state.
      const String recoveredBothJson = '''
      {
        "api_version": "experimental/v0",
        "revision": 18,
        "lifecycle": "ready_read_only",
        "readiness": "read_only",
        "operational_cars": [
          {
            "id": "lift-A",
            "location": {"kind": "floor", "floor": 3},
            "motion": {"state": "stopped"},
            "door": "closed",
            "operating_mode": "normal",
            "reported_destination": {"kind": "known", "floor": 6}
          },
          {
            "id": "lift-B",
            "location": {"kind": "floor", "floor": 5},
            "motion": {"state": "stopped"},
            "door": "open",
            "operating_mode": "normal",
            "reported_destination": {"kind": "none"}
          }
        ],
        "sources": [
          {
            "source": "rmf/site-main",
            "session": {"state": "active", "session": "session-alpha"},
            "transport": "discovery_observed",
            "payload": "active",
            "observations": [
              {"car": "lift-A", "role": "operational", "availability": "resolved"},
              {"car": "lift-B", "role": "operational", "availability": "resolved"}
            ]
          },
          {
            "source": "bacnet/wing-west",
            "session": {"state": "active", "session": "session-beta"},
            "transport": "discovery_observed",
            "payload": "active",
            "observations": [
              {"car": "lift-B", "role": "auxiliary", "availability": "resolved"},
              {"car": "lift-C", "role": "auxiliary", "availability": "resolved"}
            ]
          }
        ],
        "recent_diagnostics": []
      }
      ''';

      final _WidgetTestTransport transport = _WidgetTestTransport();
      final ClientController controller = ClientController(
        transport: transport,
        initialUrlInput: 'http://127.0.0.1:8080',
      );
      addTearDown(() {
        controller.dispose();
        transport.dispose();
      });

      await tester.pumpWidget(
        LiftAssistantClientApp(
          controller: controller,
          initialHostUrl: 'http://127.0.0.1:8080',
          autoConnect: true,
        ),
      );
      await tester.pumpAndSettle();

      // Case A: 2 resolved operational cars, 0 unavailable duplicates.
      expect(find.byType(OperationalCarCard), findsNWidgets(2));
      expect(find.byType(UnavailableOperationalCarCard), findsNothing);
      expect(
        find.byKey(const Key('operational-car-card-lift-A')),
        findsOneWidget,
      );
      expect(
        find.byKey(const Key('operational-car-card-lift-B')),
        findsOneWidget,
      );
      expect(find.text('2.25 F'), findsOneWidget);
      expect(find.text('Inspection'), findsOneWidget);

      // Case B + C + G: lift-B loses Operational state while Auxiliary is resolved,
      // and lift-C is Auxiliary-only.
      transport.sseController.add(partialRevokedWithAuxJson);
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);

      expect(find.byType(OperationalCarCard), findsOneWidget);
      expect(find.byType(UnavailableOperationalCarCard), findsOneWidget);
      expect(
        find.byKey(const Key('operational-car-card-lift-A')),
        findsOneWidget,
      );
      expect(
        find.byKey(const Key('operational-car-card-lift-B')),
        findsNothing,
      );
      expect(
        find.byKey(const Key('unavailable-car-card-lift-B')),
        findsOneWidget,
      );
      expect(
        find.byKey(const Key('unavailable-car-card-lift-A')),
        findsNothing,
      );
      expect(
        find.byKey(const Key('unavailable-car-card-lift-C')),
        findsNothing,
      );

      // Desktop section header distinguishes total configured vs resolved/unavailable.
      expect(find.text('1 resolved · 1 unavailable'), findsOneWidget);

      // Side-by-side resolved + unavailable cards share the same M3 grid row
      // height; the unavailable card retains its explicit semantic badge.
      final Rect resolvedRect = tester.getRect(find.byType(OperationalCarCard));
      final Rect unavailableRect = tester.getRect(
        find.byType(UnavailableOperationalCarCard),
      );
      expect(resolvedRect.top, equals(unavailableRect.top));
      expect(resolvedRect.height, equals(unavailableRect.height));
      expect(
        find.descendant(
          of: find.byType(UnavailableOperationalCarCard),
          matching: find.widgetWithText(SemanticBadge, 'Revoked'),
        ),
        findsOneWidget,
      );

      // Unavailable lift-B shows honest observation-unavailable state + Operational source context only.
      final Finder unavailableCard = find.byType(UnavailableOperationalCarCard);
      expect(
        find.descendant(of: unavailableCard, matching: find.text('lift-B')),
        findsOneWidget,
      );
      expect(
        find.descendant(
          of: unavailableCard,
          matching: find.text('Current observation unavailable'),
        ),
        findsOneWidget,
      );
      expect(
        find.descendant(of: unavailableCard, matching: find.text('Revoked')),
        findsOneWidget,
      );
      expect(
        find.descendant(
          of: unavailableCard,
          matching: find.text(
            'Configured Operational binding; observation revoked.',
          ),
        ),
        findsOneWidget,
      );
      expect(
        find.descendant(
          of: unavailableCard,
          matching: find.text('rmf/site-main'),
        ),
        findsOneWidget,
      );
      expect(
        find.descendant(of: unavailableCard, matching: find.text('revoked')),
        findsOneWidget,
      );
      expect(
        find.descendant(
          of: unavailableCard,
          matching: find.text('Source payload'),
        ),
        findsOneWidget,
      );
      // Auxiliary source must NOT leak into the unavailable car card.
      expect(
        find.descendant(
          of: unavailableCard,
          matching: find.text('bacnet/wing-west'),
        ),
        findsNothing,
      );

      // Stale lift-B canonical telemetry must NOT be retained anywhere.
      expect(find.text('2.25 F'), findsNothing);
      expect(find.text('Nearest Floor 2 (+0.25)'), findsNothing);
      expect(find.text('Moving Up'), findsNothing);
      expect(find.text('Opening'), findsNothing);
      expect(find.text('Inspection'), findsNothing);
      expect(find.text('Unknown'), findsNothing);

      // Never mislabels unavailable observation as OutOfService, Fault, timeout, or disconnected.
      expect(find.text('OutOfService'), findsNothing);
      expect(find.text('Fault'), findsNothing);
      expect(find.textContaining('timeout'), findsNothing);
      expect(find.textContaining('disconnected'), findsNothing);

      // Existing Sources view still surfaces Auxiliary bindings for lift-B and lift-C.
      expect(
        find.descendant(
          of: find.byType(SourceHealthCard),
          matching: find.text('lift-C'),
        ),
        findsOneWidget,
      );
      expect(
        find.descendant(
          of: find.byType(SourceHealthCard),
          matching: find.text('Auxiliary'),
        ),
        findsNWidgets(2),
      );

      // Case D (+ defensive resolved-but-omitted case): never-received / partial / unresolved
      // Operational states are shown honestly without claiming disconnect.
      for (final (int rev, String wire, String badge, String detail)
          in <(int, String, String, String)>[
            (
              14,
              'awaiting',
              'Awaiting',
              'Configured Operational binding; awaiting observation state.',
            ),
            (
              15,
              'incomplete',
              'Incomplete',
              'Configured Operational binding; partial evidence only.',
            ),
            (
              16,
              'unresolved',
              'Unresolved',
              'Configured Operational binding; observation unresolved.',
            ),
            (
              17,
              'resolved',
              'Unavailable',
              'Configured Operational binding; no current operational state.',
            ),
          ]) {
        transport.sseController.add(
          partialAvailabilityJson(rev, wire, payload: 'awaiting'),
        );
        await tester.pumpAndSettle();
        expect(find.byType(OperationalCarCard), findsOneWidget);
        expect(find.byType(UnavailableOperationalCarCard), findsOneWidget);
        expect(
          find.descendant(of: unavailableCard, matching: find.text(badge)),
          findsOneWidget,
        );
        expect(
          find.descendant(of: unavailableCard, matching: find.text(detail)),
          findsOneWidget,
        );
        expect(find.textContaining('disconnected'), findsNothing);
      }

      // Compact desktop split (940x720) keeps both M3 cards aligned and
      // responsive without prescribing the old tabular-footer implementation.
      tester.view.physicalSize = const Size(940, 720);
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
      final Rect compactResolvedRect = tester.getRect(
        find.byType(OperationalCarCard),
      );
      final Rect compactUnavailableRect = tester.getRect(unavailableCard);
      expect(compactResolvedRect.top, equals(compactUnavailableRect.top));
      expect(compactResolvedRect.height, equals(compactUnavailableRect.height));

      await tester.tap(find.byKey(const Key('topbar-theme-toggle')));
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);

      tester.view.physicalSize = const Size(1366, 900);
      await tester.pumpAndSettle();

      // Case E: lift-B recovers -> replaced cleanly by normal OperationalCarCard with no duplicate.
      transport.sseController.add(recoveredBothJson);
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);

      expect(find.byType(OperationalCarCard), findsNWidgets(2));
      expect(find.byType(UnavailableOperationalCarCard), findsNothing);
      expect(
        find.byKey(const Key('operational-car-card-lift-B')),
        findsOneWidget,
      );
      expect(find.text('5F'), findsOneWidget);
      expect(find.text('Open'), findsOneWidget);
      expect(find.textContaining('unavailable'), findsNothing);
    },
  );

  testWidgets(
    'handles narrow layout, 2.0x text scaling, all-unavailable empty UX, and snapshot/reconnect replacement without ghost cars',
    (WidgetTester tester) async {
      tester.view.devicePixelRatio = 1.0;
      tester.view.physicalSize = const Size(360, 780);
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      addTearDown(tester.platformDispatcher.clearTextScaleFactorTestValue);

      const String mixedLongIdsJson = '''
      {
        "api_version": "experimental/v0",
        "revision": 10,
        "lifecycle": "synchronizing",
        "readiness": "not_ready",
        "operational_cars": [
          {
            "id": "lift-A",
            "location": {"kind": "floor", "floor": 1},
            "motion": {"state": "stopped"},
            "door": "closed",
            "operating_mode": "normal",
            "reported_destination": {"kind": "none"}
          }
        ],
        "sources": [
          {
            "source": "rmf/site-main-east-tower-primary",
            "session": {"state": "inactive"},
            "transport": "subscriber_created",
            "payload": "stale",
            "observations": [
              {"car": "lift-A", "role": "operational", "availability": "resolved"},
              {"car": "building-north-tower-service-car-02", "role": "operational", "availability": "revoked"}
            ]
          }
        ],
        "recent_diagnostics": []
      }
      ''';

      // Case F: All Operational cars unavailable + Auxiliary-only car present.
      const String allUnavailableWithAuxJson = '''
      {
        "api_version": "experimental/v0",
        "revision": 11,
        "lifecycle": "synchronizing",
        "readiness": "not_ready",
        "operational_cars": [],
        "sources": [
          {
            "source": "rmf/site-main-east-tower-primary",
            "session": {"state": "active", "session": "s2"},
            "transport": "discovery_observed",
            "payload": "stale",
            "observations": [
              {"car": "lift-A", "role": "operational", "availability": "revoked"},
              {"car": "building-north-tower-service-car-02", "role": "operational", "availability": "awaiting"}
            ]
          },
          {
            "source": "bacnet/wing-west",
            "session": {"state": "active", "session": "s3"},
            "transport": "discovery_observed",
            "payload": "active",
            "observations": [
              {"car": "aux-only-car", "role": "auxiliary", "availability": "resolved"}
            ]
          }
        ],
        "recent_diagnostics": []
      }
      ''';

      // Case H: Host restart/reconnect replaces entire fleet (revision resets 11 -> 1).
      const String restartedDifferentFleetJson = '''
      {
        "api_version": "experimental/v0",
        "revision": 1,
        "lifecycle": "ready_read_only",
        "readiness": "read_only",
        "operational_cars": [
          {
            "id": "lift-Z",
            "location": {"kind": "floor", "floor": 8},
            "motion": {"state": "stopped"},
            "door": "closed",
            "operating_mode": "normal",
            "reported_destination": {"kind": "none"}
          }
        ],
        "sources": [
          {
            "source": "rmf/site-west",
            "session": {"state": "active", "session": "s-fresh"},
            "transport": "discovery_observed",
            "payload": "active",
            "observations": [
              {"car": "lift-Z", "role": "operational", "availability": "resolved"}
            ]
          }
        ],
        "recent_diagnostics": []
      }
      ''';

      final _WidgetTestTransport transport = _WidgetTestTransport()
        ..currentSnapshot = mixedLongIdsJson;
      final ClientController controller = ClientController(
        transport: transport,
        initialUrlInput: 'http://127.0.0.1:8080',
        retryBackoff: (_) => const Duration(seconds: 30),
      );
      addTearDown(() {
        controller.dispose();
        transport.dispose();
      });

      await tester.pumpWidget(
        LiftAssistantClientApp(
          controller: controller,
          initialHostUrl: 'http://127.0.0.1:8080',
          autoConnect: true,
        ),
      );
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);

      expect(find.byType(OperationalCarCard), findsOneWidget);
      expect(find.byType(UnavailableOperationalCarCard), findsOneWidget);
      expect(find.byKey(const Key('mobile-navigation-bar')), findsOneWidget);
      expect(
        find.descendant(
          of: find.byKey(const Key('mobile-navigation-bar')),
          matching: find.text('2'),
        ),
        findsOneWidget,
      );

      // Narrow 320x568 viewport at 2.0x text scaling renders without overflow.
      tester.view.physicalSize = const Size(320, 568);
      tester.platformDispatcher.textScaleFactorTestValue = 2.0;
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);

      // Case F at 320x568 + 2.0x text scaling also renders without overflow.
      transport.sseController.add(allUnavailableWithAuxJson);
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);

      // Restore mobile viewport and verify Sources tab still works cleanly.
      tester.platformDispatcher.clearTextScaleFactorTestValue();
      tester.view.physicalSize = const Size(360, 780);
      await tester.pumpAndSettle();

      await tester.tap(find.text('Sources'));
      await tester.pumpAndSettle();
      expect(find.text('rmf/site-main-east-tower-primary'), findsOneWidget);
      expect(find.text('aux-only-car'), findsOneWidget);
      await tester.tap(find.text('Cars'));
      await tester.pumpAndSettle();

      // Case F: all cars unavailable -> single summary panel without duplicate per-car cards,
      // and Auxiliary-only car is NOT listed in Cars.
      expect(
        find.byKey(const Key('empty-operational-cars-card')),
        findsOneWidget,
      );
      expect(find.byType(OperationalCarCard), findsNothing);
      expect(find.byType(UnavailableOperationalCarCard), findsNothing);
      expect(find.text('lift-A'), findsOneWidget);
      expect(find.text('building-north-tower-service-car-02'), findsOneWidget);
      expect(find.text('Revoked'), findsOneWidget);
      expect(find.text('Awaiting'), findsOneWidget);
      expect(
        find.text('Source: rmf/site-main-east-tower-primary'),
        findsNWidgets(2),
      );
      expect(find.textContaining('aux-only-car'), findsNothing);

      // Case H: disconnect + Host restart/reconnect replaces snapshot completely with zero ghost entries.
      transport.sseController.addError(
        const HostStreamDisconnectedException('Host restarted'),
      );
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 50));

      transport.currentSnapshot = restartedDifferentFleetJson;
      await tester.tap(find.byKey(const Key('banner-reconnect-button')));
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);

      expect(find.byType(OperationalCarCard), findsOneWidget);
      expect(find.byType(UnavailableOperationalCarCard), findsNothing);
      expect(
        find.byKey(const Key('operational-car-card-lift-Z')),
        findsOneWidget,
      );
      expect(
        find.byKey(const Key('empty-operational-cars-card')),
        findsNothing,
      );
      expect(find.text('lift-Z'), findsOneWidget);
      expect(find.text('8F'), findsOneWidget);
      expect(find.text('lift-A'), findsNothing);
      expect(find.text('building-north-tower-service-car-02'), findsNothing);
    },
  );
}

final class _FakeConditionalTransport implements HostTransport {
  _FakeConditionalTransport({required this.onFetch, required this.delegate});

  final Future<String> Function() onFetch;
  final _WidgetTestTransport delegate;

  @override
  Future<String> fetchSnapshotJson(HostEndpoint endpoint) => onFetch();

  @override
  Future<HostSseConnection> openSnapshotEvents(HostEndpoint endpoint) =>
      delegate.openSnapshotEvents(endpoint);

  @override
  void dispose() {}
}
