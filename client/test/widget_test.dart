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
    'desktop M3 supporting-pane layout balances Operational Cars and Observation Sources, and opens Pop-up Diagnostics Terminal on demand without write controls',
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

      // Single-row M3 AppBar displays Read-Only chip, Host status M3 chips, and Diagnostics Terminal button + badge.
      expect(find.byType(NavigationRail), findsNothing);
      expect(find.byKey(const Key('compact-section-strip')), findsNothing);
      expect(find.text('Read-Only'), findsOneWidget);
      expect(find.text('LIVE SSE'), findsOneWidget);
      expect(find.text('ReadyReadOnly'), findsOneWidget);
      expect(find.text('ReadOnly'), findsOneWidget);
      expect(find.text('Rev #12'), findsOneWidget);
      expect(
        find.byKey(const Key('topbar-diagnostics-button')),
        findsOneWidget,
      );
      expect(find.byKey(const Key('topbar-diagnostics-badge')), findsOneWidget);

      // Host Diagnostics is NOT permanently occupying the main desktop canvas before clicking the terminal button.
      expect(find.byKey(const Key('diagnostics-terminal-modal')), findsNothing);

      // Desktop Supporting-Pane layout: Operational Cars on the left primary pane, Observation Sources on the right supporting pane.
      final Rect carsHeaderRect = tester.getRect(
        find.widgetWithText(SectionHeader, 'Operational Cars'),
      );
      final Rect sourcesHeaderRect = tester.getRect(
        find.widgetWithText(SectionHeader, 'Observation Sources'),
      );
      expect(carsHeaderRect.top, equals(sourcesHeaderRect.top));
      expect(carsHeaderRect.left, lessThan(sourcesHeaderRect.left));

      // Side-by-side car cards in the primary pane share exact row height on 1366x900,
      // and trailing mode chips align flush-right within 16px card padding.
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

      // Source cards in the right supporting pane align their trailing bound chips flush-right within 16px padding.
      final Rect src0 = tester.getRect(find.byType(SourceHealthCard).at(0));
      final Rect src1 = tester.getRect(find.byType(SourceHealthCard).at(1));
      expect(src0.left, equals(src1.left));
      expect(src0.width, equals(src1.width));

      final Rect src0BoundRect = tester.getRect(
        find.descendant(
          of: find.byType(SourceHealthCard).at(0),
          matching: find.widgetWithText(SemanticBadge, '2 bound'),
        ),
      );
      final Rect src1BoundRect = tester.getRect(
        find.descendant(
          of: find.byType(SourceHealthCard).at(1),
          matching: find.widgetWithText(SemanticBadge, '1 bound'),
        ),
      );
      expect((src0.right - 16.0 - src0BoundRect.right).abs(), lessThan(2.0));
      expect((src1.right - 16.0 - src1BoundRect.right).abs(), lessThan(2.0));

      // Location precision distinction: Floor Reference vs Continuous position.
      expect(find.text('Floor Reference'), findsOneWidget);
      expect(find.text('3F'), findsOneWidget);
      expect(find.text('Continuous'), findsOneWidget);
      expect(find.text('2.25 F'), findsOneWidget);
      expect(find.text('Nearest Floor 2'), findsOneWidget);
      expect(find.text('+0.25'), findsOneWidget);

      // Source-reported destination is labeled clearly in the 3-column M3 footer.
      expect(find.text('Reported Destination'), findsNWidgets(2));
      expect(find.text('Floor 6'), findsOneWidget);
      expect(find.text('Unknown'), findsOneWidget);

      // Source Operational vs Auxiliary distinction, 3-column tonal health tiles, & discovery!=payload warning banner.
      expect(find.text('Session: session-alpha'), findsOneWidget);
      expect(find.text('Session: session-beta'), findsOneWidget);
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

      // Open Pop-up Diagnostics Terminal via top AppBar button.
      await tester.tap(find.byKey(const Key('topbar-diagnostics-button')));
      await tester.pumpAndSettle();
      expect(
        find.byKey(const Key('diagnostics-terminal-modal')),
        findsOneWidget,
      );
      expect(find.byType(DiagnosticsTerminalModalContent), findsOneWidget);
      expect(find.text('Host Diagnostics'), findsOneWidget);
      expect(
        find.textContaining('Machine-readable Host evidence only'),
        findsOneWidget,
      );
      expect(find.text('readiness.changed'), findsOneWidget);
      expect(find.text('mapping.rejected'), findsOneWidget);

      // Structured M3 context chips inside diagnostic entry row (no raw key=value soup).
      final Finder modalFinder = find.byType(DiagnosticsTerminalModalContent);
      expect(
        find.descendant(
          of: modalFinder,
          matching: find.textContaining('Source: bacnet/wing-west'),
        ),
        findsOneWidget,
      );
      expect(
        find.descendant(
          of: modalFinder,
          matching: find.textContaining('Session: session-beta'),
        ),
        findsOneWidget,
      );
      expect(
        find.descendant(
          of: modalFinder,
          matching: find.textContaining('Car: lift-A'),
        ),
        findsOneWidget,
      );
      expect(
        find.descendant(
          of: modalFinder,
          matching: find.textContaining('Role: Auxiliary'),
        ),
        findsOneWidget,
      );
      expect(
        find.descendant(
          of: modalFinder,
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
      // When search matches zero entries, empty diagnostics card stays compact rather than stretching to modal height.
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

      // Close Pop-up Diagnostics Terminal.
      await tester.tap(find.byKey(const Key('diagnostics-modal-close')));
      await tester.pumpAndSettle();
      expect(find.byKey(const Key('diagnostics-terminal-modal')), findsNothing);

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
    'mobile layout uses 2-tab M3 strip (Cars / Sources), opens Pop-up Diagnostics Terminal bottom sheet, preserves filters, and surfaces disconnect + Host revision reset',
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

      // Mobile uses 2-tab M3 strip (Cars / Sources) and renders both cars cleanly.
      expect(find.byKey(const Key('compact-section-strip')), findsOneWidget);
      expect(find.byKey(const Key('compact-tab-cars')), findsOneWidget);
      expect(find.byKey(const Key('compact-tab-sources')), findsOneWidget);
      expect(find.byType(OperationalCarCard), findsNWidgets(2));
      expect(tester.takeException(), isNull);

      // Switch to Sources tab on mobile.
      await tester.tap(find.byKey(const Key('compact-tab-sources')));
      await tester.pumpAndSettle();
      expect(find.text('rmf/site-main'), findsOneWidget);
      expect(tester.takeException(), isNull);

      // Open Pop-up Diagnostics Terminal bottom sheet from top AppBar button, apply filter + search, close & reopen.
      await tester.tap(find.byKey(const Key('topbar-diagnostics-button')));
      await tester.pumpAndSettle();
      expect(
        find.byKey(const Key('diagnostics-terminal-modal')),
        findsOneWidget,
      );
      expect(find.text('mapping.rejected'), findsOneWidget);
      expect(find.text('readiness.changed'), findsOneWidget);

      await tester.tap(find.byKey(const Key('diag-filter-layer-mapping')));
      await tester.enterText(
        find.byKey(const Key('diagnostics-search-input')),
        'LiftObj',
      );
      await tester.pumpAndSettle();
      expect(find.text('readiness.changed'), findsNothing);

      // Close modal, switch back to Cars tab, resize to desktop, and reopen Diagnostics Terminal: filter & query remain intact!
      await tester.tap(find.byKey(const Key('diagnostics-modal-close')));
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(const Key('compact-tab-cars')));
      await tester.pumpAndSettle();

      tester.view.physicalSize = const Size(1280, 800);
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(const Key('topbar-diagnostics-button')));
      await tester.pumpAndSettle();
      expect(find.text('mapping.rejected'), findsOneWidget);
      expect(find.text('readiness.changed'), findsNothing);
      await tester.tap(find.byKey(const Key('diagnostics-modal-close')));
      await tester.pumpAndSettle();

      tester.view.physicalSize = const Size(360, 780);
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

      // Simulate Host restart (revision resets 12 -> 1, empty operational_cars) and tap Reconnect.
      transport.currentSnapshot = _restartedSynchronizingJson;
      await tester.tap(find.byKey(const Key('banner-reconnect-button')));
      await tester.pumpAndSettle();

      expect(find.text('LIVE SSE'), findsOneWidget);
      expect(find.text('Synchronizing'), findsOneWidget);
      expect(find.text('NotReady'), findsOneWidget);
      expect(find.text('Rev #1'), findsOneWidget);
      expect(
        find.textContaining('New Host lifetime (#12 → #1)'),
        findsOneWidget,
      );
      expect(
        find.byKey(const Key('empty-operational-cars-card')),
        findsOneWidget,
      );
      // Pending Operational car binding from sources is surfaced via structured M3 chips!
      expect(find.text('Source: rmf/site-main'), findsOneWidget);
      expect(find.text('Payload: Awaiting'), findsOneWidget);
    },
  );

  testWidgets(
    'surfaces hard API/protocol errors alongside retained snapshots, normalizes -0.00 continuous positions, handles long CarIds and landscape viewports, and syncs inline URL form',
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
      expect(
        find.byKey(const Key('error-configure-host-button')),
        findsNothing,
      );

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

      // Verify long CarId does not overflow on 360x780 mobile portrait or 800x300 short landscape + modal.
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

      // Open Pop-up Diagnostics Terminal on 800x300 short landscape, filter, and switch Host endpoint while modal is open.
      await tester.tap(find.byKey(const Key('topbar-diagnostics-button')));
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
      await tester.enterText(
        find.byKey(const Key('diagnostics-search-input')),
        'unmatched-filter',
      );
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
      expect(find.text('mapping.rejected'), findsNothing);

      // Switching Host endpoint while Diagnostics Terminal is mounted resets filter outside build() without error.
      unawaited(syncController.connectToUrl('http://192.168.1.100:9090'));
      await tester.pump();
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
      expect(find.text('mapping.rejected'), findsOneWidget);

      await tester.tap(find.byKey(const Key('diagnostics-modal-close')));
      await tester.pumpAndSettle();

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

      // Check Sources tab and Pop-up Diagnostics Terminal at 2.0x text scale on 320x568 without overflow!
      await tester.tap(find.byKey(const Key('compact-tab-sources')));
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);

      await tester.tap(find.byKey(const Key('topbar-diagnostics-button')));
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
      expect(find.text('observation.updated.200'), findsOneWidget);
      expect(find.text('observation.updated.1'), findsNothing);
      await tester.tap(find.byKey(const Key('diagnostics-modal-close')));
      await tester.pumpAndSettle();

      // Desktop supporting-pane layout at 2.0x text scale AND at 1.0x virtualization check.
      tester.view.physicalSize = const Size(1366, 900);
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);

      tester.platformDispatcher.clearTextScaleFactorTestValue();
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
      expect(
        tester.widgetList(find.byType(OperationalCarCard)).length,
        lessThan(30),
      );

      await tester.tap(find.byKey(const Key('topbar-diagnostics-button')));
      await tester.pumpAndSettle();
      expect(find.text('observation.updated.200'), findsOneWidget);
      expect(find.text('observation.updated.1'), findsNothing);
      await tester.tap(find.byKey(const Key('diagnostics-modal-close')));
      await tester.pumpAndSettle();
    },
  );

  testWidgets(
    'balances single-car and single-source cards side-by-side in desktop Supporting-Pane layout',
    (WidgetTester tester) async {
      tester.view.devicePixelRatio = 1.0;
      tester.view.physicalSize = const Size(1366, 900);
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);

      const String singleCarFixtureJson = '''
      {
        "api_version": "experimental/v0",
        "revision": 2,
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
        "recent_diagnostics": []
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

      // In desktop M3 Supporting-Pane layout, 1 OperationalCarCard sits in the left primary pane
      // and 1 SourceHealthCard sits side-by-side at the same top Y in the right supporting pane!
      final Rect singleCarRect = tester.getRect(
        find.byType(OperationalCarCard),
      );
      final Rect singleSrcRect = tester.getRect(find.byType(SourceHealthCard));
      expect((singleCarRect.top - singleSrcRect.top).abs(), lessThan(2.0));
      expect(singleCarRect.right, lessThan(singleSrcRect.left));
      expect(singleCarRect.width, greaterThan(singleSrcRect.width));
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
