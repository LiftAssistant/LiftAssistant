import 'dart:async';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:liftassistant_client/src/api/host_url.dart';
import 'package:liftassistant_client/src/transport/host_transport.dart';
import 'package:liftassistant_client/src/transport/host_transport_io.dart';

void main() {
  group('IoHostTransport against real loopback sockets', () {
    test('fetches snapshot JSON and rejects bad status or content-type', () async {
      final HttpServer server = await HttpServer.bind(
        InternetAddress.loopbackIPv4,
        0,
      );
      addTearDown(server.close);

      int mode = 0;
      server.listen((HttpRequest request) async {
        if (mode == 0) {
          request.response.statusCode = HttpStatus.ok;
          request.response.headers.contentType = ContentType.json;
          request.response.write('{"api_version":"experimental/v0"}');
        } else if (mode == 1) {
          request.response.statusCode = HttpStatus.serviceUnavailable;
          request.response.write('unavailable');
        } else {
          request.response.statusCode = HttpStatus.ok;
          request.response.headers.contentType = ContentType.text;
          request.response.write('plain text');
        }
        await request.response.close();
      });

      final HostEndpoint endpoint = HostEndpoint.parse(
        'http://127.0.0.1:${server.port}',
      );
      final IoHostTransport transport = IoHostTransport();
      addTearDown(transport.dispose);

      expect(
        await transport.fetchSnapshotJson(endpoint),
        '{"api_version":"experimental/v0"}',
      );

      mode = 1;
      await expectLater(
        transport.fetchSnapshotJson(endpoint),
        throwsA(
          isA<HostHttpStatusException>().having(
            (HostHttpStatusException e) => e.statusCode,
            'statusCode',
            503,
          ),
        ),
      );

      mode = 2;
      await expectLater(
        transport.fetchSnapshotJson(endpoint),
        throwsA(isA<UnexpectedContentTypeException>()),
      );
    });

    test('reports HostUnreachableException when port is closed', () async {
      final ServerSocket probe = await ServerSocket.bind(
        InternetAddress.loopbackIPv4,
        0,
      );
      final int closedPort = probe.port;
      await probe.close();

      final HostEndpoint endpoint = HostEndpoint.parse(
        'http://127.0.0.1:$closedPort',
      );
      final IoHostTransport transport = IoHostTransport(
        requestTimeout: const Duration(seconds: 2),
      );
      addTearDown(transport.dispose);

      await expectLater(
        transport.fetchSnapshotJson(endpoint),
        throwsA(isA<HostUnreachableException>()),
      );
    });

    test('streams SSE snapshots and surfaces disconnect when Host closes socket', () async {
      final HttpServer server = await HttpServer.bind(
        InternetAddress.loopbackIPv4,
        0,
      );
      addTearDown(() => server.close(force: true));

      server.listen((HttpRequest request) async {
        request.response.statusCode = HttpStatus.ok;
        request.response.bufferOutput = false;
        request.response.headers.set(
          HttpHeaders.contentTypeHeader,
          'text/event-stream',
        );
        request.response.write(': keepalive\n\n');
        await request.response.flush();
        request.response.write('event: snapshot\ndata: {"rev":1}\n\n');
        await request.response.flush();
        await Future<void>.delayed(const Duration(milliseconds: 30));
        request.response.write('event: snapshot\ndata: {"rev":2}\n\n');
        await request.response.flush();
        await Future<void>.delayed(const Duration(milliseconds: 30));
        await request.response.close();
      });

      final HostEndpoint endpoint = HostEndpoint.parse(
        'http://127.0.0.1:${server.port}',
      );
      final IoHostTransport transport = IoHostTransport();
      addTearDown(transport.dispose);

      final HostSseConnection sse = await transport.openSnapshotEvents(
        endpoint,
      );
      final List<String> received = <String>[];
      Object? terminalError;

      final Completer<void> done = Completer<void>();
      sse.snapshots.listen(
        received.add,
        onError: (Object error) {
          terminalError = error;
        },
        onDone: done.complete,
      );

      await done.future.timeout(const Duration(seconds: 3));
      expect(received, <String>['{"rev":1}', '{"rev":2}']);
      expect(terminalError, isA<HostStreamDisconnectedException>());
    });

    test('fires inactivity timeout when SSE stream stalls without keepalive', () async {
      final HttpServer server = await HttpServer.bind(
        InternetAddress.loopbackIPv4,
        0,
      );
      addTearDown(() => server.close(force: true));

      final List<HttpResponse> stalledResponses = <HttpResponse>[];
      server.listen((HttpRequest request) async {
        request.response.statusCode = HttpStatus.ok;
        request.response.bufferOutput = false;
        request.response.headers.set(
          HttpHeaders.contentTypeHeader,
          'text/event-stream',
        );
        request.response.write('event: snapshot\ndata: {"rev":1}\n\n');
        await request.response.flush();
        stalledResponses.add(request.response);
      });

      final HostEndpoint endpoint = HostEndpoint.parse(
        'http://127.0.0.1:${server.port}',
      );
      final IoHostTransport transport = IoHostTransport(
        streamInactivityTimeout: const Duration(milliseconds: 150),
      );
      addTearDown(() async {
        transport.dispose();
        for (final HttpResponse r in stalledResponses) {
          try {
            await r.close();
          } catch (_) {}
        }
      });

      final HostSseConnection sse = await transport.openSnapshotEvents(
        endpoint,
      );
      final List<String> received = <String>[];
      Object? terminalError;
      final Completer<void> done = Completer<void>();

      sse.snapshots.listen(
        received.add,
        onError: (Object error) {
          terminalError = error;
        },
        onDone: done.complete,
      );

      await done.future.timeout(const Duration(seconds: 3));
      expect(received, <String>['{"rev":1}']);
      expect(terminalError, isA<HostStreamDisconnectedException>());
    });
  });
}
