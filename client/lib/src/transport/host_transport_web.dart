import 'dart:async';
import 'dart:convert';
import 'dart:js_interop';
import 'dart:typed_data';

import '../api/host_url.dart';
import 'host_transport.dart';
import 'sse_parser.dart';

@JS('fetch')
external JSPromise<_JSResponse> _jsFetch(JSString url, _JSRequestInit init);

@JS('AbortController')
extension type _JSAbortController._(JSObject _) implements JSObject {
  external factory _JSAbortController();
  external JSObject get signal;
  external void abort();
}

@JS('Headers')
extension type _JSHeaders._(JSObject _) implements JSObject {
  external factory _JSHeaders();
  external void set(JSString name, JSString value);
  external JSString? get(JSString name);
}

extension type _JSRequestInit._(JSObject _) implements JSObject {
  external factory _JSRequestInit({
    JSString method,
    JSObject headers,
    JSString cache,
    JSString redirect,
    JSObject signal,
  });
}

extension type _JSResponse._(JSObject _) implements JSObject {
  external int get status;
  external _JSHeaders get headers;
  external _JSReadableStream? get body;
  external JSPromise<JSString> text();
}

extension type _JSReadableStream._(JSObject _) implements JSObject {
  external _JSReadableStreamDefaultReader getReader();
}

extension type _JSReadableStreamDefaultReader._(JSObject _)
    implements JSObject {
  external JSPromise<_JSReadResult> read();
  external JSPromise<JSAny?> cancel();
}

extension type _JSReadResult._(JSObject _) implements JSObject {
  external bool get done;
  external JSUint8Array? get value;
}

HostTransport createHostTransport({
  required Duration requestTimeout,
  required Duration streamInactivityTimeout,
  int maxPayloadBytes = maxSnapshotBytes,
}) {
  return WebHostTransport(
    requestTimeout: requestTimeout,
    streamInactivityTimeout: streamInactivityTimeout,
    maxPayloadBytes: maxPayloadBytes,
  );
}

final class WebHostTransport implements HostTransport {
  WebHostTransport({
    this.requestTimeout = const Duration(seconds: 8),
    this.streamInactivityTimeout = const Duration(seconds: 40),
    this.maxPayloadBytes = maxSnapshotBytes,
  }) : assert(maxPayloadBytes > 0, 'maxPayloadBytes must be positive');

  final Duration requestTimeout;
  final Duration streamInactivityTimeout;
  final int maxPayloadBytes;
  final Set<_JSAbortController> _activeControllers = <_JSAbortController>{};
  bool _disposed = false;

  bool _isCrossOrigin(HostEndpoint endpoint) {
    final Uri pageUri = Uri.base;
    if (pageUri.scheme != 'http' && pageUri.scheme != 'https') {
      return false;
    }
    return pageUri.origin != endpoint.origin;
  }

  HostUnreachableException _mapFetchError(
    HostEndpoint endpoint,
    Object error,
  ) {
    if (_isCrossOrigin(endpoint)) {
      return HostUnreachableException(
        'Browser could not reach ${endpoint.origin} from web origin ${Uri.base.origin}. '
        'Either the Host is offline, or browser Same-Origin Policy blocked cross-origin access '
        '(LiftAssistant Host intentionally omits wildcard CORS on unauthenticated HTTP).',
        isCrossOriginWebFailure: true,
      );
    }
    return HostUnreachableException(
      'Cannot reach LiftAssistant Host at ${endpoint.origin}: $error',
    );
  }

  @override
  Future<String> fetchSnapshotJson(HostEndpoint endpoint) async {
    if (_disposed) {
      throw const HostUnreachableException('Transport has been disposed.');
    }
    final _JSAbortController abortController = _JSAbortController();
    _activeControllers.add(abortController);
    final Timer timeoutTimer = Timer(
      requestTimeout,
      () => abortController.abort(),
    );

    try {
      final _JSHeaders headers = _JSHeaders()
        ..set('Accept'.toJS, 'application/json'.toJS);
      final _JSResponse response = await _jsFetch(
        endpoint.snapshotUri.toString().toJS,
        _JSRequestInit(
          method: 'GET'.toJS,
          headers: headers,
          cache: 'no-store'.toJS,
          redirect: 'error'.toJS,
          signal: abortController.signal,
        ),
      ).toDart.timeout(requestTimeout);

      if (response.status < 200 || response.status >= 300) {
        throw HostHttpStatusException(
          statusCode: response.status,
          operation: 'Snapshot',
        );
      }

      final String contentType =
          response.headers.get('content-type'.toJS)?.toDart ?? '';
      if (!contentType.toLowerCase().startsWith('application/json')) {
        throw UnexpectedContentTypeException(
          expectedPrefix: 'application/json',
          actualContentType: contentType,
          operation: 'Snapshot',
        );
      }

      final String? contentLengthRaw =
          response.headers.get('content-length'.toJS)?.toDart;
      final int? contentLength =
          contentLengthRaw == null ? null : int.tryParse(contentLengthRaw);
      if (contentLength != null && contentLength > maxPayloadBytes) {
        abortController.abort();
        throw SnapshotTooLargeException(limitBytes: maxPayloadBytes);
      }

      final _JSReadableStream? body = response.body;
      if (body != null) {
        final _JSReadableStreamDefaultReader reader = body.getReader();
        final BytesBuilder builder = BytesBuilder(copy: false);
        try {
          await (() async {
            while (true) {
              final _JSReadResult chunk = await reader.read().toDart;
              if (chunk.done) {
                break;
              }
              final JSUint8Array? bytes = chunk.value;
              if (bytes != null) {
                final Uint8List dartBytes = bytes.toDart;
                if (builder.length + dartBytes.length > maxPayloadBytes) {
                  abortController.abort();
                  unawaited(
                    reader.cancel().toDart.then<void>((_) {}, onError: (_) {}),
                  );
                  throw SnapshotTooLargeException(limitBytes: maxPayloadBytes);
                }
                if (dartBytes.isNotEmpty) {
                  builder.add(dartBytes);
                }
              }
            }
          })().timeout(requestTimeout);
          return utf8.decode(builder.takeBytes());
        } on FormatException catch (error) {
          throw HostUnreachableException(
            'Invalid UTF-8 response from ${endpoint.origin}: ${error.message}',
          );
        }
      }

      final JSString bodyText =
          await response.text().toDart.timeout(requestTimeout);
      final String dartText = bodyText.toDart;
      if (utf8.encode(dartText).length > maxPayloadBytes) {
        throw SnapshotTooLargeException(limitBytes: maxPayloadBytes);
      }
      return dartText;
    } on HostTransportException {
      rethrow;
    } on TimeoutException {
      abortController.abort();
      throw HostUnreachableException(
        'Timed out connecting to LiftAssistant Host at ${endpoint.origin}.',
      );
    } catch (error) {
      throw _mapFetchError(endpoint, error);
    } finally {
      timeoutTimer.cancel();
      _activeControllers.remove(abortController);
    }
  }

  @override
  Future<HostSseConnection> openSnapshotEvents(HostEndpoint endpoint) async {
    if (_disposed) {
      throw const HostUnreachableException('Transport has been disposed.');
    }
    final _JSAbortController abortController = _JSAbortController();
    _activeControllers.add(abortController);
    final Timer connectTimer = Timer(
      requestTimeout,
      () => abortController.abort(),
    );

    final _JSResponse response;
    try {
      final _JSHeaders headers = _JSHeaders()
        ..set('Accept'.toJS, 'text/event-stream'.toJS);
      response = await _jsFetch(
        endpoint.eventsUri.toString().toJS,
        _JSRequestInit(
          method: 'GET'.toJS,
          headers: headers,
          cache: 'no-store'.toJS,
          redirect: 'error'.toJS,
          signal: abortController.signal,
        ),
      ).toDart.timeout(requestTimeout);
    } on TimeoutException {
      abortController.abort();
      _activeControllers.remove(abortController);
      throw HostUnreachableException(
        'Timed out opening SSE stream from ${endpoint.origin}.',
      );
    } catch (error) {
      _activeControllers.remove(abortController);
      throw _mapFetchError(endpoint, error);
    } finally {
      connectTimer.cancel();
    }

    if (response.status < 200 || response.status >= 300) {
      abortController.abort();
      _activeControllers.remove(abortController);
      throw HostHttpStatusException(
        statusCode: response.status,
        operation: 'SSE',
      );
    }

    final String contentType =
        response.headers.get('content-type'.toJS)?.toDart ?? '';
    if (!contentType.toLowerCase().startsWith('text/event-stream')) {
      abortController.abort();
      _activeControllers.remove(abortController);
      throw UnexpectedContentTypeException(
        expectedPrefix: 'text/event-stream',
        actualContentType: contentType,
        operation: 'SSE',
      );
    }

    final _JSReadableStream? body = response.body;
    if (body == null) {
      abortController.abort();
      _activeControllers.remove(abortController);
      throw const HostStreamDisconnectedException(
        'Browser fetch response did not expose a ReadableStream body.',
      );
    }

    final _JSReadableStreamDefaultReader reader = body.getReader();
    final StreamController<List<int>> byteController =
        StreamController<List<int>>();
    late final StreamController<String> outputController;
    StreamSubscription<String>? parsedSubscription;
    Timer? watchdog;
    bool closedLocally = false;

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
        abortController.abort();
        _activeControllers.remove(abortController);
        unawaited(
          reader.cancel().toDart.then<void>((_) {}, onError: (_) {}),
        );
        unawaited(parsedSubscription?.cancel());
        if (!byteController.isClosed) {
          unawaited(byteController.close());
        }
        unawaited(outputController.close());
      });
    }

    Future<void> pumpReader() async {
      try {
        while (!closedLocally) {
          final _JSReadResult chunk = await reader.read().toDart;
          if (chunk.done) {
            break;
          }
          final JSUint8Array? bytes = chunk.value;
          if (bytes != null &&
              !closedLocally &&
              !byteController.isClosed &&
              byteController.hasListener) {
            final Uint8List dartBytes = bytes.toDart;
            if (dartBytes.isNotEmpty) {
              byteController.add(dartBytes);
            }
          }
        }
        if (!byteController.isClosed) {
          await byteController.close();
        }
      } catch (error, stackTrace) {
        if (!closedLocally && !byteController.isClosed) {
          byteController.addError(error, stackTrace);
          await byteController.close();
        }
      }
    }

    Future<void> closeConnection() async {
      if (closedLocally) {
        return;
      }
      closedLocally = true;
      watchdog?.cancel();
      abortController.abort();
      _activeControllers.remove(abortController);
      try {
        await reader.cancel().toDart;
      } catch (_) {}
      await parsedSubscription?.cancel();
      if (!byteController.isClosed) {
        if (byteController.hasListener) {
          await byteController.close();
        } else {
          unawaited(byteController.close());
        }
      }
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
        parsedSubscription = parseSseSnapshotStream(
          byteController.stream,
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
            abortController.abort();
            _activeControllers.remove(abortController);
            unawaited(
              reader.cancel().toDart.then<void>((_) {}, onError: (_) {}),
            );
            unawaited(parsedSubscription?.cancel());
            if (!byteController.isClosed) {
              unawaited(byteController.close());
            }
            outputController.addError(
              error is HostTransportException
                  ? error
                  : HostStreamDisconnectedException(
                      'SSE stream interrupted: $error',
                    ),
              stackTrace,
            );
            unawaited(outputController.close());
          },
          onDone: () {
            watchdog?.cancel();
            _activeControllers.remove(abortController);
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
        unawaited(pumpReader());
      },
      onCancel: closeConnection,
    );

    return HostSseConnection(
      snapshots: outputController.stream,
      onClose: closeConnection,
    );
  }

  @override
  void dispose() {
    _disposed = true;
    for (final _JSAbortController controller in _activeControllers.toList()) {
      controller.abort();
    }
    _activeControllers.clear();
  }
}
