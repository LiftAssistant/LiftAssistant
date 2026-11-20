import 'dart:async';
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:liftassistant_client/src/api/host_url.dart';
import 'package:liftassistant_client/src/state/client_controller.dart';
import 'package:liftassistant_client/src/transport/host_transport.dart';
import 'package:liftassistant_client/src/ui/app.dart';

const String _mixedFleetSnapshotJson = '''
{
  "api_version": "experimental/v0",
  "revision": 5,
  "lifecycle": "synchronizing",
  "readiness": "not_ready",
  "operational_cars": [
    {
      "id": "A",
      "location": {"kind": "continuous", "floor_position": 3.0, "nearest_floor": 3},
      "motion": {"state": "stopped"},
      "door": "closed",
      "operating_mode": "normal",
      "reported_destination": {"kind": "none"}
    },
    {
      "id": "B",
      "location": {"kind": "continuous", "floor_position": 1.0, "nearest_floor": 1},
      "motion": {"state": "stopped"},
      "door": "closed",
      "operating_mode": "normal",
      "reported_destination": {"kind": "none"}
    },
    {
      "id": "C",
      "location": {"kind": "floor", "floor": 2},
      "motion": {"state": "stopped"},
      "door": "closed",
      "operating_mode": "normal",
      "reported_destination": {"kind": "none"}
    },
    {
      "id": "D",
      "location": {"kind": "floor", "floor": 4},
      "motion": {"state": "stopped"},
      "door": "closed",
      "operating_mode": "normal",
      "reported_destination": {"kind": "none"}
    }
  ],
  "sources": [
    {
      "source": "simulator",
      "session": {"state": "active", "session": "sim-session-1"},
      "transport": "discovery_observed",
      "payload": "active",
      "observations": [
        {"car": "A", "role": "operational", "availability": "resolved"},
        {"car": "B", "role": "operational", "availability": "resolved"},
        {"car": "C", "role": "operational", "availability": "resolved"},
        {"car": "D", "role": "operational", "availability": "resolved"},
        {"car": "U", "role": "operational", "availability": "revoked"}
      ]
    },
    {
      "source": "simulator-aux",
      "session": {"state": "active", "session": "aux-session-1"},
      "transport": "discovery_observed",
      "payload": "active",
      "observations": [
        {"car": "U", "role": "auxiliary", "availability": "resolved"}
      ]
    }
  ],
  "recent_diagnostics": []
}
''';

const String _mixedCapabilitiesJson = '''
{
  "control_api_version": "dev-sim/v0",
  "development_only": true,
  "control_instance_id": "11111111111111111111111111111111",
  "ephemeral_process_memory": true,
  "authority_context": {
    "kind": "dev_simulator_process_local_grant",
    "multi_user_auth": false,
    "principal": "dev-sim-operator"
  },
  "cars": [
    {
      "car": "A",
      "serve_floor": {
        "support": "supported",
        "control_path_availability": "available",
        "control_path_reason": null,
        "authorization": "dev_process_local_grant",
        "executable_floors": [1, 2, 3, 4, 5, 6, 7, 8],
        "active_intent_id": null
      }
    },
    {
      "car": "B",
      "serve_floor": {
        "support": "supported",
        "control_path_availability": "available",
        "control_path_reason": null,
        "authorization": "dev_process_local_grant",
        "executable_floors": [1, 2, 3, 4, 5, 6],
        "active_intent_id": null
      }
    },
    {
      "car": "C",
      "serve_floor": {
        "support": "unsupported",
        "control_path_availability": "unavailable",
        "control_path_reason": "operation_unsupported",
        "authorization": "dev_process_local_grant",
        "executable_floors": [],
        "active_intent_id": null
      }
    },
    {
      "car": "D",
      "serve_floor": {
        "support": "supported",
        "control_path_availability": "unavailable",
        "control_path_reason": "controller_unavailable",
        "authorization": "dev_process_local_grant",
        "executable_floors": [1, 2, 3],
        "active_intent_id": null
      }
    },
    {
      "car": "U",
      "serve_floor": {
        "support": "supported",
        "control_path_availability": "available",
        "control_path_reason": null,
        "authorization": "dev_process_local_grant",
        "executable_floors": [1, 2, 3, 4],
        "active_intent_id": null
      }
    }
  ]
}
''';

final class _WidgetControlTransport
    implements HostTransport, DevControlTransport {
  String snapshotJson = _mixedFleetSnapshotJson;
  String capabilitiesJson = _mixedCapabilitiesJson;
  StreamController<String> sseController = StreamController<String>();

  int previewCalls = 0;
  int submitCalls = 0;
  int queryCalls = 0;
  final List<String> submittedKeys = <String>[];

  Future<ControlHttpResponse> Function()? onCapabilities;
  Future<ControlHttpResponse> Function(String body)? onPreview;
  Future<ControlHttpResponse> Function(String body)? onSubmit;
  Future<ControlHttpResponse> Function(String key, String? expectedInstance)?
  onQuery;

  @override
  Future<String> fetchSnapshotJson(HostEndpoint endpoint) async => snapshotJson;

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
  Future<ControlHttpResponse> fetchControlCapabilities(
    HostEndpoint endpoint,
  ) async {
    if (onCapabilities != null) {
      return onCapabilities!();
    }
    return ControlHttpResponse(
      statusCode: 200,
      body: capabilitiesJson,
      contentType: 'application/json',
    );
  }

  @override
  Future<ControlHttpResponse> postServeFloorPreview(
    HostEndpoint endpoint,
    String jsonBody,
  ) async {
    previewCalls++;
    if (onPreview != null) {
      return onPreview!(jsonBody);
    }
    final Map<String, Object?> req =
        jsonDecode(jsonBody) as Map<String, Object?>;
    return ControlHttpResponse(
      statusCode: 200,
      body: jsonEncode(<String, Object?>{
        'control_api_version': 'dev-sim/v0',
        'development_only': true,
        'control_instance_id': '11111111111111111111111111111111',
        'advisory': true,
        'operation': 'serve_floor',
        'car': req['car'],
        'floor': req['floor'],
        'support': 'supported',
        'control_path_availability': 'available',
        'control_path_reason': null,
        'authorization': 'dev_process_local_grant',
        'eligible': true,
        'blocker': null,
        'observed_car_available': true,
        'observed_floor_reference': 1,
        'observed_operating_mode': 'normal',
      }),
      contentType: 'application/json',
    );
  }

  @override
  Future<ControlHttpResponse> postServeFloorSubmit(
    HostEndpoint endpoint,
    String jsonBody,
  ) async {
    submitCalls++;
    final Map<String, Object?> req =
        jsonDecode(jsonBody) as Map<String, Object?>;
    submittedKeys.add(req['submission_key']! as String);
    return onSubmit!(jsonBody);
  }

  @override
  Future<ControlHttpResponse> fetchSubmissionStatus(
    HostEndpoint endpoint,
    String submissionKey, {
    String? expectedInstanceId,
  }) async {
    queryCalls++;
    return onQuery!(submissionKey, expectedInstanceId);
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
    'separates supported/unsupported/unavailable cars, restricts floors to ExecutionCapabilities, enforces preview->review->submit, and projects Accepted->InProgress->Completed',
    (WidgetTester tester) async {
      tester.view.devicePixelRatio = 1.0;
      tester.view.physicalSize = const Size(1440, 960);
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);

      final _WidgetControlTransport transport = _WidgetControlTransport();
      String projectedStatus = 'accepted';
      String reconciliation = 'pending';
      bool sawProgress = false;

      transport.onSubmit = (String rawBody) async {
        final Map<String, Object?> req =
            jsonDecode(rawBody) as Map<String, Object?>;
        return ControlHttpResponse(
          statusCode: 200,
          body: jsonEncode(<String, Object?>{
            'control_api_version': 'dev-sim/v0',
            'development_only': true,
            'control_instance_id': '11111111111111111111111111111111',
            'ephemeral_process_memory': true,
            'submission_key': req['submission_key'],
            'operation': 'serve_floor',
            'car': req['car'],
            'floor': req['floor'],
            'status': projectedStatus,
            'duplicate_submission': false,
            'intent_id': 1,
            'receipt': 'awaiting',
            'reconciliation': reconciliation,
            'diverged_floor': null,
            'saw_progress': sawProgress,
            'is_active_in_engine': true,
            'blocker': null,
            'knowledge_reason': null,
          }),
          contentType: 'application/json',
        );
      };
      transport.onQuery = (String key, String? _) async {
        return ControlHttpResponse(
          statusCode: 200,
          body: jsonEncode(<String, Object?>{
            'control_api_version': 'dev-sim/v0',
            'development_only': true,
            'control_instance_id': '11111111111111111111111111111111',
            'ephemeral_process_memory': true,
            'submission_key': key,
            'operation': 'serve_floor',
            'car': 'B',
            'floor': 5,
            'status': projectedStatus,
            'duplicate_submission': false,
            'intent_id': 1,
            'receipt': 'accepted',
            'reconciliation': reconciliation,
            'diverged_floor': null,
            'saw_progress': sawProgress,
            'is_active_in_engine': projectedStatus != 'completed',
            'blocker': null,
            'knowledge_reason': null,
          }),
          contentType: 'application/json',
        );
      };

      int keyCounter = 0;
      final ClientController controller = ClientController(
        transport: transport,
        submissionKeyGenerator: () => 'sub-widget-${++keyCounter}-key',
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

      // Top bar reflects dev-sim control surface truthfully.
      expect(find.text('dev-sim control'), findsOneWidget);

      // Car A and Car B expose Serve floor action; Car C shows unsupported; Car D shows unavailable.
      expect(find.byKey(const Key('car-serve-floor-button-A')), findsOneWidget);
      expect(find.byKey(const Key('car-serve-floor-button-B')), findsOneWidget);
      expect(
        find.text('ServeFloor: unsupported (operation_unsupported)'),
        findsOneWidget,
      );
      expect(
        find.text('ServeFloor: unavailable (controller_unavailable)'),
        findsOneWidget,
      );

      // Configured-but-unavailable car U MUST NOT expose Serve floor control.
      final Finder unavailableU = find.byKey(
        const Key('unavailable-car-card-U'),
      );
      expect(unavailableU, findsOneWidget);
      expect(find.byKey(const Key('car-serve-floor-button-U')), findsNothing);
      expect(
        find.descendant(of: unavailableU, matching: find.text('Serve floor')),
        findsNothing,
      );

      // Open ServeFloor workflow for Car B (ExecutionCapabilities floors 1..=6 only!).
      await tester.tap(find.byKey(const Key('car-serve-floor-button-B')));
      await tester.pumpAndSettle();

      // Floor selector comes strictly from ExecutionCapabilities (1F..6F; NO 7F or 8F!).
      expect(find.byKey(const Key('serve-floor-choice-1')), findsOneWidget);
      expect(find.byKey(const Key('serve-floor-choice-6')), findsOneWidget);
      expect(find.byKey(const Key('serve-floor-choice-7')), findsNothing);
      expect(find.byKey(const Key('serve-floor-choice-8')), findsNothing);

      // Tap 5F -> triggers advisory preview ONLY, NO immediate side effect!
      await tester.tap(find.byKey(const Key('serve-floor-choice-5')));
      await tester.pumpAndSettle();
      expect(transport.previewCalls, 1);
      expect(transport.submitCalls, 0);

      // Transaction review binds explicit Car, Target floor, Operation, Correlation key.
      expect(find.text('Transaction Review'), findsOneWidget);
      expect(find.text('Eligible (advisory)'), findsOneWidget);
      expect(find.text('sub-widget-1-key'), findsOneWidget);
      expect(find.text('Submit ServeFloor (B → 5F)'), findsOneWidget);

      // Submit -> Accepted (distinct from Completed!).
      await tester.tap(find.byKey(const Key('serve-floor-submit-button')));
      await tester.pumpAndSettle();
      expect(transport.submitCalls, 1);
      expect(find.text('Accepted'), findsWidgets);
      expect(
        find.textContaining('does not confirm physical arrival or completion'),
        findsWidgets,
      );

      // SSE update triggers read-only GET -> In progress.
      projectedStatus = 'in_progress';
      sawProgress = true;
      transport.sseController.add(
        _mixedFleetSnapshotJson.replaceFirst('"revision": 5', '"revision": 6'),
      );
      await tester.pumpAndSettle();
      expect(transport.submitCalls, 1);
      expect(find.text('In progress'), findsWidgets);

      // SSE update after arrival triggers read-only GET -> Completed.
      projectedStatus = 'completed';
      reconciliation = 'satisfied';
      transport.sseController.add(
        _mixedFleetSnapshotJson.replaceFirst('"revision": 5', '"revision": 7'),
      );
      await tester.pumpAndSettle();
      expect(transport.submitCalls, 1);
      expect(find.text('Completed'), findsWidgets);
      expect(find.textContaining('reconciliation: satisfied'), findsWidgets);
    },
  );

  testWidgets(
    'handles blocked preview, server rejection with fresh new-attempt key, response-loss Unknown recovery, and narrow 320x568 + 2.0x text scaling without overflow',
    (WidgetTester tester) async {
      tester.view.devicePixelRatio = 1.0;
      tester.view.physicalSize = const Size(320, 568);
      tester.platformDispatcher.textScaleFactorTestValue = 2.0;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      addTearDown(tester.platformDispatcher.clearTextScaleFactorTestValue);

      final _WidgetControlTransport transport = _WidgetControlTransport();
      bool blockPreviewFloor4 = true;
      bool rejectFirstSubmit = true;

      transport.onPreview = (String rawBody) async {
        final Map<String, Object?> req =
            jsonDecode(rawBody) as Map<String, Object?>;
        final int floor = req['floor']! as int;
        final bool blocked = blockPreviewFloor4 && floor == 4;
        return ControlHttpResponse(
          statusCode: 200,
          body: jsonEncode(<String, Object?>{
            'control_api_version': 'dev-sim/v0',
            'development_only': true,
            'control_instance_id': '11111111111111111111111111111111',
            'advisory': true,
            'operation': 'serve_floor',
            'car': req['car'],
            'floor': floor,
            'support': 'supported',
            'control_path_availability': 'available',
            'control_path_reason': null,
            'authorization': 'dev_process_local_grant',
            'eligible': !blocked,
            'blocker': blocked
                ? <String, Object?>{
                    'code': 'mode_not_normal',
                    'detail': 'car operating mode is OutOfService, not Normal',
                    'blocking_intent_id': null,
                    'observed_mode': 'out_of_service',
                  }
                : null,
            'observed_car_available': true,
            'observed_floor_reference': 3,
            'observed_operating_mode': blocked ? 'out_of_service' : 'normal',
          }),
          contentType: 'application/json',
        );
      };

      transport.onSubmit = (String rawBody) async {
        final Map<String, Object?> req =
            jsonDecode(rawBody) as Map<String, Object?>;
        if (rejectFirstSubmit) {
          rejectFirstSubmit = false;
          return ControlHttpResponse(
            statusCode: 200,
            body: jsonEncode(<String, Object?>{
              'control_api_version': 'dev-sim/v0',
              'development_only': true,
              'control_instance_id': '11111111111111111111111111111111',
              'ephemeral_process_memory': true,
              'submission_key': req['submission_key'],
              'operation': 'serve_floor',
              'car': req['car'],
              'floor': req['floor'],
              'status': 'rejected',
              'duplicate_submission': false,
              'intent_id': null,
              'receipt': null,
              'reconciliation': null,
              'diverged_floor': null,
              'saw_progress': false,
              'is_active_in_engine': false,
              'blocker': <String, Object?>{
                'code': 'active_intent',
                'detail': 'another controlled intent (#9) is already active',
                'blocking_intent_id': 9,
                'observed_mode': null,
              },
              'knowledge_reason': null,
            }),
            contentType: 'application/json',
          );
        }
        // Second submit loses response!
        throw const HostUnreachableException('Dropped response');
      };

      transport.onQuery = (String key, String? _) async {
        return ControlHttpResponse(
          statusCode: 200,
          body: jsonEncode(<String, Object?>{
            'control_api_version': 'dev-sim/v0',
            'development_only': true,
            'control_instance_id': '11111111111111111111111111111111',
            'ephemeral_process_memory': true,
            'submission_key': key,
            'operation': 'serve_floor',
            'car': 'A',
            'floor': 5,
            'status': 'completed',
            'duplicate_submission': false,
            'intent_id': 10,
            'receipt': 'accepted',
            'reconciliation': 'satisfied',
            'diverged_floor': null,
            'saw_progress': true,
            'is_active_in_engine': false,
            'blocker': null,
            'knowledge_reason': null,
          }),
          contentType: 'application/json',
        );
      };

      int seq = 0;
      final ClientController controller = ClientController(
        transport: transport,
        retryBackoff: (_) => const Duration(milliseconds: 20),
        submissionKeyGenerator: () => 'sub-narrow-${++seq}-key',
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

      Future<void> scrollAndTap(Finder finder) async {
        await tester.ensureVisible(finder);
        await tester.pumpAndSettle();
        await tester.tap(finder);
        await tester.pumpAndSettle();
      }

      // Open ServeFloor bottom sheet on narrow 320x568 + 2.0x text scale.
      await scrollAndTap(find.byKey(const Key('car-serve-floor-button-A')));
      expect(tester.takeException(), isNull);

      // Floor 4 preview is blocked -> Submit button disabled and blocker shown.
      await scrollAndTap(find.byKey(const Key('serve-floor-choice-4')));
      expect(tester.takeException(), isNull);
      expect(
        find.byKey(const Key('serve-floor-preview-blocker')),
        findsOneWidget,
      );
      final FilledButton blockedSubmit = tester.widget<FilledButton>(
        find.byKey(const Key('serve-floor-submit-button')),
      );
      expect(blockedSubmit.onPressed, isNull);

      // Floor 5 preview is eligible -> submit -> server rejects (TOCTOU) -> Rejected displayed!
      blockPreviewFloor4 = false;
      await scrollAndTap(find.byKey(const Key('serve-floor-choice-5')));
      await scrollAndTap(find.byKey(const Key('serve-floor-submit-button')));
      expect(tester.takeException(), isNull);
      expect(find.text('Rejected'), findsWidgets);
      expect(find.textContaining('active_intent'), findsWidgets);
      expect(transport.submittedKeys, <String>['sub-narrow-2-key']);

      // Tap New attempt -> mints brand-new correlation key, submit loses response -> Unknown!
      await scrollAndTap(find.byKey(const Key('modal-new-attempt-button')));
      await scrollAndTap(find.byKey(const Key('serve-floor-choice-5')));
      await scrollAndTap(find.byKey(const Key('serve-floor-submit-button')));
      expect(tester.takeException(), isNull);

      expect(transport.submittedKeys, <String>[
        'sub-narrow-2-key',
        'sub-narrow-3-key',
      ]);
      expect(find.text('Unknown'), findsWidgets);
      expect(find.textContaining('No automatic resend'), findsWidgets);

      // Query status recovers Completed WITHOUT resending submit!
      await scrollAndTap(find.byKey(const Key('modal-query-status-button')));
      expect(tester.takeException(), isNull);

      expect(transport.submitCalls, 2);
      expect(find.text('Completed'), findsWidgets);
      expect(find.text('IntentId #10'), findsWidgets);
    },
  );

  testWidgets(
    'labels offline car location as retained (not live) and hides New attempt after Host restarts read-only (404)',
    (WidgetTester tester) async {
      tester.view.devicePixelRatio = 1.0;
      tester.view.physicalSize = const Size(1280, 900);
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);

      final _WidgetControlTransport transport = _WidgetControlTransport();
      bool controlEnabled = true;

      transport.onCapabilities = () async => controlEnabled
          ? ControlHttpResponse(
              statusCode: 200,
              body: transport.capabilitiesJson,
              contentType: 'application/json',
            )
          : const ControlHttpResponse(
              statusCode: 404,
              body: '',
              contentType: 'text/plain',
            );
      transport.onSubmit = (String rawBody) async {
        final Map<String, Object?> req =
            jsonDecode(rawBody) as Map<String, Object?>;
        return ControlHttpResponse(
          statusCode: 200,
          body: jsonEncode(<String, Object?>{
            'control_api_version': 'dev-sim/v0',
            'development_only': true,
            'control_instance_id': '11111111111111111111111111111111',
            'ephemeral_process_memory': true,
            'submission_key': req['submission_key'],
            'operation': 'serve_floor',
            'car': req['car'],
            'floor': req['floor'],
            'status': 'accepted',
            'duplicate_submission': false,
            'intent_id': 1,
            'receipt': 'awaiting',
            'reconciliation': 'pending',
            'diverged_floor': null,
            'saw_progress': false,
            'is_active_in_engine': true,
            'blocker': null,
            'knowledge_reason': null,
          }),
          contentType: 'application/json',
        );
      };
      transport.onQuery = (String _, String? _) async =>
          const ControlHttpResponse(
            statusCode: 404,
            body: '',
            contentType: 'text/plain',
          );

      final ClientController controller = ClientController(
        transport: transport,
        retryBackoff: (_) => const Duration(milliseconds: 50),
        submissionKeyGenerator: () => 'sub-offline-label-01',
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

      await tester.tap(find.byKey(const Key('car-serve-floor-button-A')));
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(const Key('serve-floor-choice-6')));
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(const Key('serve-floor-submit-button')));
      await tester.pumpAndSettle();

      expect(find.text('Live car location'), findsOneWidget);
      expect(find.byKey(const Key('modal-new-attempt-button')), findsOneWidget);

      // Disconnect Host while modal status view stays open -> location MUST NOT be called "Live"!
      await tester.runAsync(() => controller.disconnect());
      await tester.pumpAndSettle();
      expect(find.text('Live car location'), findsNothing);
      expect(find.text('Retained location'), findsOneWidget);
      expect(find.text('3.00F (not live)'), findsOneWidget);

      // Reconnect to Host that restarted without control surface (HTTP 404) ->
      // honest Unknown (control_surface_unavailable), New attempt hidden, zero resends!
      controlEnabled = false;
      await tester.runAsync(() async {
        await controller.reconnectNow();
        await Future<void>.delayed(const Duration(milliseconds: 20));
      });
      await tester.pumpAndSettle();

      expect(transport.submitCalls, 1);
      expect(find.text('Unknown'), findsWidgets);
      expect(find.text('control_surface_unavailable'), findsOneWidget);
      expect(find.byKey(const Key('modal-new-attempt-button')), findsNothing);
    },
  );
}
