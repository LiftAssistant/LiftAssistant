import 'dart:async';
import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:liftassistant_client/src/api/control_model.dart';
import 'package:liftassistant_client/src/api/host_url.dart';
import 'package:liftassistant_client/src/state/client_controller.dart';
import 'package:liftassistant_client/src/transport/host_transport.dart';

String _snapshotJson(int revision, {int carAFloor = 3, bool moving = false}) {
  final String motionJson = moving
      ? '{"state": "moving", "direction": "up"}'
      : '{"state": "stopped"}';
  return '''
  {
    "api_version": "experimental/v0",
    "revision": $revision,
    "lifecycle": "ready_read_only",
    "readiness": "read_only",
    "operational_cars": [
      {
        "id": "A",
        "location": {"kind": "continuous", "floor_position": $carAFloor.0, "nearest_floor": $carAFloor},
        "motion": $motionJson,
        "door": "${moving ? "closed" : "open"}",
        "operating_mode": "normal",
        "reported_destination": {"kind": "none"}
      }
    ],
    "sources": [
      {
        "source": "simulator",
        "session": {"state": "active", "session": "sim-1"},
        "transport": "discovery_observed",
        "payload": "active",
        "observations": [
          {"car": "A", "role": "operational", "availability": "resolved"}
        ]
      }
    ],
    "recent_diagnostics": []
  }
  ''';
}

String _capabilitiesJson(String instanceId) =>
    '''
{
  "control_api_version": "dev-sim/v0",
  "development_only": true,
  "control_instance_id": "$instanceId",
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
        "executable_floors": [1, 2, 3, 4, 5, 6],
        "active_intent_id": null
      }
    }
  ]
}
''';

String _submissionJson({
  required String instanceId,
  required String key,
  required String status,
  int? intentId,
  String? receipt,
  String? reconciliation,
  bool sawProgress = false,
  bool active = false,
  String? blockerCode,
  String? knowledgeReason,
}) {
  final String blockerJson = blockerCode == null
      ? 'null'
      : '{"code": "$blockerCode", "detail": "blocked by $blockerCode", "blocking_intent_id": null, "observed_mode": null}';
  return '''
  {
    "control_api_version": "dev-sim/v0",
    "development_only": true,
    "control_instance_id": "$instanceId",
    "ephemeral_process_memory": true,
    "submission_key": "$key",
    "operation": "serve_floor",
    "car": "A",
    "floor": 6,
    "status": "$status",
    "duplicate_submission": false,
    "intent_id": ${intentId ?? 'null'},
    "receipt": ${receipt == null ? 'null' : '"$receipt"'},
    "reconciliation": ${reconciliation == null ? 'null' : '"$reconciliation"'},
    "diverged_floor": null,
    "saw_progress": $sawProgress,
    "is_active_in_engine": $active,
    "blocker": $blockerJson,
    "knowledge_reason": ${knowledgeReason == null ? 'null' : '"$knowledgeReason"'}
  }
  ''';
}

final class _FakeControlHostTransport
    implements HostTransport, DevControlTransport {
  String snapshotBody = _snapshotJson(1);
  StreamController<String> sseController = StreamController<String>();

  int capabilitiesCalls = 0;
  int previewCalls = 0;
  int submitCalls = 0;
  int queryCalls = 0;
  final List<String> queriedExpectedInstances = <String>[];

  Future<ControlHttpResponse> Function(HostEndpoint)? onCapabilities;
  Future<ControlHttpResponse> Function(HostEndpoint, String)? onPreview;
  Future<ControlHttpResponse> Function(HostEndpoint, String)? onSubmit;
  Future<ControlHttpResponse> Function(HostEndpoint, String, String?)? onQuery;

  @override
  Future<String> fetchSnapshotJson(HostEndpoint endpoint) async => snapshotBody;

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
    capabilitiesCalls++;
    if (onCapabilities != null) {
      return onCapabilities!(endpoint);
    }
    return ControlHttpResponse(
      statusCode: 200,
      body: _capabilitiesJson('inst-alpha-0001'),
      contentType: 'application/json',
    );
  }

  @override
  Future<ControlHttpResponse> postServeFloorPreview(
    HostEndpoint endpoint,
    String jsonBody,
  ) async {
    previewCalls++;
    return onPreview!(endpoint, jsonBody);
  }

  @override
  Future<ControlHttpResponse> postServeFloorSubmit(
    HostEndpoint endpoint,
    String jsonBody,
  ) async {
    submitCalls++;
    return onSubmit!(endpoint, jsonBody);
  }

  @override
  Future<ControlHttpResponse> fetchSubmissionStatus(
    HostEndpoint endpoint,
    String submissionKey, {
    String? expectedInstanceId,
  }) async {
    queryCalls++;
    if (expectedInstanceId != null) {
      queriedExpectedInstances.add(expectedInstanceId);
    }
    return onQuery!(endpoint, submissionKey, expectedInstanceId);
  }

  @override
  void dispose() {
    if (!sseController.isClosed) {
      unawaited(sseController.close());
    }
  }
}

void main() {
  test('response loss enters Unknown without auto-resend and reconnect recovers via read-only GET query', () async {
    final _FakeControlHostTransport transport = _FakeControlHostTransport();
    String currentServerStatus = 'in_progress';
    String currentReconciliation = 'pending';
    const bool currentSawProgress = true;

    transport.onSubmit = (HostEndpoint _, String body) async {
      final Map<String, Object?> decoded =
          jsonDecode(body) as Map<String, Object?>;
      expect(decoded['submission_key'], 'sub-loss-key-0001');
      // Simulate mid-flight response loss after server accepted the operation.
      throw const HostUnreachableException('Connection reset before response');
    };
    transport.onQuery =
        (HostEndpoint _, String key, String? expectedInstance) async {
          return ControlHttpResponse(
            statusCode: 200,
            body: _submissionJson(
              instanceId: 'inst-alpha-0001',
              key: key,
              status: currentServerStatus,
              intentId: 1,
              receipt: 'accepted',
              reconciliation: currentReconciliation,
              sawProgress: currentSawProgress,
              active: currentServerStatus != 'completed',
            ),
            contentType: 'application/json',
          );
        };

    final ClientController controller = ClientController(
      transport: transport,
      retryBackoff: (_) => const Duration(milliseconds: 20),
      submissionKeyGenerator: () => 'sub-loss-key-0001',
    );
    addTearDown(controller.dispose);

    await controller.connectToUrl('http://127.0.0.1:8080');
    await Future<void>.delayed(const Duration(milliseconds: 40));
    expect(
      controller.state.controlCapabilities,
      isA<ControlSurfaceAvailable>(),
    );

    final String key = controller.mintSubmissionKey();
    await controller.submitServeFloor(
      submissionKey: key,
      car: 'A',
      floor: 6,
      expectedControlInstanceId: 'inst-alpha-0001',
    );

    // Response lost -> first-class Unknown, NEVER automatic resend.
    expect(transport.submitCalls, 1);
    expect(controller.state.trackedOperation?.responseLost, isTrue);
    expect(
      controller.state.trackedOperation?.effectiveStatus,
      ProjectedOperationStatus.knowledgeUnavailable,
    );
    expect(
      controller.state.trackedOperation?.effectiveKnowledgeReason,
      'response_lost',
    );

    // Attempting a duplicate submit with the same key is suppressed client-side too.
    await controller.submitServeFloor(
      submissionKey: key,
      car: 'A',
      floor: 6,
      expectedControlInstanceId: 'inst-alpha-0001',
    );
    expect(transport.submitCalls, 1);

    // SSE disconnects and automatically reconnects -> controller queries via GET ONLY.
    transport.sseController.addError(
      const HostStreamDisconnectedException('network drop'),
    );
    await Future<void>.delayed(const Duration(milliseconds: 80));

    expect(transport.submitCalls, 1, reason: 'Reconnect MUST NOT resend POST');
    expect(transport.queryCalls, greaterThanOrEqualTo(1));
    expect(transport.queriedExpectedInstances, contains('inst-alpha-0001'));
    expect(
      controller.state.trackedOperation?.effectiveStatus,
      ProjectedOperationStatus.inProgress,
    );
    expect(controller.state.trackedOperation?.projection?.intentId, 1);

    // Next SSE snapshot after physical arrival triggers read-only GET -> Completed.
    currentServerStatus = 'completed';
    currentReconciliation = 'satisfied';
    transport.sseController.add(_snapshotJson(2, carAFloor: 6));
    await Future<void>.delayed(const Duration(milliseconds: 40));

    expect(transport.submitCalls, 1);
    expect(
      controller.state.trackedOperation?.effectiveStatus,
      ProjectedOperationStatus.completed,
    );
    expect(
      controller.state.trackedOperation?.projection?.reconciliation,
      'satisfied',
    );
  });

  test('control process restart projects knowledge_unavailable without automatic resend', () async {
    final _FakeControlHostTransport transport = _FakeControlHostTransport();
    String serverInstance = 'inst-before-restart-1';
    transport.onCapabilities = (_) async => ControlHttpResponse(
      statusCode: 200,
      body: _capabilitiesJson(serverInstance),
      contentType: 'application/json',
    );
    transport.onSubmit = (_, String body) async {
      final Map<String, Object?> decoded =
          jsonDecode(body) as Map<String, Object?>;
      return ControlHttpResponse(
        statusCode: 200,
        body: _submissionJson(
          instanceId: serverInstance,
          key: decoded['submission_key']! as String,
          status: 'accepted',
          intentId: 1,
          receipt: 'awaiting',
          reconciliation: 'pending',
          active: true,
        ),
        contentType: 'application/json',
      );
    };
    transport.onQuery = (_, String key, String? expectedInstance) async {
      final bool restarted =
          expectedInstance != null && expectedInstance != serverInstance;
      return ControlHttpResponse(
        statusCode: 200,
        body: _submissionJson(
          instanceId: serverInstance,
          key: key,
          status: restarted ? 'knowledge_unavailable' : 'not_found',
          knowledgeReason: restarted
              ? 'control_process_restarted'
              : 'no_known_submission_in_current_process',
        ),
        contentType: 'application/json',
      );
    };

    final ClientController controller = ClientController(
      transport: transport,
      retryBackoff: (_) => const Duration(milliseconds: 20),
    );
    addTearDown(controller.dispose);

    await controller.connectToUrl('http://127.0.0.1:8080');
    await Future<void>.delayed(const Duration(milliseconds: 40));

    await controller.submitServeFloor(
      submissionKey: 'sub-restart-0001',
      car: 'A',
      floor: 6,
      expectedControlInstanceId: serverInstance,
    );
    expect(transport.submitCalls, 1);
    expect(
      controller.state.trackedOperation?.effectiveStatus,
      ProjectedOperationStatus.accepted,
    );

    // Simulate control process restart with new instance ID and lower snapshot revision.
    serverInstance = 'inst-after-restart-2';
    transport.snapshotBody = _snapshotJson(1);
    transport.sseController.addError(
      const HostStreamDisconnectedException('process restarted'),
    );
    await Future<void>.delayed(const Duration(milliseconds: 80));

    expect(transport.submitCalls, 1, reason: 'Restart MUST NOT resend POST');
    expect(
      controller.state.trackedOperation?.effectiveStatus,
      ProjectedOperationStatus.knowledgeUnavailable,
    );
    expect(
      controller.state.trackedOperation?.effectiveKnowledgeReason,
      'control_process_restarted',
    );
    expect(controller.state.trackedOperation?.isProcessRestarted, isTrue);
  });

  test('handles 404 read-only Host, 409 conflict, rejection, and endpoint switch cleanup', () async {
    final _FakeControlHostTransport transport = _FakeControlHostTransport();
    transport.onCapabilities = (_) async => const ControlHttpResponse(
      statusCode: 404,
      body: '',
      contentType: 'text/plain',
    );
    transport.onSubmit = (_, _) async => const ControlHttpResponse(
      statusCode: 409,
      body: '''
        {
          "control_api_version": "dev-sim/v0",
          "development_only": true,
          "control_instance_id": "inst-1",
          "error_code": "submission_key_conflict",
          "submission_key": "sub-conflict-001",
          "message": "submission_key is bound to a different request"
        }
        ''',
      contentType: 'application/json',
    );

    final ClientController controller = ClientController(transport: transport);
    addTearDown(controller.dispose);

    await controller.connectToUrl('http://127.0.0.1:8080');
    await Future<void>.delayed(const Duration(milliseconds: 40));
    expect(controller.state.controlCapabilities, const ControlSurfaceAbsent());

    await controller.submitServeFloor(
      submissionKey: 'sub-conflict-001',
      car: 'A',
      floor: 6,
    );
    expect(
      controller.state.trackedOperation?.conflictMessage,
      contains('different request'),
    );

    // Switching Host endpoint clears controlCapabilities and trackedOperation.
    await controller.connectToUrl('http://127.0.0.1:9090');
    expect(controller.state.trackedOperation, isNull);
  });
}
