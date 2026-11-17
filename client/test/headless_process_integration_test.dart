@TestOn('vm')
library;

import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:liftassistant_client/src/api/read_model.dart';
import 'package:liftassistant_client/src/state/client_controller.dart';
import 'package:liftassistant_client/src/state/client_state.dart';
import 'package:liftassistant_client/src/transport/host_transport_io.dart';

String _resolveHeadlessBinaryPath() {
  final File fromClientDir = File('../target/debug/liftassistant-headless');
  if (fromClientDir.existsSync()) {
    return fromClientDir.path;
  }
  final File fromRepoRoot = File('target/debug/liftassistant-headless');
  if (fromRepoRoot.existsSync()) {
    return fromRepoRoot.path;
  }
  throw StateError(
    'liftassistant-headless debug binary not found; build it with `cargo build -p liftassistant-headless`.',
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
  Duration timeout = const Duration(seconds: 6),
  required String Function() describeFailure,
}) async {
  final Stopwatch sw = Stopwatch()..start();
  while (!predicate()) {
    if (sw.elapsed > timeout) {
      fail('Timed out: ${describeFailure()}');
    }
    await Future<void>.delayed(const Duration(milliseconds: 20));
  }
}

void main() {
  setUpAll(() {
    // Allow real loopback sockets inside flutter_test.
    HttpOverrides.global = null;
  });

  test(
    'integrates end-to-end with real liftassistant-headless across SSE push, process stop, Host restart, and revision reset',
    () async {
      final String headlessBin = _resolveHeadlessBinaryPath();
      final int port = await _allocateFreeLoopbackPort();
      final String hostUrl = 'http://127.0.0.1:$port';

      Process? activeProcess;
      final StringBuffer hostLogs = StringBuffer();
      addTearDown(() async {
        final Process? proc = activeProcess;
        if (proc != null) {
          proc.kill(ProcessSignal.sigterm);
          await proc.exitCode.timeout(
            const Duration(seconds: 3),
            onTimeout: () {
              proc.kill(ProcessSignal.sigkill);
              return -1;
            },
          );
        }
      });

      final IoHostTransport transport = IoHostTransport(
        requestTimeout: const Duration(seconds: 2),
      );
      final ClientController controller = ClientController(
        transport: transport,
        retryBackoff: (_) => const Duration(milliseconds: 100),
      );
      addTearDown(() {
        controller.dispose();
        transport.dispose();
      });

      String debugState() =>
          'phase=${controller.state.phase}, error=${controller.state.error?.message}, '
          'rev=${controller.state.snapshot?.revision}, '
          'diags=${controller.state.snapshot?.recentDiagnostics.map((DiagnosticEntry d) => d.id).toList()}, '
          'logs=$hostLogs';

      // 1. Connect BEFORE Host is started -> client must survive and enter reconnecting.
      expect(await controller.connectToUrl(hostUrl), isTrue);
      await _waitUntil(
        () => controller.state.phase == ConnectionPhase.reconnecting,
        describeFailure: () =>
            'reconnecting while real Host is not yet started (${debugState()})',
      );
      expect(controller.state.snapshot, isNull);

      // 2. Start real liftassistant-headless --serve-experimental-snapshot-fixture.
      final Process fixtureHost = await Process.start(headlessBin, <String>[
        '--serve-experimental-snapshot-fixture',
        '127.0.0.1:$port',
      ]);
      activeProcess = fixtureHost;
      fixtureHost.stdout
          .transform(utf8.decoder)
          .listen((String s) => hostLogs.write('[fixture:out] $s'));
      fixtureHost.stderr
          .transform(utf8.decoder)
          .listen((String s) => hostLogs.write('[fixture:err] $s'));

      // 3. Client auto-reconnects, loads real snapshot, and receives pushed SSE diagnostic update (~300ms).
      await _waitUntil(
        () =>
            controller.state.phase == ConnectionPhase.live &&
            controller.state.snapshot != null &&
            controller.state.snapshot!.recentDiagnostics.any(
              (DiagnosticEntry d) => d.id == 'transport.receive-failed',
            ),
        describeFailure: () =>
            'live SSE snapshot push from real fixture Host (${debugState()})',
      );

      final ReadModelSnapshot fixtureSnapshot = controller.state.snapshot!;
      expect(fixtureSnapshot.apiVersion, 'experimental/v0');
      expect(fixtureSnapshot.lifecycle, HostLifecycle.readyReadOnly);
      expect(fixtureSnapshot.readiness, HostReadiness.readOnly);
      expect(fixtureSnapshot.operationalCars, hasLength(1));
      expect(fixtureSnapshot.operationalCars.single.id, 'lift-A');
      expect(
        fixtureSnapshot.operationalCars.single.location,
        const ContinuousLocation(floorPosition: 4.0, nearestFloor: 4),
      );
      final int fixtureFinalRevision = fixtureSnapshot.revision;
      expect(fixtureFinalRevision, greaterThanOrEqualTo(5));

      // 4. Stop the first real Host process -> client must survive and mark snapshot non-live.
      fixtureHost.kill(ProcessSignal.sigterm);
      final int fixtureExit = await fixtureHost.exitCode.timeout(
        const Duration(seconds: 5),
      );
      expect(fixtureExit, 0);
      activeProcess = null;

      await _waitUntil(
        () => controller.state.phase == ConnectionPhase.reconnecting,
        describeFailure: () =>
            'client enters reconnecting after real Host termination (${debugState()})',
      );
      expect(controller.state.hasRetainedStaleSnapshot, isTrue);
      expect(controller.state.isSnapshotLive, isFalse);

      // 5. Start a fresh real production RMF read-only Host on the SAME port (new lifetime, lower initial revision!).
      final Process rmfHost = await Process.start(headlessBin, <String>[
        '--run-rmf-read-only',
        '--domain-id',
        '230',
        '--source-id',
        'rmf/client-integration',
        '--max-silence-ms',
        '3000',
        '--floor',
        'L1=1',
        '--floor',
        'L2=2',
        '--bind',
        'Lift1=A:operational',
        '--bind',
        'Lift2=B:auxiliary',
        '--api-listen',
        '127.0.0.1:$port',
      ]);
      activeProcess = rmfHost;
      rmfHost.stdout
          .transform(utf8.decoder)
          .listen((String s) => hostLogs.write('[rmf:out] $s'));
      rmfHost.stderr
          .transform(utf8.decoder)
          .listen((String s) => hostLogs.write('[rmf:err] $s'));

      // 6. Verify client automatically reconnects to the restarted Host, accepts the restarted revision sequence,
      //    and replaces the old fixture state with the new RMF Host's Starting/NotReady state.
      await _waitUntil(
        () =>
            controller.state.phase == ConnectionPhase.live &&
            controller.state.snapshot != null &&
            controller.state.snapshot!.sources.any(
              (SourceHealth s) =>
                  s.source == 'rmf/client-integration' &&
                  s.transport == TransportState.subscriberCreated,
            ),
        describeFailure: () =>
            'automatic reconnect to restarted real RMF Host (${debugState()})',
      );

      final ReadModelSnapshot rmfSnapshot = controller.state.snapshot!;
      expect(rmfSnapshot.lifecycle, HostLifecycle.starting);
      expect(rmfSnapshot.readiness, HostReadiness.notReady);
      expect(rmfSnapshot.operationalCars, isEmpty);
      expect(rmfSnapshot.revision, lessThan(fixtureFinalRevision));
      expect(
        controller.state.lastRevisionReset?.previousRevision,
        fixtureFinalRevision,
      );
      expect(controller.state.isSnapshotLive, isTrue);

      final SourceHealth rmfSource = rmfSnapshot.sources.single;
      expect(rmfSource.source, 'rmf/client-integration');
      expect(rmfSource.session, isA<ActiveSourceSession>());
      expect(rmfSource.transport, TransportState.subscriberCreated);
      expect(rmfSource.payload, PayloadActivity.awaiting);
      expect(rmfSource.observations, hasLength(2));
      expect(
        rmfSource.observations
            .firstWhere((SourceObservation o) => o.car == 'A')
            .role,
        BindingRole.operational,
      );
      expect(
        rmfSource.observations
            .firstWhere((SourceObservation o) => o.car == 'B')
            .role,
        BindingRole.auxiliary,
      );

      // 7. Cleanly terminate the production RMF Host and disconnect client.
      await controller.disconnect();
      rmfHost.kill(ProcessSignal.sigterm);
      final int rmfExit = await rmfHost.exitCode.timeout(
        const Duration(seconds: 5),
      );
      expect(rmfExit, 0);
      activeProcess = null;
    },
  );
}
