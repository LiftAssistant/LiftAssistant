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
  StreamController<String> sseController = StreamController<String>();

  @override
  Future<String> fetchSnapshotJson(HostEndpoint endpoint) async {
    return currentSnapshot;
  }

  @override
  Future<HostSseConnection> openSnapshotEvents(HostEndpoint endpoint) async {
    sseController = StreamController<String>();
    final StreamController<String> local = sseController;
    return HostSseConnection(
      snapshots: local.stream,
      onClose: () async {
        if (!local.isClosed) {
          if (local.hasListener) {
            await local.close();
          } else {
            unawaited(local.close());
          }
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
    'desktop split console renders equal-height cars/sources, flush-right headers, and filterable diagnostics without redundant tab chrome or write controls',
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
      expect(onboardingFormRect.top, greaterThan(120.0));
      expect(onboardingFormRect.center.dy, lessThan(430.0));

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

      // Unified desktop split console: zero redundant NavigationRail / Overview tab.
      expect(find.byType(NavigationRail), findsNothing);
      expect(find.byKey(const Key('compact-section-strip')), findsNothing);
      expect(find.text('LIVE SSE'), findsOneWidget);
      expect(find.text('ReadyReadOnly'), findsOneWidget);
      expect(find.text('ReadOnly'), findsOneWidget);
      expect(find.text('#12'), findsOneWidget);

      // SectionHeader trailingNote aligns flush with the right edge of Host Diagnostics header.
      final Rect diagHeaderRect = tester.getRect(
        find.widgetWithText(SectionHeader, 'Host Diagnostics'),
      );
      final Rect diagNoteRect = tester.getRect(
        find.text('Host evidence · non-fault'),
      );
      expect((diagHeaderRect.right - diagNoteRect.right).abs(), lessThan(2.0));

      // Side-by-side car cards and side-by-side source cards share exact row height on 1366x900,
      // AND trailing header readouts (mode badge & bound count) align flush-right within 12px card padding.
      final Rect car0 = tester.getRect(find.byType(OperationalCarCard).at(0));
      final Rect car1 = tester.getRect(find.byType(OperationalCarCard).at(1));
      expect(car0.top, equals(car1.top));
      expect(car0.height, equals(car1.height));

      final Rect car0ModeRect = tester.getRect(
        find.descendant(
          of: find.byType(OperationalCarCard).at(0),
          matching: find.text('Normal'),
        ),
      );
      final Rect car1ModeRect = tester.getRect(
        find.descendant(
          of: find.byType(OperationalCarCard).at(1),
          matching: find.widgetWithText(SemanticBadge, 'Inspection'),
        ),
      );
      expect((car0.right - 12.0 - car0ModeRect.right).abs(), lessThan(2.0));
      expect((car1.right - 12.0 - car1ModeRect.right).abs(), lessThan(2.0));

      final Rect src0 = tester.getRect(find.byType(SourceHealthCard).at(0));
      final Rect src1 = tester.getRect(find.byType(SourceHealthCard).at(1));
      expect(src0.top, equals(src1.top));
      expect(src0.height, equals(src1.height));

      final Rect src0BoundRect = tester.getRect(
        find.descendant(
          of: find.byType(SourceHealthCard).at(0),
          matching: find.text('2 bound'),
        ),
      );
      final Rect src1BoundRect = tester.getRect(
        find.descendant(
          of: find.byType(SourceHealthCard).at(1),
          matching: find.text('1 bound'),
        ),
      );
      expect((src0.right - 12.0 - src0BoundRect.right).abs(), lessThan(2.0));
      expect((src1.right - 12.0 - src1BoundRect.right).abs(), lessThan(2.0));

      // DiagnosticsFilterToolbar: #seq button is flush-right, Role segmented control sits paired
      // on Row 1, and all 6 Layer pills sit on a single non-wrapping row (no orphan Auxiliary wrap).
      final Rect toolbarRect = tester.getRect(
        find.byType(DiagnosticsFilterToolbar),
      );
      final Rect sortButtonRect = tester.getRect(
        find.byKey(const Key('diagnostics-sort-button')),
      );
      expect((toolbarRect.right - sortButtonRect.right).abs(), lessThan(2.0));

      final Rect roleOpRect = tester.getRect(
        find.byKey(const Key('diag-filter-role-operational')),
      );
      final Rect roleAuxRect = tester.getRect(
        find.byKey(const Key('diag-filter-role-auxiliary')),
      );
      expect(roleOpRect.top, equals(roleAuxRect.top));

      final double allPillTop = tester
          .getRect(find.byKey(const Key('diag-filter-all')))
          .top;
      for (final String layerKey in <String>[
        'diag-filter-layer-transport',
        'diag-filter-layer-source',
        'diag-filter-layer-mapping',
        'diag-filter-layer-observation',
        'diag-filter-layer-readiness',
      ]) {
        expect(
          tester.getRect(find.byKey(Key(layerKey))).top,
          equals(allPillTop),
        );
      }

      // Location precision distinction: Floor ref vs Continuous position.
      expect(find.text('Floor ref'), findsOneWidget);
      expect(find.text('Floor 3'), findsOneWidget);
      expect(find.text('Continuous'), findsOneWidget);
      expect(find.text('2.25'), findsOneWidget);
      expect(find.text('Nearest Floor 2 (+0.25)'), findsOneWidget);

      // Source-reported destination is labeled concisely without conflation.
      expect(find.text('Source dest'), findsNWidgets(2));
      expect(find.text('Floor 6'), findsOneWidget);
      expect(find.text('Unknown'), findsOneWidget);

      // Source Operational vs Auxiliary distinction & discovery!=payload callout.
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
      expect(find.text('diagnostic only'), findsOneWidget);
      expect(
        find.textContaining('Discovery observed without active payload'),
        findsOneWidget,
      );

      // Even at 1040x800 laptop width, primary left stage keeps 2 equal-height car columns!
      tester.view.physicalSize = const Size(1040, 800);
      await tester.pumpAndSettle();
      final Rect laptopCar0 = tester.getRect(
        find.byType(OperationalCarCard).at(0),
      );
      final Rect laptopCar1 = tester.getRect(
        find.byType(OperationalCarCard).at(1),
      );
      expect(laptopCar0.top, equals(laptopCar1.top));
      expect(laptopCar0.height, equals(laptopCar1.height));

      tester.view.physicalSize = const Size(1366, 900);
      await tester.pumpAndSettle();

      // Full filterable diagnostics stream is immediately available on desktop right pane.
      expect(find.text('readiness.changed'), findsOneWidget);
      expect(find.text('mapping.rejected'), findsOneWidget);

      await tester.tap(find.byKey(const Key('diag-filter-layer-mapping')));
      await tester.pumpAndSettle();
      expect(find.text('mapping.rejected'), findsOneWidget);
      expect(find.text('readiness.changed'), findsNothing);

      // Reset filter, test role filter and search input + clear button.
      await tester.tap(find.byKey(const Key('diag-filter-all')));
      await tester.pumpAndSettle();
      expect(find.text('readiness.changed'), findsOneWidget);

      await tester.tap(find.byKey(const Key('diag-filter-role-auxiliary')));
      await tester.pumpAndSettle();
      expect(find.text('mapping.rejected'), findsOneWidget);
      expect(find.text('readiness.changed'), findsNothing);

      await tester.tap(find.byKey(const Key('diag-filter-all')));
      await tester.enterText(
        find.byKey(const Key('diagnostics-search-input')),
        'LiftObj',
      );
      await tester.pumpAndSettle();
      expect(find.text('mapping.rejected'), findsOneWidget);
      expect(find.text('readiness.changed'), findsNothing);
      await tester.tap(find.byKey(const Key('diagnostics-search-clear')));
      await tester.pumpAndSettle();
      expect(find.text('readiness.changed'), findsOneWidget);

      // Open and close on-demand semantic reference modal.
      await tester.tap(find.byKey(const Key('topbar-semantics-button')));
      await tester.pumpAndSettle();
      expect(find.text('experimental/v0 Semantic Reference'), findsOneWidget);
      await tester.tap(find.byIcon(Icons.close));
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
    'mobile layout shows operational cars above the fold, preserves diagnostic filters across tabs/resizes, and surfaces disconnect + Host revision reset',
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

      // Mobile uses slim 3-tab strip (Cars / Sources / Diagnostics) and renders BOTH cars above the fold
      // with flush-right mode readouts!
      expect(find.byKey(const Key('compact-section-strip')), findsOneWidget);
      expect(find.byType(OperationalCarCard), findsNWidgets(2));
      final Rect mobileCar0 = tester.getRect(
        find.byType(OperationalCarCard).at(0),
      );
      final Rect mobileCar0Mode = tester.getRect(
        find.descendant(
          of: find.byType(OperationalCarCard).at(0),
          matching: find.text('Normal'),
        ),
      );
      expect(
        (mobileCar0.right - 12.0 - mobileCar0Mode.right).abs(),
        lessThan(2.0),
      );
      final Rect secondCarRect = tester.getRect(
        find.byType(OperationalCarCard).at(1),
      );
      expect(secondCarRect.bottom, lessThan(780));
      expect(tester.takeException(), isNull);

      // Visit Sources, then Diagnostics, apply filter + search, switch tabs AND resize across desktop/mobile!
      await tester.tap(find.byKey(const Key('compact-tab-sources')));
      await tester.pumpAndSettle();
      expect(find.text('rmf/site-main'), findsOneWidget);
      final Rect mobileSrc0 = tester.getRect(
        find.byType(SourceHealthCard).at(0),
      );
      final Rect mobileSrc0Bound = tester.getRect(
        find.descendant(
          of: find.byType(SourceHealthCard).at(0),
          matching: find.text('2 bound'),
        ),
      );
      expect(
        (mobileSrc0.right - 12.0 - mobileSrc0Bound.right).abs(),
        lessThan(2.0),
      );
      expect(tester.takeException(), isNull);

      await tester.tap(find.byKey(const Key('compact-tab-diagnostics')));
      await tester.pumpAndSettle();
      expect(find.text('mapping.rejected'), findsOneWidget);
      expect(find.text('readiness.changed'), findsOneWidget);

      // On mobile Diagnostics, #seq button is flush-right and Role buttons stay paired together on their own row.
      final Rect mobileToolbarRect = tester.getRect(
        find.byType(DiagnosticsFilterToolbar),
      );
      final Rect mobileSortRect = tester.getRect(
        find.byKey(const Key('diagnostics-sort-button')),
      );
      expect(
        (mobileToolbarRect.right - mobileSortRect.right).abs(),
        lessThan(2.0),
      );
      final Rect mobileRoleOp = tester.getRect(
        find.byKey(const Key('diag-filter-role-operational')),
      );
      final Rect mobileRoleAux = tester.getRect(
        find.byKey(const Key('diag-filter-role-auxiliary')),
      );
      expect(mobileRoleOp.top, equals(mobileRoleAux.top));

      await tester.tap(find.byKey(const Key('diag-filter-layer-mapping')));
      await tester.enterText(
        find.byKey(const Key('diagnostics-search-input')),
        'LiftObj',
      );
      await tester.pumpAndSettle();
      expect(find.text('readiness.changed'), findsNothing);

      // Switch away to Cars and back to Diagnostics: filter & query remain intact!
      await tester.tap(find.byKey(const Key('compact-tab-cars')));
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(const Key('compact-tab-diagnostics')));
      await tester.pumpAndSettle();
      expect(find.text('mapping.rejected'), findsOneWidget);
      expect(find.text('readiness.changed'), findsNothing);

      // Resize across 920x460 split-desktop breakpoint and back: filter & query remain intact!
      tester.view.physicalSize = const Size(1280, 800);
      await tester.pumpAndSettle();
      expect(find.text('mapping.rejected'), findsOneWidget);
      expect(find.text('readiness.changed'), findsNothing);

      tester.view.physicalSize = const Size(360, 780);
      await tester.pumpAndSettle();
      expect(find.text('mapping.rejected'), findsOneWidget);
      expect(find.text('readiness.changed'), findsNothing);

      await tester.tap(find.byKey(const Key('compact-tab-cars')));
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
      expect(find.text('#1'), findsOneWidget);
      expect(
        find.textContaining('New Host lifetime (#12 → #1)'),
        findsOneWidget,
      );
      expect(
        find.byKey(const Key('empty-operational-cars-card')),
        findsOneWidget,
      );
      // Pending Operational car binding from sources is surfaced directly in empty state!
      expect(
        find.textContaining(
          'lift-A · awaiting (rmf/site-main, payload:awaiting)',
        ),
        findsOneWidget,
      );
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

      // Now bring Host online and connect.
      failConnect = false;
      await tester.tap(find.byKey(const Key('error-retry-button')));
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);

      // Verify -0.00 / +-0.00 NEVER appear anywhere, and +0.85 outlier delta renders accurately.
      expect(find.textContaining('-0.00'), findsNothing);
      expect(find.textContaining('+-0.00'), findsNothing);
      expect(find.text('0.00'), findsOneWidget);
      expect(find.text('Nearest Floor 2 (+0.00)'), findsOneWidget);
      expect(find.text('Nearest Floor 0 (+0.00)'), findsOneWidget);
      expect(find.text('Nearest Floor 2 (+0.85)'), findsOneWidget);
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

      // Check Sources and Diagnostics tabs at 2.0x text scale on 320x568 without overflow!
      await tester.tap(find.byKey(const Key('compact-tab-sources')));
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);

      await tester.tap(find.byKey(const Key('compact-tab-diagnostics')));
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);

      // Scroll past the 2.0x-scaled filter toolbar on 320x568; oldest entry (#1) stays unmaterialized offscreen!
      await tester.drag(find.byType(CustomScrollView), const Offset(0, -320));
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
      expect(find.text('observation.updated.200'), findsOneWidget);
      expect(find.text('observation.updated.1'), findsNothing);

      // Desktop split console at 2.0x text scale AND at 1.0x virtualization check.
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
      expect(find.text('observation.updated.200'), findsOneWidget);
      expect(find.text('observation.updated.1'), findsNothing);
    },
  );

  testWidgets(
    'bounds single-car and single-source cards on wide desktop stages and aligns single source under column 0 when 2 cars resolve',
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

      const String twoCarSingleSourceJson = '''
      {
        "api_version": "experimental/v0",
        "revision": 3,
        "lifecycle": "ready_read_only",
        "readiness": "read_only",
        "operational_cars": [
          {
            "id": "lift-A",
            "location": {"kind": "floor", "floor": 1},
            "motion": {"state": "stopped"},
            "door": "closed",
            "operating_mode": "normal",
            "reported_destination": {"kind": "none"}
          },
          {
            "id": "lift-B",
            "location": {"kind": "floor", "floor": 3},
            "motion": {"state": "moving", "direction": "up"},
            "door": "closed",
            "operating_mode": "normal",
            "reported_destination": {"kind": "known", "floor": 5}
          }
        ],
        "sources": [
          {
            "source": "rmf/site-main",
            "session": {"state": "active", "session": "session-1"},
            "transport": "discovery_observed",
            "payload": "active",
            "observations": [
              {"car": "lift-A", "role": "operational", "availability": "resolved"},
              {"car": "lift-B", "role": "operational", "availability": "resolved"}
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

      // Single car and single source share identical bounded width (<= 540px) rather than stretching across ~873px.
      final Rect singleCarRect = tester.getRect(
        find.byType(OperationalCarCard),
      );
      final Rect singleSrcRect = tester.getRect(find.byType(SourceHealthCard));
      expect(singleCarRect.left, equals(singleSrcRect.left));
      expect(singleCarRect.width, equals(singleSrcRect.width));
      expect(singleCarRect.width, lessThanOrEqualTo(540.0));
      expect(singleCarRect.width, greaterThanOrEqualTo(500.0));

      final Rect fireBadgeRect = tester.getRect(
        find.widgetWithText(SemanticBadge, 'FireService'),
      );
      final Rect singleBoundRect = tester.getRect(find.text('1 bound'));
      expect(
        (singleCarRect.right - 12.0 - fireBadgeRect.right).abs(),
        lessThan(2.0),
      );
      expect(
        (singleSrcRect.right - 12.0 - singleBoundRect.right).abs(),
        lessThan(2.0),
      );

      // Push a 2-car, 1-source SSE snapshot (real Hotel RMF topology): Source 0 snaps to match Car 0's column width!
      transport.sseController.add(twoCarSingleSourceJson);
      await tester.pumpAndSettle();

      final Rect twoCar0Rect = tester.getRect(
        find.byType(OperationalCarCard).at(0),
      );
      final Rect pairedSrcRect = tester.getRect(find.byType(SourceHealthCard));
      expect((twoCar0Rect.right - pairedSrcRect.right).abs(), lessThan(1.0));

      // Ultra-wide 1920x1080 bounds multi-column rows cleanly without 700px+ card bloat.
      tester.view.physicalSize = const Size(1920, 1080);
      await tester.pumpAndSettle();
      final Rect ultraWideCar0 = tester.getRect(
        find.byType(OperationalCarCard).at(0),
      );
      expect(ultraWideCar0.width, lessThanOrEqualTo(500.0));
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
