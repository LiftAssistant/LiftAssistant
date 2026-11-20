import 'package:flutter_test/flutter_test.dart';
import 'package:liftassistant_client/src/api/control_model.dart';
import 'package:liftassistant_client/src/api/control_model_decoder.dart';
import 'package:liftassistant_client/src/api/read_model.dart';

void main() {
  test('DevControlV0Decoder decodes distinct capability axes and exact execution floors', () {
    const String raw = '''
      {
        "control_api_version": "dev-sim/v0",
        "development_only": true,
        "control_instance_id": "0123456789abcdef0123456789abcdef",
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
          },
          {
            "car": "B",
            "serve_floor": {
              "support": "unknown",
              "control_path_availability": "unavailable",
              "control_path_reason": "capabilities_unavailable",
              "authorization": "fenced",
              "executable_floors": [],
              "active_intent_id": 7
            }
          },
          {
            "car": "C",
            "serve_floor": {
              "support": "unsupported",
              "control_path_availability": "unavailable",
              "control_path_reason": "controller_unbound",
              "authorization": "unbound",
              "executable_floors": [],
              "active_intent_id": null
            }
          }
        ]
      }
      ''';

    final ControlSurfaceAvailable decoded =
        DevControlV0Decoder.decodeCapabilitiesJson(raw);
    expect(decoded.developmentOnly, isTrue);
    expect(decoded.multiUserAuth, isFalse);
    expect(decoded.controlInstanceId, '0123456789abcdef0123456789abcdef');

    final CarServeFloorCapability carA = decoded.capabilityForCar('A')!;
    expect(carA.support, OperationSupport.supported);
    expect(carA.controlPathAvailability, ControlPathAvailability.available);
    expect(carA.authorization, ControlAuthorizationKind.devProcessLocalGrant);
    expect(carA.executableFloors, <int>[1, 2, 3, 4, 5, 6]);

    final CarServeFloorCapability carB = decoded.capabilityForCar('B')!;
    expect(carB.support, OperationSupport.unknown);
    expect(carB.controlPathAvailability, ControlPathAvailability.unavailable);
    expect(carB.controlPathReason, 'capabilities_unavailable');
    expect(carB.authorization, ControlAuthorizationKind.fenced);
    expect(carB.activeIntentId, 7);

    final CarServeFloorCapability carC = decoded.capabilityForCar('C')!;
    expect(carC.support, OperationSupport.unsupported);
    expect(carC.authorization, ControlAuthorizationKind.unbound);
  });

  test('DevControlV0Decoder decodes advisory preview including blocked and unavailable-car states', () {
    const String rawBlocked = '''
      {
        "control_api_version": "dev-sim/v0",
        "development_only": true,
        "control_instance_id": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "advisory": true,
        "operation": "serve_floor",
        "car": "B",
        "floor": 4,
        "support": "supported",
        "control_path_availability": "available",
        "control_path_reason": null,
        "authorization": "dev_process_local_grant",
        "eligible": false,
        "blocker": {
          "code": "observation_unavailable",
          "detail": "current valid Operational observation state is unavailable",
          "blocking_intent_id": null,
          "observed_mode": null
        },
        "observed_car_available": false,
        "observed_floor_reference": null,
        "observed_operating_mode": null
      }
      ''';

    final ServeFloorPreview preview = DevControlV0Decoder.decodePreviewJson(
      rawBlocked,
    );
    expect(preview.advisory, isTrue);
    expect(preview.eligible, isFalse);
    expect(preview.observedCarAvailable, isFalse);
    expect(preview.observedFloorReference, isNull);
    expect(preview.observedOperatingMode, isNull);
    expect(preview.blocker?.code, 'observation_unavailable');

    const String rawModeBlocked = '''
      {
        "control_api_version": "dev-sim/v0",
        "development_only": true,
        "control_instance_id": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "advisory": true,
        "operation": "serve_floor",
        "car": "A",
        "floor": 6,
        "support": "supported",
        "control_path_availability": "available",
        "control_path_reason": null,
        "authorization": "dev_process_local_grant",
        "eligible": false,
        "blocker": {
          "code": "mode_not_normal",
          "detail": "car operating mode is OutOfService, not Normal",
          "blocking_intent_id": null,
          "observed_mode": "out_of_service"
        },
        "observed_car_available": true,
        "observed_floor_reference": 3,
        "observed_operating_mode": "out_of_service"
      }
      ''';
    final ServeFloorPreview modePreview = DevControlV0Decoder.decodePreviewJson(
      rawModeBlocked,
    );
    expect(modePreview.observedOperatingMode, CarOperatingMode.outOfService);
    expect(modePreview.blocker?.observedMode, CarOperatingMode.outOfService);
  });

  test('DevControlV0Decoder decodes all projected submission statuses and rejects unsupported versions', () {
    for (final ProjectedOperationStatus status
        in ProjectedOperationStatus.values) {
      final String raw =
          '''
        {
          "control_api_version": "dev-sim/v0",
          "development_only": true,
          "control_instance_id": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
          "ephemeral_process_memory": true,
          "submission_key": "sub-test-0001",
          "operation": "serve_floor",
          "car": "A",
          "floor": 5,
          "status": "${status.wireValue}",
          "duplicate_submission": false,
          "intent_id": 1,
          "receipt": "accepted",
          "reconciliation": "pending",
          "diverged_floor": null,
          "saw_progress": false,
          "is_active_in_engine": true,
          "blocker": null,
          "knowledge_reason": null
        }
        ''';
      final ProjectedSubmission decoded =
          DevControlV0Decoder.decodeSubmissionJson(raw);
      expect(decoded.status, status);
    }

    expect(
      () => DevControlV0Decoder.decodeCapabilitiesJson(
        '{"control_api_version":"dev-sim/v99"}',
      ),
      throwsA(isA<UnsupportedControlApiVersionException>()),
    );
    expect(
      () => DevControlV0Decoder.decodeSubmissionJson('{not json'),
      throwsA(isA<MalformedControlPayloadException>()),
    );
  });
}
