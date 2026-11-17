import 'dart:async';

import 'package:flutter/foundation.dart';

import '../api/host_url.dart';
import 'host_transport_io.dart'
    if (dart.library.js_interop) 'host_transport_web.dart'
    as platform_transport;

@immutable
sealed class HostTransportException implements Exception {
  const HostTransportException(this.message);

  final String message;

  @override
  String toString() => message;
}

@immutable
final class HostUnreachableException extends HostTransportException {
  const HostUnreachableException(
    super.message, {
    this.isCrossOriginWebFailure = false,
  });

  final bool isCrossOriginWebFailure;
}

@immutable
final class HostHttpStatusException extends HostTransportException {
  const HostHttpStatusException({
    required this.statusCode,
    required this.operation,
  }) : super('$operation endpoint returned HTTP $statusCode.');

  final int statusCode;
  final String operation;
}

@immutable
final class UnexpectedContentTypeException extends HostTransportException {
  const UnexpectedContentTypeException({
    required this.expectedPrefix,
    required this.actualContentType,
    required this.operation,
  }) : super(
          '$operation endpoint returned unexpected Content-Type "$actualContentType" (expected "$expectedPrefix").',
        );

  final String expectedPrefix;
  final String actualContentType;
  final String operation;
}

const int maxSnapshotBytes = 8 * 1024 * 1024;

@immutable
final class SnapshotTooLargeException extends HostTransportException {
  const SnapshotTooLargeException({required this.limitBytes})
      : super(
          'Snapshot response exceeded maximum bounded size ($limitBytes bytes).',
        );

  final int limitBytes;
}

@immutable
final class HostStreamDisconnectedException extends HostTransportException {
  const HostStreamDisconnectedException(super.message);
}

class HostSseConnection {
  HostSseConnection({
    required this.snapshots,
    required Future<void> Function() onClose,
  }) {
    _onClose = onClose;
  }

  final Stream<String> snapshots;
  late final Future<void> Function() _onClose;
  bool _closed = false;

  Future<void> close() async {
    if (_closed) {
      return;
    }
    _closed = true;
    await _onClose();
  }
}

abstract interface class HostTransport {
  /// Performs `GET /api/experimental/v0/snapshot` and returns the raw UTF-8 JSON body.
  Future<String> fetchSnapshotJson(HostEndpoint endpoint);

  /// Opens `GET /api/experimental/v0/events` and yields raw JSON strings from
  /// `event: snapshot` frames until closed or disconnected.
  Future<HostSseConnection> openSnapshotEvents(HostEndpoint endpoint);

  void dispose();
}

HostTransport createPlatformHostTransport({
  Duration requestTimeout = const Duration(seconds: 8),
  Duration streamInactivityTimeout = const Duration(seconds: 40),
  int maxPayloadBytes = maxSnapshotBytes,
}) {
  return platform_transport.createHostTransport(
    requestTimeout: requestTimeout,
    streamInactivityTimeout: streamInactivityTimeout,
    maxPayloadBytes: maxPayloadBytes,
  );
}
