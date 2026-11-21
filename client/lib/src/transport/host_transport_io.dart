import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import '../api/host_url.dart';
import 'host_transport.dart';
import 'sse_parser.dart';

HostTransport createHostTransport({
  required Duration requestTimeout,
  required Duration streamInactivityTimeout,
  int maxPayloadBytes = maxSnapshotBytes,
}) {
  return IoHostTransport(
    requestTimeout: requestTimeout,
    streamInactivityTimeout: streamInactivityTimeout,
    maxPayloadBytes: maxPayloadBytes,
  );
}

final class IoHostTransport implements HostTransport, DevControlTransport {
  IoHostTransport({
    this.requestTimeout = const Duration(seconds: 8),
    this.streamInactivityTimeout = const Duration(seconds: 40),
    this.maxPayloadBytes = maxSnapshotBytes,
  }) : assert(maxPayloadBytes > 0, 'maxPayloadBytes must be positive');

  final Duration requestTimeout;
  final Duration streamInactivityTimeout;
  final int maxPayloadBytes;
  final Set<HttpClient> _activeClients = <HttpClient>{};
  bool _disposed = false;

  HttpClient _newClient() {
    final HttpClient client = HttpClient()
      ..connectionTimeout = requestTimeout
      ..idleTimeout = const Duration(seconds: 15);
    _activeClients.add(client);
    return client;
  }

  void _releaseClient(HttpClient client, {bool force = false}) {
    _activeClients.remove(client);
    client.close(force: force);
  }

  @override
  Future<String> fetchSnapshotJson(HostEndpoint endpoint) async {
    if (_disposed) {
      throw const HostUnreachableException('Transport has been disposed.');
    }
    final HttpClient client = _newClient();
    try {
      final HttpClientRequest request = await client
          .getUrl(endpoint.snapshotUri)
          .timeout(requestTimeout);
      request.followRedirects = false;
      request.headers.set(HttpHeaders.acceptHeader, 'application/json');
      request.headers.set(HttpHeaders.cacheControlHeader, 'no-cache');

      final HttpClientResponse response = await request.close().timeout(
        requestTimeout,
      );

      if (response.statusCode < 200 || response.statusCode >= 300) {
        await response.drain<void>();
        throw HostHttpStatusException(
          statusCode: response.statusCode,
          operation: 'Snapshot',
        );
      }

      final String contentType =
          response.headers.value(HttpHeaders.contentTypeHeader) ?? '';
      if (!contentType.toLowerCase().startsWith('application/json')) {
        await response.drain<void>();
        throw UnexpectedContentTypeException(
          expectedPrefix: 'application/json',
          actualContentType: contentType,
          operation: 'Snapshot',
        );
      }

      if (response.contentLength > maxPayloadBytes) {
        throw SnapshotTooLargeException(limitBytes: maxPayloadBytes);
      }

      final BytesBuilder builder = BytesBuilder(copy: false);
      await (() async {
        await for (final List<int> chunk in response) {
          if (builder.length + chunk.length > maxPayloadBytes) {
            throw SnapshotTooLargeException(limitBytes: maxPayloadBytes);
          }
          builder.add(chunk);
        }
      })().timeout(requestTimeout);

      return utf8.decode(builder.takeBytes());
    } on HostTransportException {
      rethrow;
    } on TimeoutException {
      throw HostUnreachableException(
        'Timed out connecting to LiftAssistant Host at ${endpoint.origin}.',
      );
    } on SocketException catch (error) {
      throw HostUnreachableException(
        'Cannot reach LiftAssistant Host at ${endpoint.origin}: ${error.message}',
      );
    } on HttpException catch (error) {
      throw HostUnreachableException(
        'HTTP error communicating with ${endpoint.origin}: ${error.message}',
      );
    } on FormatException catch (error) {
      throw HostUnreachableException(
        'Invalid UTF-8 response from ${endpoint.origin}: ${error.message}',
      );
    } finally {
      _releaseClient(client, force: true);
    }
  }

  @override
  Future<HostSseConnection> openSnapshotEvents(HostEndpoint endpoint) async {
    if (_disposed) {
      throw const HostUnreachableException('Transport has been disposed.');
    }
    final HttpClient client = _newClient();
    final HttpClientResponse response;
    try {
      final HttpClientRequest request = await client
          .getUrl(endpoint.eventsUri)
          .timeout(requestTimeout);
      request.followRedirects = false;
      request.headers.set(HttpHeaders.acceptHeader, 'text/event-stream');
      request.headers.set(HttpHeaders.cacheControlHeader, 'no-cache');

      response = await request.close().timeout(requestTimeout);
    } on TimeoutException {
      _releaseClient(client, force: true);
      throw HostUnreachableException(
        'Timed out opening SSE stream from ${endpoint.origin}.',
      );
    } on SocketException catch (error) {
      _releaseClient(client, force: true);
      throw HostUnreachableException(
        'Cannot open SSE stream from ${endpoint.origin}: ${error.message}',
      );
    } on HttpException catch (error) {
      _releaseClient(client, force: true);
      throw HostUnreachableException(
        'HTTP error opening SSE stream from ${endpoint.origin}: ${error.message}',
      );
    }

    if (response.statusCode < 200 || response.statusCode >= 300) {
      _releaseClient(client, force: true);
      throw HostHttpStatusException(
        statusCode: response.statusCode,
        operation: 'SSE',
      );
    }

    final String contentType =
        response.headers.value(HttpHeaders.contentTypeHeader) ?? '';
    if (!contentType.toLowerCase().startsWith('text/event-stream')) {
      _releaseClient(client, force: true);
      throw UnexpectedContentTypeException(
        expectedPrefix: 'text/event-stream',
        actualContentType: contentType,
        operation: 'SSE',
      );
    }

    bool closedLocally = false;
    Timer? watchdog;
    late final StreamController<String> outputController;
    StreamSubscription<String>? parsedSubscription;

    void resetWatchdog() {
      watchdog?.cancel();
      watchdog = Timer(streamInactivityTimeout, () {
        if (closedLocally || outputController.isClosed) {
          return;
        }
        closedLocally = true;
        outputController.addError(
          const HostStreamDisconnectedException(
            'SSE stream timed out waiting for Host activity or keepalive.',
          ),
        );
        unawaited(parsedSubscription?.cancel());
        _releaseClient(client, force: true);
        unawaited(outputController.close());
      });
    }

    Future<void> closeConnection() async {
      if (closedLocally) {
        return;
      }
      closedLocally = true;
      watchdog?.cancel();
      await parsedSubscription?.cancel();
      _releaseClient(client, force: true);
      if (!outputController.isClosed) {
        if (outputController.hasListener) {
          await outputController.close();
        } else {
          unawaited(outputController.close());
        }
      }
    }

    outputController = StreamController<String>(
      onListen: () {
        resetWatchdog();
        parsedSubscription =
            parseSseSnapshotStream(
              response,
              onByteActivity: resetWatchdog,
              maxEventBytes: maxPayloadBytes,
            ).listen(
              (String payload) {
                if (!outputController.isClosed) {
                  outputController.add(payload);
                }
              },
              onError: (Object error, StackTrace stackTrace) {
                if (closedLocally || outputController.isClosed) {
                  return;
                }
                closedLocally = true;
                watchdog?.cancel();
                unawaited(parsedSubscription?.cancel());
                outputController.addError(
                  error is HostTransportException
                      ? error
                      : HostStreamDisconnectedException(
                          'SSE stream interrupted: $error',
                        ),
                  stackTrace,
                );
                _releaseClient(client, force: true);
                unawaited(outputController.close());
              },
              onDone: () {
                watchdog?.cancel();
                _releaseClient(client, force: true);
                if (!closedLocally && !outputController.isClosed) {
                  outputController.addError(
                    const HostStreamDisconnectedException(
                      'Host closed the SSE stream.',
                    ),
                  );
                }
                if (!outputController.isClosed) {
                  unawaited(outputController.close());
                }
              },
              cancelOnError: false,
            );
      },
      onCancel: closeConnection,
    );

    return HostSseConnection(
      snapshots: outputController.stream,
      onClose: closeConnection,
    );
  }

  Future<ControlHttpResponse> _sendControlRequest(
    HostEndpoint endpoint, {
    required String method,
    required Uri uri,
    String? jsonBody,
  }) async {
    if (_disposed) {
      throw const HostUnreachableException('Transport has been disposed.');
    }
    final int byteLimit = maxPayloadBytes < maxControlResponseBytes
        ? maxPayloadBytes
        : maxControlResponseBytes;
    final HttpClient client = _newClient();
    try {
      final HttpClientRequest request = await client
          .openUrl(method, uri)
          .timeout(requestTimeout);
      request.followRedirects = false;
      request.headers.set(HttpHeaders.acceptHeader, 'application/json');
      request.headers.set(HttpHeaders.cacheControlHeader, 'no-cache');
      if (jsonBody != null) {
        final List<int> bodyBytes = utf8.encode(jsonBody);
        request.headers.set(
          HttpHeaders.contentTypeHeader,
          'application/json; charset=utf-8',
        );
        request.contentLength = bodyBytes.length;
        request.add(bodyBytes);
      }

      final HttpClientResponse response = await request.close().timeout(
        requestTimeout,
      );

      final int status = response.statusCode;
      if (status >= 300 && status < 400) {
        throw HostHttpStatusException(
          statusCode: status,
          operation: 'Control $method',
        );
      }

      final String contentType =
          response.headers.value(HttpHeaders.contentTypeHeader) ?? '';

      if (response.contentLength > byteLimit) {
        throw SnapshotTooLargeException(limitBytes: byteLimit);
      }

      final BytesBuilder builder = BytesBuilder(copy: false);
      await (() async {
        await for (final List<int> chunk in response) {
          if (builder.length + chunk.length > byteLimit) {
            throw SnapshotTooLargeException(limitBytes: byteLimit);
          }
          builder.add(chunk);
        }
      })().timeout(requestTimeout);

      return ControlHttpResponse(
        statusCode: status,
        body: utf8.decode(builder.takeBytes()),
        contentType: contentType,
      );
    } on HostTransportException {
      rethrow;
    } on TimeoutException {
      throw HostUnreachableException(
        'Timed out communicating with control surface at ${endpoint.origin}.',
      );
    } on SocketException catch (error) {
      throw HostUnreachableException(
        'Cannot reach control surface at ${endpoint.origin}: ${error.message}',
      );
    } on HttpException catch (error) {
      throw HostUnreachableException(
        'HTTP error communicating with control surface at ${endpoint.origin}: ${error.message}',
      );
    } on FormatException catch (error) {
      throw HostUnreachableException(
        'Invalid UTF-8 control response from ${endpoint.origin}: ${error.message}',
      );
    } finally {
      _releaseClient(client, force: true);
    }
  }

  @override
  Future<ControlHttpResponse> fetchControlCapabilities(HostEndpoint endpoint) {
    return _sendControlRequest(
      endpoint,
      method: 'GET',
      uri: endpoint.devControlCapabilitiesUri,
    );
  }

  @override
  Future<ControlHttpResponse> postServeFloorPreview(
    HostEndpoint endpoint,
    String jsonBody,
  ) {
    return _sendControlRequest(
      endpoint,
      method: 'POST',
      uri: endpoint.devControlServeFloorPreviewUri,
      jsonBody: jsonBody,
    );
  }

  @override
  Future<ControlHttpResponse> postServeFloorSubmit(
    HostEndpoint endpoint,
    String jsonBody,
  ) {
    return _sendControlRequest(
      endpoint,
      method: 'POST',
      uri: endpoint.devControlServeFloorSubmitUri,
      jsonBody: jsonBody,
    );
  }

  @override
  Future<ControlHttpResponse> fetchSubmissionStatus(
    HostEndpoint endpoint,
    String submissionKey, {
    String? expectedInstanceId,
  }) {
    return _sendControlRequest(
      endpoint,
      method: 'GET',
      uri: endpoint.devControlSubmissionUri(
        submissionKey,
        expectedInstanceId: expectedInstanceId,
      ),
    );
  }

  @override
  void dispose() {
    _disposed = true;
    for (final HttpClient client in _activeClients.toList()) {
      client.close(force: true);
    }
    _activeClients.clear();
  }
}
