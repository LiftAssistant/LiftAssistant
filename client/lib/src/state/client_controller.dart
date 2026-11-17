import 'dart:async';

import 'package:flutter/foundation.dart';

import '../api/host_url.dart';
import '../api/read_model.dart';
import '../api/read_model_decoder.dart';
import '../transport/host_transport.dart';
import 'client_state.dart';

Duration defaultReconnectBackoff(int attempt) {
  final int clampedAttempt = attempt.clamp(1, 4);
  final int seconds = 1 << (clampedAttempt - 1); // 1s, 2s, 4s, 8s
  return Duration(seconds: seconds);
}

class ClientController extends ChangeNotifier {
  ClientController({
    HostTransport? transport,
    String initialUrlInput = '',
    Duration Function(int attempt)? retryBackoff,
    DateTime Function()? clock,
  }) : _transport = transport ?? createPlatformHostTransport(),
       _ownsTransport = transport == null,
       _retryBackoff = retryBackoff ?? defaultReconnectBackoff,
       _clock = clock ?? DateTime.now,
       _state = ClientState.initial(initialUrlInput: initialUrlInput);

  final HostTransport _transport;
  final bool _ownsTransport;
  final Duration Function(int attempt) _retryBackoff;
  final DateTime Function() _clock;

  ClientState _state;
  int _operationId = 0;
  Timer? _retryTimer;
  HostSseConnection? _activeSse;
  StreamSubscription<String>? _sseSubscription;
  bool _disposed = false;

  ClientState get state => _state;

  void _setState(ClientState next) {
    if (_disposed) {
      return;
    }
    _state = next;
    notifyListeners();
  }

  /// Validates [rawUrl] and initiates complete-snapshot synchronization
  /// followed by continuous SSE streaming.
  Future<bool> connectToUrl(String rawUrl) async {
    final HostEndpoint parsedEndpoint;
    try {
      parsedEndpoint = HostEndpoint.parse(rawUrl);
    } on InvalidHostUrlException catch (error) {
      await _cancelActiveOperations();
      _setState(
        _state.copyWith(
          rawUrlInput: rawUrl,
          phase: ConnectionPhase.disconnected,
          error: ClientConnectionError(
            kind: ClientErrorKind.invalidUrl,
            message: error.message,
          ),
          reconnectAttempt: 0,
          clearNextRetryDelay: true,
        ),
      );
      return false;
    }

    final bool switchingHost = _state.endpoint != parsedEndpoint;
    await _cancelActiveOperations();
    final int opId = ++_operationId;

    _setState(
      ClientState(
        rawUrlInput: parsedEndpoint.origin,
        endpoint: parsedEndpoint,
        phase: ConnectionPhase.connecting,
        snapshot: switchingHost ? null : _state.snapshot,
        lastUpdatedAt: switchingHost ? null : _state.lastUpdatedAt,
        snapshotsReceived: switchingHost ? 0 : _state.snapshotsReceived,
        lastRevisionReset: switchingHost ? null : _state.lastRevisionReset,
      ),
    );

    unawaited(_establishAndStream(opId, parsedEndpoint, attempt: 0));
    return true;
  }

  /// Immediately retries connecting to the currently configured [HostEndpoint].
  Future<void> reconnectNow() async {
    final HostEndpoint? endpoint = _state.endpoint;
    if (endpoint == null) {
      if (_state.rawUrlInput.trim().isNotEmpty) {
        await connectToUrl(_state.rawUrlInput);
      }
      return;
    }
    await _cancelActiveOperations();
    final int opId = ++_operationId;
    _setState(
      _state.copyWith(
        phase: ConnectionPhase.connecting,
        clearError: true,
        reconnectAttempt: 0,
        clearNextRetryDelay: true,
      ),
    );
    unawaited(_establishAndStream(opId, endpoint, attempt: 0));
  }

  /// Stops active synchronization and closes any open SSE stream.
  Future<void> disconnect() async {
    await _cancelActiveOperations();
    _operationId++;
    _setState(
      _state.copyWith(
        phase: ConnectionPhase.disconnected,
        clearError: true,
        reconnectAttempt: 0,
        clearNextRetryDelay: true,
      ),
    );
  }

  Future<void> _cancelActiveOperations() async {
    _retryTimer?.cancel();
    _retryTimer = null;

    final StreamSubscription<String>? sub = _sseSubscription;
    _sseSubscription = null;
    if (sub != null) {
      await sub.cancel();
    }

    final HostSseConnection? sse = _activeSse;
    _activeSse = null;
    if (sse != null) {
      await sse.close();
    }
  }

  Future<void> _establishAndStream(
    int opId,
    HostEndpoint endpoint, {
    required int attempt,
  }) async {
    if (_disposed || opId != _operationId) {
      return;
    }

    // Step 1: Obtain complete current snapshot via GET /api/experimental/v0/snapshot.
    final ReadModelSnapshot initialSnapshot;
    try {
      final String rawSnapshotJson = await _transport.fetchSnapshotJson(
        endpoint,
      );
      if (_disposed || opId != _operationId) {
        return;
      }
      initialSnapshot = ExperimentalV0Decoder.decodeJson(rawSnapshotJson);
    } catch (error) {
      if (_disposed || opId != _operationId) {
        return;
      }
      _handleConnectionFailure(opId, endpoint, error, attempt: attempt + 1);
      return;
    }

    _applyIncomingSnapshot(
      initialSnapshot,
      phase: ConnectionPhase.attachingStream,
    );

    // Step 2: Open continuous SSE stream via GET /api/experimental/v0/events.
    final HostSseConnection sseConnection;
    try {
      sseConnection = await _transport.openSnapshotEvents(endpoint);
    } catch (error) {
      if (_disposed || opId != _operationId) {
        return;
      }
      _handleConnectionFailure(opId, endpoint, error, attempt: 1);
      return;
    }

    if (_disposed || opId != _operationId) {
      await sseConnection.close();
      return;
    }

    _activeSse = sseConnection;
    _setState(
      _state.copyWith(
        phase: ConnectionPhase.live,
        clearError: true,
        reconnectAttempt: 0,
        clearNextRetryDelay: true,
      ),
    );

    _sseSubscription = sseConnection.snapshots.listen(
      (String rawEventJson) {
        if (_disposed || opId != _operationId) {
          return;
        }
        final ReadModelSnapshot nextSnapshot;
        try {
          nextSnapshot = ExperimentalV0Decoder.decodeJson(rawEventJson);
        } catch (decodeError) {
          unawaited(_abortSseOnError(opId, endpoint, decodeError));
          return;
        }
        _applyIncomingSnapshot(nextSnapshot, phase: ConnectionPhase.live);
      },
      onError: (Object streamError) {
        if (_disposed || opId != _operationId) {
          return;
        }
        unawaited(_abortSseOnError(opId, endpoint, streamError));
      },
      onDone: () {
        if (_disposed || opId != _operationId) {
          return;
        }
        unawaited(
          _abortSseOnError(
            opId,
            endpoint,
            const HostStreamDisconnectedException(
              'SSE stream ended unexpectedly.',
            ),
          ),
        );
      },
      cancelOnError: false,
    );
  }

  Future<void> _abortSseOnError(
    int opId,
    HostEndpoint endpoint,
    Object error,
  ) async {
    if (_disposed || opId != _operationId) {
      return;
    }
    await _cancelActiveOperations();
    if (_disposed || opId != _operationId) {
      return;
    }
    _handleConnectionFailure(opId, endpoint, error, attempt: 1);
  }

  void _applyIncomingSnapshot(
    ReadModelSnapshot incoming, {
    required ConnectionPhase phase,
  }) {
    final ReadModelSnapshot? previous = _state.snapshot;
    RevisionResetNotice? resetNotice = _state.lastRevisionReset;

    // Host revision is monotonic only within a single Host process lifetime.
    // A smaller revision indicates a new Host process lifetime, NOT corruption.
    if (previous != null && incoming.revision < previous.revision) {
      resetNotice = RevisionResetNotice(
        previousRevision: previous.revision,
        newRevision: incoming.revision,
        observedAt: _clock(),
      );
    }

    _setState(
      _state.copyWith(
        phase: phase,
        snapshot: incoming,
        lastUpdatedAt: _clock(),
        clearError: true,
        reconnectAttempt: 0,
        clearNextRetryDelay: true,
        snapshotsReceived: _state.snapshotsReceived + 1,
        lastRevisionReset: resetNotice,
      ),
    );
  }

  void _handleConnectionFailure(
    int opId,
    HostEndpoint endpoint,
    Object error, {
    required int attempt,
  }) {
    final (
      ConnectionPhase targetPhase,
      ClientConnectionError structuredError,
      bool shouldAutoRetry,
    ) = _classifyFailure(error);

    if (!shouldAutoRetry) {
      _setState(
        _state.copyWith(
          phase: targetPhase,
          error: structuredError,
          reconnectAttempt: attempt,
          clearNextRetryDelay: true,
        ),
      );
      return;
    }

    final Duration delay = _retryBackoff(attempt);
    _setState(
      _state.copyWith(
        phase: ConnectionPhase.reconnecting,
        error: structuredError,
        reconnectAttempt: attempt,
        nextRetryDelay: delay,
      ),
    );

    _retryTimer?.cancel();
    _retryTimer = Timer(delay, () {
      if (_disposed || opId != _operationId) {
        return;
      }
      _setState(
        _state.copyWith(
          phase: ConnectionPhase.connecting,
          clearNextRetryDelay: true,
        ),
      );
      unawaited(_establishAndStream(opId, endpoint, attempt: attempt));
    });
  }

  (ConnectionPhase, ClientConnectionError, bool) _classifyFailure(
    Object error,
  ) {
    if (error is UnsupportedApiVersionException) {
      return (
        ConnectionPhase.incompatibleApi,
        ClientConnectionError(
          kind: ClientErrorKind.unsupportedApiVersion,
          message: error.message,
          unsupportedVersion: error.actualVersion,
        ),
        false,
      );
    }

    if (error is MalformedReadModelException) {
      return (
        ConnectionPhase.protocolError,
        ClientConnectionError(
          kind: ClientErrorKind.malformedResponse,
          message: error.message,
        ),
        false,
      );
    }

    if (error is UnexpectedContentTypeException) {
      return (
        ConnectionPhase.protocolError,
        ClientConnectionError(
          kind: ClientErrorKind.unexpectedContentType,
          message: error.message,
        ),
        false,
      );
    }

    if (error is HostHttpStatusException) {
      final int code = error.statusCode;
      final bool isTransientStatus =
          code >= 500 || code == 408 || code == 429;
      return (
        isTransientStatus
            ? ConnectionPhase.reconnecting
            : ConnectionPhase.protocolError,
        ClientConnectionError(
          kind: ClientErrorKind.httpStatusError,
          message: error.message,
          httpStatusCode: code,
        ),
        isTransientStatus,
      );
    }

    if (error is HostStreamDisconnectedException) {
      return (
        ConnectionPhase.reconnecting,
        ClientConnectionError(
          kind: ClientErrorKind.streamDisconnected,
          message: error.message,
        ),
        true,
      );
    }

    if (error is HostUnreachableException) {
      return (
        ConnectionPhase.reconnecting,
        ClientConnectionError(
          kind: error.isCrossOriginWebFailure
              ? ClientErrorKind.crossOriginBlockedOrUnreachable
              : ClientErrorKind.unreachable,
          message: error.message,
        ),
        true,
      );
    }

    return (
      ConnectionPhase.reconnecting,
      ClientConnectionError(
        kind: ClientErrorKind.unreachable,
        message: 'Connection error: $error',
      ),
      true,
    );
  }

  @override
  void dispose() {
    _disposed = true;
    _operationId++;
    _retryTimer?.cancel();
    _retryTimer = null;
    unawaited(_sseSubscription?.cancel());
    _sseSubscription = null;
    unawaited(_activeSse?.close());
    _activeSse = null;
    if (_ownsTransport) {
      _transport.dispose();
    }
    super.dispose();
  }
}
