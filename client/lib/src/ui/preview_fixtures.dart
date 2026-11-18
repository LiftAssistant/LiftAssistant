import 'dart:async';

import '../api/host_url.dart';
import '../transport/host_transport.dart';

/// Synthetic `experimental/v0` snapshot JSON for UI/layout stress validation only.
///
/// Covers:
/// - 4 operational cars (including a long logical CarId, large floor 128,
///   negative floor -2, discrete floor vs continuous position, FireService,
///   OutOfService, Inspection, Normal, Known/None/Unknown destination, and
///   Open/Closed/Obstructed/Fault door states);
/// - 3 observation sources (RMF primary, BACnet supervisory, CiA 417 auxiliary)
///   with long source/session IDs, Operational + Auxiliary bindings, one
///   unresolved Operational observation, and one stale source with active discovery;
/// - Host `Synchronizing` / `NotReady` state;
/// - 24 Host diagnostic entries across all 5 layers with long diagnostic IDs.
const String kStressPreviewSnapshotJson = '''
{
  "api_version": "experimental/v0",
  "revision": 84,
  "lifecycle": "synchronizing",
  "readiness": "not_ready",
  "operational_cars": [
    {
      "id": "lift-north-A",
      "location": {"kind": "floor", "floor": 128},
      "motion": {"state": "stopped"},
      "door": "open",
      "operating_mode": "normal",
      "reported_destination": {"kind": "known", "floor": 128}
    },
    {
      "id": "building-north-tower-high-rise-service-elevator-car-02",
      "location": {"kind": "continuous", "floor_position": 42.35, "nearest_floor": 42},
      "motion": {"state": "moving", "direction": "up"},
      "door": "closed",
      "operating_mode": "fire_service",
      "reported_destination": {"kind": "unknown"}
    },
    {
      "id": "lift-west-express-C",
      "location": {"kind": "continuous", "floor_position": -1.80, "nearest_floor": -2},
      "motion": {"state": "moving", "direction": "down"},
      "door": "obstructed",
      "operating_mode": "inspection",
      "reported_destination": {"kind": "known", "floor": -2}
    },
    {
      "id": "lift-freight-bay-D",
      "location": {"kind": "floor", "floor": 1},
      "motion": {"state": "stopped"},
      "door": "fault",
      "operating_mode": "out_of_service",
      "reported_destination": {"kind": "none"}
    }
  ],
  "sources": [
    {
      "source": "rmf/campus-north-tower-primary-adapter-gateway",
      "session": {
        "state": "active",
        "session": "session-rmf-north-tower-2026-11-18-instance-009481"
      },
      "transport": "discovery_observed",
      "payload": "active",
      "observations": [
        {"car": "lift-north-A", "role": "operational", "availability": "resolved"},
        {
          "car": "building-north-tower-high-rise-service-elevator-car-02",
          "role": "operational",
          "availability": "resolved"
        },
        {"car": "lift-west-express-C", "role": "operational", "availability": "resolved"},
        {"car": "lift-south-E", "role": "operational", "availability": "unresolved"}
      ]
    },
    {
      "source": "bacnet/bms-wing-west-supervisory-controller-02",
      "session": {
        "state": "active",
        "session": "session-bacnet-cov-gateway-west-wing-7742"
      },
      "transport": "discovery_observed",
      "payload": "stale",
      "observations": [
        {"car": "lift-freight-bay-D", "role": "operational", "availability": "resolved"},
        {"car": "lift-north-A", "role": "auxiliary", "availability": "unresolved"},
        {
          "car": "building-north-tower-high-rise-service-elevator-car-02",
          "role": "auxiliary",
          "availability": "incomplete"
        }
      ]
    },
    {
      "source": "cia417/hoistway-can-bridge-auxiliary-observer",
      "session": {
        "state": "active",
        "session": "session-cia417-canopen-bus-0-node-14"
      },
      "transport": "subscriber_created",
      "payload": "awaiting",
      "observations": [
        {"car": "lift-west-express-C", "role": "auxiliary", "availability": "awaiting"},
        {"car": "lift-freight-bay-D", "role": "auxiliary", "availability": "revoked"}
      ]
    }
  ],
  "recent_diagnostics": [
    {
      "sequence": 61,
      "layer": "transport",
      "id": "transport.subscriber-created",
      "source": "rmf/campus-north-tower-primary-adapter-gateway",
      "session": "session-rmf-north-tower-2026-11-18-instance-009481",
      "car": null,
      "role": null,
      "native_entity": "/lift_states"
    },
    {
      "sequence": 62,
      "layer": "source",
      "id": "source.session-activated",
      "source": "rmf/campus-north-tower-primary-adapter-gateway",
      "session": "session-rmf-north-tower-2026-11-18-instance-009481",
      "car": null,
      "role": null,
      "native_entity": null
    },
    {
      "sequence": 63,
      "layer": "transport",
      "id": "transport.discovery-observed",
      "source": "rmf/campus-north-tower-primary-adapter-gateway",
      "session": "session-rmf-north-tower-2026-11-18-instance-009481",
      "car": null,
      "role": null,
      "native_entity": "rmf_lift_msgs/msg/LiftState"
    },
    {
      "sequence": 64,
      "layer": "observation",
      "id": "observation.resolved",
      "source": "rmf/campus-north-tower-primary-adapter-gateway",
      "session": "session-rmf-north-tower-2026-11-18-instance-009481",
      "car": "lift-north-A",
      "role": "operational",
      "native_entity": "NorthTower_Lift_A"
    },
    {
      "sequence": 65,
      "layer": "observation",
      "id": "observation.resolved",
      "source": "rmf/campus-north-tower-primary-adapter-gateway",
      "session": "session-rmf-north-tower-2026-11-18-instance-009481",
      "car": "building-north-tower-high-rise-service-elevator-car-02",
      "role": "operational",
      "native_entity": "NorthTower_Service_Lift_02"
    },
    {
      "sequence": 66,
      "layer": "observation",
      "id": "observation.resolved",
      "source": "rmf/campus-north-tower-primary-adapter-gateway",
      "session": "session-rmf-north-tower-2026-11-18-instance-009481",
      "car": "lift-west-express-C",
      "role": "operational",
      "native_entity": "WestWing_Express_C"
    },
    {
      "sequence": 67,
      "layer": "source",
      "id": "source.session-activated",
      "source": "bacnet/bms-wing-west-supervisory-controller-02",
      "session": "session-bacnet-cov-gateway-west-wing-7742",
      "car": null,
      "role": null,
      "native_entity": null
    },
    {
      "sequence": 68,
      "layer": "observation",
      "id": "observation.resolved",
      "source": "bacnet/bms-wing-west-supervisory-controller-02",
      "session": "session-bacnet-cov-gateway-west-wing-7742",
      "car": "lift-freight-bay-D",
      "role": "operational",
      "native_entity": "BACnet_LiftObject_404"
    },
    {
      "sequence": 69,
      "layer": "readiness",
      "id": "readiness.changed.ready_read_only",
      "source": null,
      "session": null,
      "car": null,
      "role": null,
      "native_entity": null
    },
    {
      "sequence": 70,
      "layer": "mapping",
      "id": "mapping.rejected.unconfigured-universal-floor-reference",
      "source": "bacnet/bms-wing-west-supervisory-controller-02",
      "session": "session-bacnet-cov-gateway-west-wing-7742",
      "car": "lift-north-A",
      "role": "auxiliary",
      "native_entity": "BACnet_LiftObject_101"
    },
    {
      "sequence": 71,
      "layer": "observation",
      "id": "observation.unresolved.disagreeing-multi-door-array",
      "source": "bacnet/bms-wing-west-supervisory-controller-02",
      "session": "session-bacnet-cov-gateway-west-wing-7742",
      "car": "lift-north-A",
      "role": "auxiliary",
      "native_entity": "BACnet_LiftObject_101"
    },
    {
      "sequence": 72,
      "layer": "observation",
      "id": "observation.incomplete.omitted-operating-mode-evidence",
      "source": "bacnet/bms-wing-west-supervisory-controller-02",
      "session": "session-bacnet-cov-gateway-west-wing-7742",
      "car": "building-north-tower-high-rise-service-elevator-car-02",
      "role": "auxiliary",
      "native_entity": "BACnet_LiftObject_102"
    },
    {
      "sequence": 73,
      "layer": "source",
      "id": "source.session-activated",
      "source": "cia417/hoistway-can-bridge-auxiliary-observer",
      "session": "session-cia417-canopen-bus-0-node-14",
      "car": null,
      "role": null,
      "native_entity": null
    },
    {
      "sequence": 74,
      "layer": "transport",
      "id": "transport.subscriber-created",
      "source": "cia417/hoistway-can-bridge-auxiliary-observer",
      "session": "session-cia417-canopen-bus-0-node-14",
      "car": null,
      "role": null,
      "native_entity": "can0:cia417-tpdo"
    },
    {
      "sequence": 75,
      "layer": "mapping",
      "id": "mapping.rejected.unadvertised-destination-floor-label",
      "source": "rmf/campus-north-tower-primary-adapter-gateway",
      "session": "session-rmf-north-tower-2026-11-18-instance-009481",
      "car": "lift-south-E",
      "role": "operational",
      "native_entity": "SouthTower_Lift_E"
    },
    {
      "sequence": 76,
      "layer": "observation",
      "id": "observation.unresolved.undirected-door-motion-and-source-specific-mode",
      "source": "rmf/campus-north-tower-primary-adapter-gateway",
      "session": "session-rmf-north-tower-2026-11-18-instance-009481",
      "car": "lift-south-E",
      "role": "operational",
      "native_entity": "SouthTower_Lift_E"
    },
    {
      "sequence": 77,
      "layer": "readiness",
      "id": "readiness.changed.not_ready",
      "source": "rmf/campus-north-tower-primary-adapter-gateway",
      "session": "session-rmf-north-tower-2026-11-18-instance-009481",
      "car": "lift-south-E",
      "role": "operational",
      "native_entity": null
    },
    {
      "sequence": 78,
      "layer": "transport",
      "id": "transport.receive-failed.dds-reader-sample-lost",
      "source": "bacnet/bms-wing-west-supervisory-controller-02",
      "session": "session-bacnet-cov-gateway-west-wing-7742",
      "car": null,
      "role": null,
      "native_entity": "udp://192.168.40.12:47808"
    },
    {
      "sequence": 79,
      "layer": "source",
      "id": "source.payload-stale",
      "source": "bacnet/bms-wing-west-supervisory-controller-02",
      "session": "session-bacnet-cov-gateway-west-wing-7742",
      "car": "lift-freight-bay-D",
      "role": "operational",
      "native_entity": "BACnet_LiftObject_404"
    },
    {
      "sequence": 80,
      "layer": "observation",
      "id": "observation.revoked.retired-source-session",
      "source": "cia417/hoistway-can-bridge-auxiliary-observer",
      "session": "session-cia417-canopen-bus-0-node-14",
      "car": "lift-freight-bay-D",
      "role": "auxiliary",
      "native_entity": "CiA417_VirtualLift_4"
    },
    {
      "sequence": 81,
      "layer": "mapping",
      "id": "mapping.rejected.unknown-source-operating-mode-enum",
      "source": "cia417/hoistway-can-bridge-auxiliary-observer",
      "session": "session-cia417-canopen-bus-0-node-14",
      "car": "lift-west-express-C",
      "role": "auxiliary",
      "native_entity": "CiA417_VirtualLift_3"
    },
    {
      "sequence": 82,
      "layer": "observation",
      "id": "observation.updated.continuous-position-offset",
      "source": "rmf/campus-north-tower-primary-adapter-gateway",
      "session": "session-rmf-north-tower-2026-11-18-instance-009481",
      "car": "building-north-tower-high-rise-service-elevator-car-02",
      "role": "operational",
      "native_entity": "NorthTower_Service_Lift_02"
    },
    {
      "sequence": 83,
      "layer": "observation",
      "id": "observation.updated.door-obstructed",
      "source": "rmf/campus-north-tower-primary-adapter-gateway",
      "session": "session-rmf-north-tower-2026-11-18-instance-009481",
      "car": "lift-west-express-C",
      "role": "operational",
      "native_entity": "WestWing_Express_C"
    },
    {
      "sequence": 84,
      "layer": "readiness",
      "id": "readiness.synchronizing.awaiting-complete-operational-fleet",
      "source": "rmf/campus-north-tower-primary-adapter-gateway",
      "session": "session-rmf-north-tower-2026-11-18-instance-009481",
      "car": "lift-south-E",
      "role": "operational",
      "native_entity": "SouthTower_Lift_E"
    }
  ]
}
''';

/// In-memory [HostTransport] used only when an explicit UI preview fixture
/// (`?preview=stress` or `?preview=reconnecting`) is requested for layout
/// verification. Never used in normal Host connections.
final class PreviewFixtureTransport implements HostTransport {
  PreviewFixtureTransport({
    this.snapshotJson = kStressPreviewSnapshotJson,
    this.simulateReconnectingAfterInitialSnapshot = false,
  });

  final String snapshotJson;
  final bool simulateReconnectingAfterInitialSnapshot;
  bool _initialFetchDone = false;
  StreamController<String>? _streamController;

  @override
  Future<String> fetchSnapshotJson(HostEndpoint endpoint) async {
    if (_initialFetchDone && simulateReconnectingAfterInitialSnapshot) {
      throw const HostUnreachableException(
        'Simulated preview stream drop (reconnecting with retained snapshot)',
      );
    }
    _initialFetchDone = true;
    return snapshotJson;
  }

  @override
  Future<HostSseConnection> openSnapshotEvents(HostEndpoint endpoint) async {
    await _streamController?.close();
    final StreamController<String> controller = StreamController<String>();
    _streamController = controller;
    if (simulateReconnectingAfterInitialSnapshot) {
      scheduleMicrotask(() {
        if (!controller.isClosed) {
          controller.addError(
            const HostStreamDisconnectedException(
              'Simulated SSE stream interruption for UI layout validation',
            ),
          );
        }
      });
    }
    return HostSseConnection(
      snapshots: controller.stream,
      onClose: () async {
        if (!controller.isClosed) {
          await controller.close();
        }
      },
    );
  }

  @override
  void dispose() {
    final StreamController<String>? current = _streamController;
    _streamController = null;
    if (current != null && !current.isClosed) {
      unawaited(current.close());
    }
  }
}
