import 'package:flutter_test/flutter_test.dart';
import 'package:liftassistant_client/src/api/read_model.dart';
import 'package:liftassistant_client/src/api/read_model_decoder.dart';

void main() {
  group('ExperimentalV0Decoder', () {
    test('decodes floor vs continuous locations, destinations, roles, and diagnostics', () {
      const String payload = '''
      {
        "api_version": "experimental/v0",
        "revision": 7,
        "lifecycle": "ready_read_only",
        "readiness": "read_only",
        "operational_cars": [
          {
            "id": "A",
            "location": {"kind": "floor", "floor": -1},
            "motion": {"state": "stopped"},
            "door": "closed",
            "operating_mode": "normal",
            "reported_destination": {"kind": "known", "floor": 4}
          },
          {
            "id": "B",
            "location": {"kind": "continuous", "floor_position": 2.25, "nearest_floor": 2},
            "motion": {"state": "moving", "direction": "up"},
            "door": "opening",
            "operating_mode": "inspection",
            "reported_destination": {"kind": "none"}
          },
          {
            "id": "C",
            "location": {"kind": "continuous", "floor_position": 5, "nearest_floor": 5},
            "motion": {"state": "moving", "direction": "down"},
            "door": "fault",
            "operating_mode": "out_of_service",
            "reported_destination": {"kind": "unknown"}
          }
        ],
        "sources": [
          {
            "source": "rmf/site-main",
            "session": {"state": "active", "session": "session-1"},
            "transport": "discovery_observed",
            "payload": "active",
            "observations": [
              {"car": "A", "role": "operational", "availability": "resolved"},
              {"car": "B", "role": "operational", "availability": "resolved"}
            ]
          },
          {
            "source": "bacnet/building-west",
            "session": {"state": "inactive"},
            "transport": "subscriber_created",
            "payload": "awaiting",
            "observations": [
              {"car": "A", "role": "auxiliary", "availability": "unresolved"},
              {"car": "C", "role": "operational", "availability": "revoked"}
            ]
          }
        ],
        "recent_diagnostics": [
          {
            "sequence": 4,
            "layer": "readiness",
            "id": "readiness.changed",
            "source": null,
            "session": null,
            "car": null,
            "role": null,
            "native_entity": null
          },
          {
            "sequence": 5,
            "layer": "mapping",
            "id": "mapping.rejected",
            "source": "rmf/site-main",
            "session": "session-1",
            "car": "A",
            "role": "auxiliary",
            "native_entity": "Lift1"
          }
        ]
      }
      ''';

      final ReadModelSnapshot snapshot = ExperimentalV0Decoder.decodeJson(
        payload,
      );
      expect(snapshot.apiVersion, 'experimental/v0');
      expect(snapshot.revision, 7);
      expect(snapshot.lifecycle, HostLifecycle.readyReadOnly);
      expect(snapshot.readiness, HostReadiness.readOnly);
      expect(snapshot.operationalCars, hasLength(3));

      final OperationalCar carA = snapshot.operationalCars[0];
      expect(carA.id, 'A');
      expect(carA.location, const FloorLocation(floor: -1));
      expect(carA.motion, const StoppedMotion());
      expect(carA.door, CarDoorState.closed);
      expect(carA.operatingMode, CarOperatingMode.normal);
      expect(
        carA.reportedDestination,
        const KnownReportedDestination(floor: 4),
      );

      final OperationalCar carB = snapshot.operationalCars[1];
      expect(
        carB.location,
        const ContinuousLocation(floorPosition: 2.25, nearestFloor: 2),
      );
      expect(
        carB.motion,
        const MovingMotion(direction: MotionDirection.up),
      );
      expect(carB.door, CarDoorState.opening);
      expect(carB.operatingMode, CarOperatingMode.inspection);
      expect(carB.reportedDestination, const NoReportedDestination());

      final OperationalCar carC = snapshot.operationalCars[2];
      expect(
        carC.location,
        const ContinuousLocation(floorPosition: 5.0, nearestFloor: 5),
      );
      expect(
        carC.motion,
        const MovingMotion(direction: MotionDirection.down),
      );
      expect(carC.door, CarDoorState.fault);
      expect(carC.operatingMode, CarOperatingMode.outOfService);
      expect(carC.reportedDestination, const UnknownReportedDestination());

      expect(snapshot.sources, hasLength(2));
      expect(
        snapshot.sources[0].session,
        const ActiveSourceSession(session: 'session-1'),
      );
      expect(snapshot.sources[0].transport, TransportState.discoveryObserved);
      expect(snapshot.sources[0].payload, PayloadActivity.active);
      expect(
        snapshot.sources[0].observations[0].role,
        BindingRole.operational,
      );
      expect(
        snapshot.sources[0].observations[0].availability,
        ObservationAvailability.resolved,
      );

      expect(snapshot.sources[1].session, const InactiveSourceSession());
      expect(snapshot.sources[1].transport, TransportState.subscriberCreated);
      expect(snapshot.sources[1].payload, PayloadActivity.awaiting);
      expect(snapshot.sources[1].observations[0].role, BindingRole.auxiliary);
      expect(
        snapshot.sources[1].observations[0].availability,
        ObservationAvailability.unresolved,
      );
      expect(
        snapshot.sources[1].observations[1].availability,
        ObservationAvailability.revoked,
      );

      expect(snapshot.recentDiagnostics, hasLength(2));
      expect(snapshot.recentDiagnostics[0].layer, DiagnosticLayer.readiness);
      expect(snapshot.recentDiagnostics[0].source, isNull);
      expect(snapshot.recentDiagnostics[1].layer, DiagnosticLayer.mapping);
      expect(snapshot.recentDiagnostics[1].id, 'mapping.rejected');
      expect(snapshot.recentDiagnostics[1].role, BindingRole.auxiliary);
      expect(snapshot.recentDiagnostics[1].nativeEntity, 'Lift1');
    });

    test('decodes all remaining HostLifecycle and HostReadiness variants', () {
      for (final (String lifecycleWire, HostLifecycle expectedLifecycle) in <(
        String,
        HostLifecycle,
      )>[
        ('starting', HostLifecycle.starting),
        ('synchronizing', HostLifecycle.synchronizing),
        ('degraded', HostLifecycle.degraded),
      ]) {
        final ReadModelSnapshot model = ExperimentalV0Decoder.decodeJson('''
        {
          "api_version": "experimental/v0",
          "revision": 1,
          "lifecycle": "$lifecycleWire",
          "readiness": "not_ready",
          "operational_cars": [],
          "sources": [],
          "recent_diagnostics": []
        }
        ''');
        expect(model.lifecycle, expectedLifecycle);
        expect(model.readiness, HostReadiness.notReady);
      }
    });

    test('reports UnsupportedApiVersionException explicitly before inspecting schema', () {
      expect(
        () => ExperimentalV0Decoder.decodeJson(
          '{"api_version": "experimental/v1", "completely_new_shape": true}',
        ),
        throwsA(
          isA<UnsupportedApiVersionException>().having(
            (UnsupportedApiVersionException e) => e.actualVersion,
            'actualVersion',
            'experimental/v1',
          ),
        ),
      );
    });

    test('rejects malformed JSON, missing fields, unknown enums, and out-of-range integers', () {
      expect(
        () => ExperimentalV0Decoder.decodeJson('{not-json'),
        throwsA(isA<MalformedReadModelException>()),
      );
      expect(
        () => ExperimentalV0Decoder.decodeJson('[]'),
        throwsA(isA<MalformedReadModelException>()),
      );
      expect(
        () => ExperimentalV0Decoder.decodeJson('{"revision": 1}'),
        throwsA(isA<MalformedReadModelException>()),
      );
      expect(
        () => ExperimentalV0Decoder.decodeJson('''
        {
          "api_version": "experimental/v0",
          "revision": -1,
          "lifecycle": "starting",
          "readiness": "not_ready",
          "operational_cars": [],
          "sources": [],
          "recent_diagnostics": []
        }
        '''),
        throwsA(isA<MalformedReadModelException>()),
      );
      expect(
        () => ExperimentalV0Decoder.decodeJson('''
        {
          "api_version": "experimental/v0",
          "revision": 1,
          "lifecycle": "invented_state",
          "readiness": "not_ready",
          "operational_cars": [],
          "sources": [],
          "recent_diagnostics": []
        }
        '''),
        throwsA(isA<MalformedReadModelException>()),
      );
      expect(
        () => ExperimentalV0Decoder.decodeJson('''
        {
          "api_version": "experimental/v0",
          "revision": 1,
          "lifecycle": "ready_read_only",
          "readiness": "read_only",
          "operational_cars": [
            {
              "id": "A",
              "location": {"kind": "floor", "floor": 40000},
              "motion": {"state": "stopped"},
              "door": "closed",
              "operating_mode": "normal",
              "reported_destination": {"kind": "none"}
            }
          ],
          "sources": [],
          "recent_diagnostics": []
        }
        '''),
        throwsA(isA<MalformedReadModelException>()),
      );
    });
  });
}
