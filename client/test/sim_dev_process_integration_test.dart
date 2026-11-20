@TestOn('vm')
library;

import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:liftassistant_client/src/api/control_model.dart';
import 'package:liftassistant_client/src/api/read_model.dart';
import 'package:liftassistant_client/src/state/client_controller.dart';
import 'package:liftassistant_client/src/state/client_state.dart';
import 'package:liftassistant_client/src/transport/host_transport_io.dart';

String _resolveSimDevServerPath() {
  final File fromClientDir = File('../target/debug/examples/sim-dev-server');
  if (fromClientDir.existsSync()) {
    return fromClientDir.path;
  }
  final File fromRepoRoot = File('target/debug/examples/sim-dev-server');
  if (fromRepoRoot.existsSync()) {
    return fromRepoRoot.path;
  }
  throw StateError(
    'sim-dev-server example binary not found; build it with `cargo build -p liftassistant-headless --example sim-dev-server`.',
  );
}

Future<int> _allocateFreeLoopbackPort() async {
  final ServerSocket probe = await ServerSocket.bind(
    InternetAddress.loopbackIPv4,
    0,
  );
  final int port = probe.port;
  await probe.close();
  return port;
}

Future<void> _waitUntil(
  bool Function() predicate, {
  Duration timeout = const Duration(seconds: 8),
  required String Function() describeFailure,
}) async {
  final Stopwatch sw = Stopwatch()..start();
  while (!predicate()) {
    if (sw.elapsed > timeout) {
      fail('Timed out: ${describeFailure()}');
    }
    await Future<void>.delayed(const Duration(milliseconds: 25));
  }
}

/// Real loopback TCP fault-injection proxy that forwards a single
/// `POST /api/dev-sim/v0/serve-floor/submit` to the real Rust Simulator server,
/// waits until the server has sent back its HTTP response (proving server
/// acceptance), and then drops the client TCP socket without forwarding the
/// response bytes.
class _ResponseLossProxy {
  _ResponseLossProxy._(this._server, this._backendPort);

  final ServerSocket _server;
  final int _backendPort;
  bool dropNextSubmitResponse = false;
  int submitPostCount = 0;
  int submissionQueryGetCount = 0;

  int get port => _server.port;

  static Future<_ResponseLossProxy> start(int backendPort) async {
    final ServerSocket server = await ServerSocket.bind(
      InternetAddress.loopbackIPv4,
      0,
    );
    final _ResponseLossProxy proxy = _ResponseLossProxy._(server, backendPort);
    unawaited(proxy._acceptLoop());
    return proxy;
  }

  Future<void> _acceptLoop() async {
    await for (final Socket client in _server) {
      unawaited(_handleConnection(client));
    }
  }

  Future<void> _handleConnection(Socket client) async {
    final Socket backend;
    try {
      backend = await Socket.connect(
        InternetAddress.loopbackIPv4,
        _backendPort,
      );
    } catch (_) {
      client.destroy();
      return;
    }

    final BytesBuilder requestHeaderBuf = BytesBuilder(copy: false);
    bool inspectedRequest = false;
    bool dropThisResponse = false;

    final StreamSubscription<List<int>> backendSub = backend.listen(
      (List<int> data) {
        if (dropThisResponse) {
          // Server has genuinely accepted & replied; now intentionally swallow
          // the response and sever the client socket to prove response-loss recovery!
          client.destroy();
          backend.destroy();
          return;
        }
        try {
          client.add(data);
        } catch (_) {
          backend.destroy();
        }
      },
      onDone: () {
        if (!dropThisResponse) {
          unawaited(client.close());
        }
      },
      onError: (_) {
        client.destroy();
      },
    );

    client.listen(
      (List<int> data) {
        if (!inspectedRequest) {
          requestHeaderBuf.add(data);
          final String text = utf8.decode(
            requestHeaderBuf.toBytes(),
            allowMalformed: true,
          );
          if (text.contains('\r\n')) {
            inspectedRequest = true;
            final String firstLine = text.split('\r\n').first;
            if (firstLine.startsWith(
              'POST /api/dev-sim/v0/serve-floor/submit',
            )) {
              submitPostCount++;
              if (dropNextSubmitResponse) {
                dropNextSubmitResponse = false;
                dropThisResponse = true;
              }
            } else if (firstLine.startsWith(
              'GET /api/dev-sim/v0/submissions/',
            )) {
              submissionQueryGetCount++;
            }
          }
        }
        try {
          backend.add(data);
        } catch (_) {
          client.destroy();
        }
      },
      onDone: () {
        if (!dropThisResponse) {
          unawaited(backend.close());
        }
      },
      onError: (_) {
        unawaited(backendSub.cancel());
        backend.destroy();
      },
    );
  }

  Future<void> close() async {
    await _server.close();
  }
}

void main() {
  setUpAll(() {
    HttpOverrides.global = null;
  });

  test('real sim-dev-server end-to-end: preview -> submit -> Engine IntentId -> Simulator movement -> SSE -> Completed, response-loss recovery without resend, process restart honesty, and unavailable-car exclusion', () async {
    final String serverBin = _resolveSimDevServerPath();
    final int port = await _allocateFreeLoopbackPort();
    final _ResponseLossProxy proxy = await _ResponseLossProxy.start(port);
    addTearDown(proxy.close);

    Process? activeProcess;
    final StringBuffer logs = StringBuffer();
    Future<void> stopServer() async {
      final Process? proc = activeProcess;
      activeProcess = null;
      if (proc != null) {
        proc.kill(ProcessSignal.sigterm);
        await proc.exitCode.timeout(
          const Duration(seconds: 4),
          onTimeout: () {
            proc.kill(ProcessSignal.sigkill);
            return -1;
          },
        );
      }
    }

    addTearDown(stopServer);

    Future<void> startServer({String profile = 'default'}) async {
      final Process proc = await Process.start(serverBin, <String>[
        '--listen',
        '127.0.0.1:$port',
        '--enable-dev-sim-control',
        '--tick-ms',
        '40',
        '--profile',
        profile,
      ]);
      activeProcess = proc;
      proc.stdout
          .transform(utf8.decoder)
          .listen((String s) => logs.write('[sim:out] $s'));
      proc.stderr
          .transform(utf8.decoder)
          .listen((String s) => logs.write('[sim:err] $s'));
    }

    await startServer();

    final IoHostTransport transport = IoHostTransport(
      requestTimeout: const Duration(seconds: 3),
    );
    final ClientController controller = ClientController(
      transport: transport,
      retryBackoff: (_) => const Duration(milliseconds: 80),
    );
    addTearDown(() {
      controller.dispose();
      transport.dispose();
    });

    String debugState() =>
        'phase=${controller.state.phase}, rev=${controller.state.snapshot?.revision}, '
        'caps=${controller.state.controlCapabilities.runtimeType}, '
        'tracked=${controller.state.trackedOperation?.effectiveStatus}, '
        'intent=${controller.state.trackedOperation?.projection?.intentId}, '
        'reason=${controller.state.trackedOperation?.effectiveKnowledgeReason}, '
        'logs=$logs';

    // Connect through the loopback proxy.
    expect(
      await controller.connectToUrl('http://127.0.0.1:${proxy.port}'),
      isTrue,
    );
    await _waitUntil(
      () =>
          controller.state.phase == ConnectionPhase.live &&
          controller.state.controlCapabilities is ControlSurfaceAvailable,
      describeFailure: () =>
          'connect & discover dev-sim caps (${debugState()})',
    );

    final ControlSurfaceAvailable caps1 =
        controller.state.controlCapabilities! as ControlSurfaceAvailable;
    expect(caps1.developmentOnly, isTrue);
    expect(caps1.multiUserAuth, isFalse);
    expect(caps1.capabilityForCar('A')?.executableFloors, <int>[
      1,
      2,
      3,
      4,
      5,
      6,
      7,
      8,
    ]);
    // Car B floors strictly reflect ExecutionCapabilities (1..=6), not 8-floor topology.
    expect(caps1.capabilityForCar('B')?.executableFloors, <int>[
      1,
      2,
      3,
      4,
      5,
      6,
    ]);

    // 1. Advisory preview for Car B floor 7 (unsupported by Car B's ExecutionCapabilities) -> blocked.
    final ServeFloorPreview blockedB7 = await controller.previewServeFloor(
      car: 'B',
      floor: 7,
    );
    expect(blockedB7.advisory, isTrue);
    expect(blockedB7.eligible, isFalse);
    expect(blockedB7.blocker?.code, 'unsupported_floor');

    // 2. Full real ServeFloor workflow on Car A (3F -> 6F).
    final ServeFloorPreview previewA6 = await controller.previewServeFloor(
      car: 'A',
      floor: 6,
    );
    expect(previewA6.advisory, isTrue);
    expect(previewA6.eligible, isTrue);
    expect(previewA6.observedFloorReference, 3);

    final String key1 = controller.mintSubmissionKey();
    await controller.submitServeFloor(
      submissionKey: key1,
      car: 'A',
      floor: 6,
      expectedControlInstanceId: previewA6.controlInstanceId,
    );
    expect(proxy.submitPostCount, 1);
    expect(controller.state.trackedOperation?.projection?.intentId, 1);

    // Watch real Simulator movement over SSE until Engine reconciliation satisfies Completed.
    await _waitUntil(
      () =>
          controller.state.trackedOperation?.effectiveStatus ==
              ProjectedOperationStatus.completed &&
          controller.state.snapshot!.operationalCars
                  .firstWhere((OperationalCar c) => c.id == 'A')
                  .door ==
              CarDoorState.open,
      describeFailure: () => 'Car A completes 3F->6F (${debugState()})',
    );
    final OperationalCar carAAfter = controller.state.snapshot!.operationalCars
        .firstWhere((OperationalCar c) => c.id == 'A');
    expect(
      carAAfter.location,
      const ContinuousLocation(floorPosition: 6.0, nearestFloor: 6),
    );
    expect(carAAfter.motion, const StoppedMotion());
    expect(carAAfter.door, CarDoorState.open);
    expect(proxy.submitPostCount, 1);

    // 3. REAL RESPONSE-LOSS PROOF (Car B: 1F -> 4F):
    // Proxy forwards POST to sim-dev-server, waits for server acceptance, then drops response!
    final ServeFloorPreview previewB4 = await controller.previewServeFloor(
      car: 'B',
      floor: 4,
    );
    expect(previewB4.eligible, isTrue);
    final String key2 = controller.mintSubmissionKey();
    proxy.dropNextSubmitResponse = true;
    await controller.submitServeFloor(
      submissionKey: key2,
      car: 'B',
      floor: 4,
      expectedControlInstanceId: previewB4.controlInstanceId,
    );

    expect(proxy.submitPostCount, 2);
    expect(controller.state.trackedOperation?.responseLost, isTrue);
    expect(
      controller.state.trackedOperation?.effectiveStatus,
      ProjectedOperationStatus.knowledgeUnavailable,
    );
    expect(
      controller.state.trackedOperation?.effectiveKnowledgeReason,
      'response_lost',
    );

    // Reconnect -> client recovers the SAME accepted IntentId via read-only GET ONLY (NO resend!).
    await controller.reconnectNow();
    await _waitUntil(
      () =>
          controller.state.trackedOperation?.effectiveStatus ==
              ProjectedOperationStatus.completed &&
          controller.state.trackedOperation?.projection?.intentId != null,
      describeFailure: () =>
          'recover response-lost Car B submission via GET (${debugState()})',
    );
    expect(
      proxy.submitPostCount,
      2,
      reason: 'Lost response recovery MUST NOT resend POST',
    );
    expect(proxy.submissionQueryGetCount, greaterThanOrEqualTo(1));
    expect(controller.state.trackedOperation?.projection?.intentId, 2);

    // 4. REAL PROCESS RESTART PROOF (unavailable-car profile on fresh process #2):
    // Stop process #1 and launch fresh process #2 on the SAME port -> new control_instance_id.
    final String oldInstanceId = caps1.controlInstanceId;
    await stopServer();
    await _waitUntil(
      () => controller.state.phase == ConnectionPhase.reconnecting,
      describeFailure: () =>
          'enter reconnecting after server stop (${debugState()})',
    );

    // Start fresh process #2 with unavailable-car profile (Car B Operational revoked + Auxiliary active).
    await startServer(profile: 'unavailable-car');
    await _waitUntil(
      () =>
          controller.state.phase == ConnectionPhase.live &&
          controller.state.controlCapabilities is ControlSurfaceAvailable &&
          (controller.state.controlCapabilities! as ControlSurfaceAvailable)
                  .controlInstanceId !=
              oldInstanceId,
      describeFailure: () =>
          'reconnect to restarted server #2 (${debugState()})',
    );

    // Querying prior key2 against restarted server #2 MUST honestly report control_process_restarted!
    await controller.queryTrackedSubmission();
    expect(
      proxy.submitPostCount,
      2,
      reason: 'Process restart MUST NOT resend POST',
    );
    expect(
      controller.state.trackedOperation?.effectiveStatus,
      ProjectedOperationStatus.knowledgeUnavailable,
    );
    expect(
      controller.state.trackedOperation?.effectiveKnowledgeReason,
      'control_process_restarted',
    );

    // Verify configured-but-unavailable Car B (with active Auxiliary source) cannot preview/control
    // and leaks zero stale floor/mode!
    final ServeFloorPreview unavailPreviewB = await controller
        .previewServeFloor(car: 'B', floor: 3);
    expect(unavailPreviewB.eligible, isFalse);
    expect(unavailPreviewB.observedCarAvailable, isFalse);
    expect(unavailPreviewB.observedFloorReference, isNull);
    expect(unavailPreviewB.observedOperatingMode, isNull);
    expect(unavailPreviewB.blocker?.code, 'observation_unavailable');

    // Verify existing read API still rejects POST with 405 on the live sim-dev-server process.
    final HttpClient rawClient = HttpClient();
    try {
      final HttpClientRequest postRead = await rawClient.postUrl(
        Uri.parse('http://127.0.0.1:$port/api/experimental/v0/snapshot'),
      );
      final HttpClientResponse postReadResp = await postRead.close();
      await postReadResp.drain<void>();
      expect(postReadResp.statusCode, 405);
    } finally {
      rawClient.close(force: true);
    }

    await controller.disconnect();
    await stopServer();
  });
}
