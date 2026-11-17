import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:liftassistant_client/src/api/host_url.dart';
import 'package:liftassistant_client/src/api/read_model.dart';
import 'package:liftassistant_client/src/state/client_controller.dart';
import 'package:liftassistant_client/src/state/client_state.dart';
import 'package:liftassistant_client/src/transport/host_transport.dart';

String _snapshotJson({
  String apiVersion = 'experimental/v0',
  required int revision,
  String lifecycle = 'ready_read_only',
  String readiness = 'read_only',
  int floor = 3,
  String session = 'session-1',
}) {
  return '''
  {
    "api_version": "$apiVersion",
    "revision": $revision,
    "lifecycle": "$lifecycle",
    "readiness": "$readiness",
    "operational_cars": [
      {
        "id": "A",
        "location": {"kind": "floor", "floor": $floor},
        "motion": {"state": "stopped"},
        "door": "closed",
        "operating_mode": "normal",
        "reported_destination": {"kind": "none"}
      }
    ],
    "sources": [
      {
        "source": "rmf/site",
        "session": {"state": "active", "session": "$session"},
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

final class _FakeHostTransport implements HostTransport {
  late Future<String> Function(HostEndpoint endpoint) onFetchSnapshot;
  late Future<HostSseConnection> Function(HostEndpoint endpoint) onOpenEvents;

  @override
  Future<String> fetchSnapshotJson(HostEndpoint endpoint) =>
      onFetchSnapshot(endpoint);

  @override
  Future<HostSseConnection> openSnapshotEvents(HostEndpoint endpoint) =>
      onOpenEvents(endpoint);

  @override
  void dispose() {}
}

Future<void> _waitUntil(
  bool Function() predicate, {
  Duration timeout = const Duration(seconds: 2),
}) async {
  final Stopwatch sw = Stopwatch()..start();
  while (!predicate()) {
    if (sw.elapsed > timeout) {
      fail('Timed out waiting for ClientController condition.');
    }
    await Future<void>.delayed(const Duration(milliseconds: 10));
  }
}

void main() {
  group('ClientController synchronization and resilience', () {
    test('survives Host unavailable at startup and auto-reconnects when Host starts', () async {
      final _FakeHostTransport transport = _FakeHostTransport();
      bool hostUp = false;
      final StreamController<String> sseStream = StreamController<String>();
      addTearDown(sseStream.close);

      transport.onFetchSnapshot = (HostEndpoint endpoint) async {
        if (!hostUp) {
          throw const HostUnreachableException('Connection refused');
        }
        return _snapshotJson(revision: 1, floor: 2);
      };
      transport.onOpenEvents = (HostEndpoint endpoint) async {
        return HostSseConnection(
          snapshots: sseStream.stream,
          onClose: () async {},
        );
      };

      final ClientController controller = ClientController(
        transport: transport,
        retryBackoff: (_) => const Duration(milliseconds: 25),
      );
      addTearDown(controller.dispose);

      expect(await controller.connectToUrl('http://127.0.0.1:8080'), isTrue);
      await _waitUntil(
        () => controller.state.phase == ConnectionPhase.reconnecting,
      );
      expect(controller.state.snapshot, isNull);
      expect(controller.state.error?.kind, ClientErrorKind.unreachable);

      // Now bring the Host online; automatic backoff retry should recover to live.
      hostUp = true;
      await _waitUntil(() => controller.state.phase == ConnectionPhase.live);
      expect(controller.state.isSnapshotLive, isTrue);
      expect(controller.state.snapshot?.revision, 1);
      expect(
        controller.state.snapshot?.operationalCars.single.location,
        const FloorLocation(floor: 2),
      );
    });

    test('replaces complete state on SSE updates, handles drops, and accepts revision reset on Host restart', () async {
      final _FakeHostTransport transport = _FakeHostTransport();
      int hostGeneration = 1;
      StreamController<String> currentSse = StreamController<String>();

      transport.onFetchSnapshot = (HostEndpoint endpoint) async {
        if (hostGeneration == 1) {
          return _snapshotJson(revision: 10, floor: 3, session: 'gen-1');
        }
        // Host restarted: revision resets from 12 back to 1!
        return _snapshotJson(revision: 1, floor: 7, session: 'gen-2');
      };
      transport.onOpenEvents = (HostEndpoint endpoint) async {
        currentSse = StreamController<String>();
        return HostSseConnection(
          snapshots: currentSse.stream,
          onClose: () async {
            if (!currentSse.isClosed) {
              await currentSse.close();
            }
          },
        );
      };

      final ClientController controller = ClientController(
        transport: transport,
        retryBackoff: (_) => const Duration(milliseconds: 40),
      );
      addTearDown(() async {
        controller.dispose();
        if (!currentSse.isClosed) {
          await currentSse.close();
        }
      });

      await controller.connectToUrl('http://127.0.0.1:8080');
      await _waitUntil(() => controller.state.phase == ConnectionPhase.live);
      expect(controller.state.snapshot?.revision, 10);

      // Push newer complete snapshot via SSE.
      currentSse.add(_snapshotJson(revision: 12, floor: 5, session: 'gen-1'));
      await _waitUntil(() => controller.state.snapshot?.revision == 12);
      expect(
        controller.state.snapshot?.operationalCars.single.location,
        const FloorLocation(floor: 5),
      );
      expect(controller.state.lastRevisionReset, isNull);

      // Simulate Host restart: switch generation and drop SSE stream.
      hostGeneration = 2;
      currentSse.addError(
        const HostStreamDisconnectedException('Host process exited'),
      );

      // Verify automatic reconnect accepts smaller revision 1 without corruption error.
      await _waitUntil(
        () =>
            controller.state.phase == ConnectionPhase.live &&
            controller.state.snapshot?.revision == 1,
      );
      expect(controller.state.isSnapshotLive, isTrue);
      expect(
        controller.state.snapshot?.operationalCars.single.location,
        const FloorLocation(floor: 7),
      );
      expect(
        controller.state.lastRevisionReset?.previousRevision,
        12,
      );
      expect(controller.state.lastRevisionReset?.newRevision, 1);

      // Verify subsequent SSE updates on the new Host lifetime continue cleanly.
      currentSse.add(_snapshotJson(revision: 2, floor: 8, session: 'gen-2'));
      await _waitUntil(() => controller.state.snapshot?.revision == 2);
      expect(
        controller.state.snapshot?.operationalCars.single.location,
        const FloorLocation(floor: 8),
      );
    });

    test('retains last snapshot marked non-live while reconnecting and clears when switching Host URL', () async {
      final _FakeHostTransport transport = _FakeHostTransport();
      bool allowReconnect = false;
      final StreamController<String> sse = StreamController<String>();
      addTearDown(sse.close);

      transport.onFetchSnapshot = (HostEndpoint endpoint) async {
        if (!allowReconnect && endpoint.origin == 'http://127.0.0.1:8080') {
          return _snapshotJson(revision: 4, floor: 1);
        }
        throw const HostUnreachableException('Offline');
      };
      transport.onOpenEvents = (HostEndpoint endpoint) async {
        return HostSseConnection(snapshots: sse.stream, onClose: () async {});
      };

      final ClientController controller = ClientController(
        transport: transport,
        retryBackoff: (_) => const Duration(seconds: 5),
      );
      addTearDown(controller.dispose);

      await controller.connectToUrl('http://127.0.0.1:8080');
      await _waitUntil(() => controller.state.phase == ConnectionPhase.live);
      expect(controller.state.isSnapshotLive, isTrue);

      // Now make Host unreachable and drop SSE.
      allowReconnect = true;
      sse.addError(const HostStreamDisconnectedException('Connection reset'));
      await _waitUntil(
        () => controller.state.phase == ConnectionPhase.reconnecting,
      );
      expect(controller.state.isSnapshotLive, isFalse);
      expect(controller.state.hasRetainedStaleSnapshot, isTrue);
      expect(controller.state.snapshot?.revision, 4);

      // Switching to a DIFFERENT Host URL must clear Host A's retained snapshot.
      await controller.connectToUrl('http://127.0.0.1:9090');
      expect(controller.state.snapshot, isNull);
      expect(controller.state.hasRetainedStaleSnapshot, isFalse);
    });

    test('surfaces unsupported api_version and malformed SSE frames explicitly', () async {
      final _FakeHostTransport transport = _FakeHostTransport();
      String snapshotPayload = _snapshotJson(
        apiVersion: 'experimental/v99',
        revision: 1,
      );
      final StreamController<String> sse = StreamController<String>();
      addTearDown(sse.close);

      transport.onFetchSnapshot = (_) async => snapshotPayload;
      transport.onOpenEvents = (_) async =>
          HostSseConnection(snapshots: sse.stream, onClose: () async {});

      final ClientController controller = ClientController(
        transport: transport,
      );
      addTearDown(controller.dispose);

      await controller.connectToUrl('http://127.0.0.1:8080');
      await _waitUntil(
        () => controller.state.phase == ConnectionPhase.incompatibleApi,
      );
      expect(
        controller.state.error?.kind,
        ClientErrorKind.unsupportedApiVersion,
      );
      expect(controller.state.error?.unsupportedVersion, 'experimental/v99');

      // Switch to supported version and manual reconnect.
      snapshotPayload = _snapshotJson(revision: 5);
      await controller.reconnectNow();
      await _waitUntil(() => controller.state.phase == ConnectionPhase.live);
      expect(controller.state.snapshot?.revision, 5);

      // Inject malformed JSON on SSE stream -> transitions to protocolError without corrupting snapshot.
      sse.add('{corrupt-json');
      await _waitUntil(
        () => controller.state.phase == ConnectionPhase.protocolError,
      );
      expect(controller.state.error?.kind, ClientErrorKind.malformedResponse);
      expect(controller.state.isSnapshotLive, isFalse);
      expect(controller.state.snapshot?.revision, 5);
    });

    test('increments reconnect backoff when snapshot succeeds but SSE open or initial stream fails repeatedly', () async {
      final _FakeHostTransport transport = _FakeHostTransport();
      final List<int> observedAttempts = <int>[];
      int sseMode = 0;

      transport.onFetchSnapshot = (_) async => _snapshotJson(revision: 3);
      transport.onOpenEvents = (_) async {
        if (sseMode == 0) {
          throw const HostHttpStatusException(statusCode: 502, operation: 'SSE');
        }
        final StreamController<String> emptySse = StreamController<String>();
        unawaited(Future<void>.microtask(emptySse.close));
        return HostSseConnection(
          snapshots: emptySse.stream,
          onClose: () async {},
        );
      };

      final ClientController controller = ClientController(
        transport: transport,
        retryBackoff: (int attempt) {
          observedAttempts.add(attempt);
          return const Duration(milliseconds: 15);
        },
      );
      addTearDown(controller.dispose);

      await controller.connectToUrl('http://127.0.0.1:8080');
      await _waitUntil(() => observedAttempts.any((int a) => a >= 3));
      expect(observedAttempts.sublist(0, 3), <int>[1, 2, 3]);

      // Switch to immediate-EOF SSE stream (0 snapshots delivered) and verify attempts still advance.
      observedAttempts.clear();
      sseMode = 1;
      await controller.reconnectNow();
      await _waitUntil(() => observedAttempts.any((int a) => a >= 3));
      expect(observedAttempts.sublist(0, 3), <int>[1, 2, 3]);
    });
  });
}
