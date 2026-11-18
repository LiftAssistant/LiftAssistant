import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:liftassistant_client/src/api/host_url.dart';
import 'package:liftassistant_client/src/state/client_controller.dart';
import 'package:liftassistant_client/src/transport/host_transport.dart';
import 'package:liftassistant_client/src/ui/app.dart';
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
      expect(find.text('2 Cars · 2 Sources'), findsOneWidget);
      expect(find.text('2 Events'), findsOneWidget);

      // Verify zero duplicate snake_case wire tags in the UI.
      expect(find.text('ready_read_only'), findsNothing);
      expect(find.text('read_only'), findsNothing);
      expect(find.text('live-snapshot'), findsNothing);

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

      // Laptop / tiled desktop widths (1040x800 and 940x720) render without overflow.
      for (final Size compactDesktop in <Size>[
        const Size(1040, 800),
        const Size(940, 720),
      ]) {
        tester.view.physicalSize = compactDesktop;
        await tester.pumpAndSettle();
        expect(tester.takeException(), isNull);
      }

      tester.view.physicalSize = const Size(1366, 900);
      await tester.pumpAndSettle();

      // Click 'Inspect all 2 diagnostics' in Overview preview to switch to full Diagnostics workbench section.
      await tester.tap(
        find.byKey(const Key('overview-inspect-all-diagnostics-button')),
      );
      await tester.pumpAndSettle();
      expect(find.text('Host Diagnostics'), findsOneWidget);
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

      // Navigate to dedicated Cars and Sources sections via left NavigationRail.
      await tester.tap(
        find.descendant(
          of: find.byType(NavigationRail),
          matching: find.text('Cars'),
        ),
      );
      await tester.pumpAndSettle();
      expect(find.byKey(const Key('cars-section-scroll')), findsOneWidget);
      expect(find.byType(OperationalCarCard), findsNWidgets(2));

      await tester.tap(
        find.descendant(
          of: find.byType(NavigationRail),
          matching: find.text('Sources'),
        ),
      );
      await tester.pumpAndSettle();
      expect(find.byKey(const Key('sources-section-scroll')), findsOneWidget);
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
      // 1 column of the 2-column grid rather than stretching across the full 1230+px workspace.
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
