import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:liftassistant_client/src/api/host_url.dart';
import 'package:liftassistant_client/src/state/client_controller.dart';
import 'package:liftassistant_client/src/transport/host_transport.dart';
import 'package:liftassistant_client/src/ui/app.dart';
import 'package:liftassistant_client/src/ui/widgets/car_card.dart';

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
    'desktop split console renders cars, sources, and filterable diagnostics simultaneously without redundant tab chrome or write controls',
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

      // Location precision distinction: Floor ref vs Continuous position.
      expect(find.text('Floor ref'), findsOneWidget);
      expect(find.text('Floor 3'), findsOneWidget);
      expect(find.text('Continuous'), findsOneWidget);
      expect(find.text('2.25'), findsOneWidget);
      expect(find.text('Nearest Floor 2 (+0.25)'), findsOneWidget);

      // Source-reported destination is labeled concisely without conflation.
      expect(find.text('Floor 6'), findsOneWidget);
      expect(find.text('Unknown'), findsOneWidget);

      // Source Operational vs Auxiliary distinction & discovery!=payload callout.
      expect(find.text('Operational'), findsNWidgets(2));
      expect(find.text('Auxiliary · diag'), findsOneWidget);
      expect(
        find.textContaining('Discovery observed without active payload'),
        findsOneWidget,
      );

      // Full filterable diagnostics stream is immediately available on desktop right pane.
      expect(find.text('readiness.changed'), findsOneWidget);
      expect(find.text('mapping.rejected'), findsOneWidget);

      await tester.tap(find.byKey(const Key('diag-filter-layer-mapping')));
      await tester.pumpAndSettle();
      expect(find.text('mapping.rejected'), findsOneWidget);
      expect(find.text('readiness.changed'), findsNothing);

      // Reset filter, test role filter and search input.
      await tester.tap(find.byKey(const Key('diag-filter-all')));
      await tester.pumpAndSettle();
      expect(find.text('readiness.changed'), findsOneWidget);

      await tester.tap(find.byKey(const Key('diag-filter-role-auxiliary')));
      await tester.pumpAndSettle();
      expect(find.text('mapping.rejected'), findsOneWidget);
      expect(find.text('readiness.changed'), findsNothing);

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
    'mobile layout shows operational cars above the fold, switches 3 non-overlapping tabs cleanly, and surfaces disconnect + Host revision reset',
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

      // Mobile uses slim 3-tab strip (Cars / Sources / Diagnostics) and renders BOTH cars above the fold!
      expect(find.byKey(const Key('compact-section-strip')), findsOneWidget);
      expect(find.byType(OperationalCarCard), findsNWidgets(2));
      final Rect secondCarRect = tester.getRect(
        find.byType(OperationalCarCard).at(1),
      );
      expect(secondCarRect.bottom, lessThan(780));
      expect(tester.takeException(), isNull);

      // Visit Sources, Diagnostics, and back to Cars with zero layout overflow.
      await tester.tap(find.byKey(const Key('compact-tab-sources')));
      await tester.pumpAndSettle();
      expect(find.text('rmf/site-main'), findsOneWidget);
      expect(tester.takeException(), isNull);

      await tester.tap(find.byKey(const Key('compact-tab-diagnostics')));
      await tester.pumpAndSettle();
      expect(find.text('mapping.rejected'), findsOneWidget);
      expect(tester.takeException(), isNull);

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

      // Verify -0.00 / +-0.00 NEVER appear anywhere (neither main position nor signed delta!).
      expect(find.textContaining('-0.00'), findsNothing);
      expect(find.textContaining('+-0.00'), findsNothing);
      expect(find.text('0.00'), findsOneWidget);
      expect(find.text('Nearest Floor 2 (+0.00)'), findsOneWidget);
      expect(find.text('Nearest Floor 0 (+0.00)'), findsOneWidget);

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
