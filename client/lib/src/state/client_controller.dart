import 'dart:async';
import 'dart:convert';
import 'dart:math';

import 'package:flutter/foundation.dart';

import '../api/control_model.dart';
import '../api/control_model_decoder.dart';
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

String defaultGenerateSubmissionKey() {
  final Random rng = Random.secure();
  final StringBuffer buffer = StringBuffer('sub-');
  for (int i = 0; i < 16; i++) {
    buffer.write(rng.nextInt(256).toRadixString(16).padLeft(2, '0'));
  }
  return buffer.toString();
}

@immutable
final class ControlOperationException implements Exception {
  const ControlOperationException(this.message);

  final String message;

  @override
  String toString() => message;
}

class ClientController extends ChangeNotifier {
  ClientController({
    HostTransport? transport,
    DevControlTransport? controlTransport,
    String initialUrlInput = '',
    Duration Function(int attempt)? retryBackoff,
    DateTime Function()? clock,
    String Function()? submissionKeyGenerator,
  }) : _transport = transport ?? createPlatformHostTransport(),
       _ownsTransport = transport == null,
       _retryBackoff = retryBackoff ?? defaultReconnectBackoff,
       _clock = clock ?? DateTime.now,
       _submissionKeyGenerator =
           submissionKeyGenerator ?? defaultGenerateSubmissionKey,
       _state = ClientState.initial(initialUrlInput: initialUrlInput) {
    if (controlTransport != null) {
      _controlTransport = controlTransport;
    } else if (_transport is DevControlTransport) {
      _controlTransport = _transport as DevControlTransport;
    } else {
      _controlTransport = null;
    }
  }

  final HostTransport _transport;
  late final DevControlTransport? _controlTransport;
  final bool _ownsTransport;
  final Duration Function(int attempt) _retryBackoff;
  final DateTime Function() _clock;
  final String Function() _submissionKeyGenerator;

  ClientState _state;
  int _operationId = 0;
  Timer? _retryTimer;
  HostSseConnection? _activeSse;
  StreamSubscription<String>? _sseSubscription;
  bool _disposed = false;

  ClientState get state => _state;

  /// Generates a fresh opaque client-side correlation key BEFORE submission.
  /// This is submission correlation only — never a canonical Engine `IntentId`.
  String mintSubmissionKey() => _submissionKeyGenerator();

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
        controlCapabilities: switchingHost ? null : _state.controlCapabilities,
        trackedOperation: switchingHost ? null : _state.trackedOperation,
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
      _handleConnectionFailure(opId, endpoint, error, attempt: attempt + 1);
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
    unawaited(_onConnectedControlSync(opId, endpoint));

    bool receivedSseSnapshot = false;
    _sseSubscription = sseConnection.snapshots.listen(
      (String rawEventJson) {
        if (_disposed || opId != _operationId) {
          return;
        }
        final ReadModelSnapshot nextSnapshot;
        try {
          nextSnapshot = ExperimentalV0Decoder.decodeJson(rawEventJson);
        } catch (decodeError) {
          unawaited(
            _abortSseOnError(
              opId,
              endpoint,
              decodeError,
              nextAttempt: receivedSseSnapshot ? 1 : attempt + 1,
            ),
          );
          return;
        }
        receivedSseSnapshot = true;
        _applyIncomingSnapshot(nextSnapshot, phase: ConnectionPhase.live);
      },
      onError: (Object streamError) {
        if (_disposed || opId != _operationId) {
          return;
        }
        unawaited(
          _abortSseOnError(
            opId,
            endpoint,
            streamError,
            nextAttempt: receivedSseSnapshot ? 1 : attempt + 1,
          ),
        );
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
            nextAttempt: receivedSseSnapshot ? 1 : attempt + 1,
          ),
        );
      },
      cancelOnError: false,
    );
  }

  Future<void> _abortSseOnError(
    int opId,
    HostEndpoint endpoint,
    Object error, {
    required int nextAttempt,
  }) async {
    if (_disposed || opId != _operationId) {
      return;
    }
    _retryTimer?.cancel();
    _retryTimer = null;
    final StreamSubscription<String>? sub = _sseSubscription;
    _sseSubscription = null;
    final HostSseConnection? sse = _activeSse;
    _activeSse = null;

    _handleConnectionFailure(opId, endpoint, error, attempt: nextAttempt);

    if (sub != null) {
      await sub.cancel();
    }
    if (sse != null) {
      await sse.close();
    }
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

    if (phase == ConnectionPhase.live &&
        _controlTransport != null &&
        _state.trackedOperation != null &&
        _state.trackedOperation!.shouldAutoQueryOnLiveUpdate) {
      unawaited(queryTrackedSubmission());
    }
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
    ) = _classifyFailure(
      error,
    );

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

    if (error is SnapshotTooLargeException) {
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
      final bool isTransientStatus = code >= 500 || code == 408 || code == 429;
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

  Future<void> _onConnectedControlSync(int opId, HostEndpoint endpoint) async {
    if (_disposed || opId != _operationId || _controlTransport == null) {
      return;
    }
    await _refreshCapabilitiesInternal(opId, endpoint);
    if (_disposed || opId != _operationId) {
      return;
    }
    final TrackedServeFloorOperation? tracked = _state.trackedOperation;
    if (tracked != null && tracked.shouldAutoQueryOnLiveUpdate) {
      await queryTrackedSubmission();
    }
  }

  /// Refreshes read-only dev-sim control capability projection (`GET /api/dev-sim/v0/capabilities`).
  Future<void> refreshControlCapabilities() async {
    final HostEndpoint? endpoint = _state.endpoint;
    if (_disposed || endpoint == null || _controlTransport == null) {
      return;
    }
    await _refreshCapabilitiesInternal(_operationId, endpoint);
  }

  Future<void> _refreshCapabilitiesInternal(
    int opId,
    HostEndpoint endpoint,
  ) async {
    final DevControlTransport? control = _controlTransport;
    if (control == null || _disposed || opId != _operationId) {
      return;
    }
    try {
      final ControlHttpResponse response = await control
          .fetchControlCapabilities(endpoint);
      if (_disposed || opId != _operationId || _state.endpoint != endpoint) {
        return;
      }
      if (response.statusCode == 404) {
        _setState(
          _state.copyWith(controlCapabilities: const ControlSurfaceAbsent()),
        );
        return;
      }
      if (response.statusCode < 200 || response.statusCode >= 300) {
        _setState(
          _state.copyWith(
            controlCapabilities: ControlSurfaceSupportUnknown(
              reason:
                  'Control capability endpoint returned HTTP ${response.statusCode}.',
            ),
          ),
        );
        return;
      }
      if (!response.contentType.toLowerCase().startsWith('application/json')) {
        _setState(
          _state.copyWith(
            controlCapabilities: ControlSurfaceSupportUnknown(
              reason:
                  'Control capability endpoint returned unexpected Content-Type "${response.contentType}".',
            ),
          ),
        );
        return;
      }
      final ControlSurfaceAvailable parsed =
          DevControlV0Decoder.decodeCapabilitiesJson(response.body);
      _setState(_state.copyWith(controlCapabilities: parsed));
    } catch (error) {
      if (_disposed || opId != _operationId || _state.endpoint != endpoint) {
        return;
      }
      _setState(
        _state.copyWith(
          controlCapabilities: ControlSurfaceSupportUnknown(reason: '$error'),
        ),
      );
    }
  }

  /// Requests server-owned advisory preview for a candidate ServeFloor transaction.
  /// Never issues an IntentId or produces a side effect.
  Future<ServeFloorPreview> previewServeFloor({
    required String car,
    required int floor,
  }) async {
    final HostEndpoint? endpoint = _state.endpoint;
    final DevControlTransport? control = _controlTransport;
    if (endpoint == null || control == null) {
      throw const ControlOperationException(
        'No control-capable Host endpoint is currently configured.',
      );
    }
    final String body = jsonEncode(<String, Object>{
      'operation': 'serve_floor',
      'car': car,
      'floor': floor,
    });
    final ControlHttpResponse response;
    try {
      response = await control.postServeFloorPreview(endpoint, body);
    } catch (error) {
      throw ControlOperationException('Preview request failed: $error');
    }
    if (response.statusCode == 404) {
      throw const ControlOperationException(
        'Connected Host does not expose a ServeFloor control surface.',
      );
    }
    if (!response.contentType.toLowerCase().startsWith('application/json')) {
      throw ControlOperationException(
        'Preview returned unexpected Content-Type "${response.contentType}" (HTTP ${response.statusCode}).',
      );
    }
    if (response.statusCode >= 200 && response.statusCode < 300) {
      try {
        return DevControlV0Decoder.decodePreviewJson(response.body);
      } on ControlDecodeException catch (error) {
        throw ControlOperationException(error.message);
      }
    }
    try {
      final ControlErrorPayload err = DevControlV0Decoder.decodeErrorJson(
        response.body,
      );
      throw ControlOperationException('${err.errorCode}: ${err.message}');
    } on ControlDecodeException {
      throw ControlOperationException(
        'Preview rejected with HTTP ${response.statusCode}.',
      );
    }
  }

  /// Submits one immutable ServeFloor transaction bound to a client-generated
  /// [submissionKey] created BEFORE any possible side effect.
  ///
  /// Never automatically resends on transport error or response loss; instead
  /// marks the operation `responseLost` (`Unknown`) so reconnect uses read-only
  /// `GET /api/dev-sim/v0/submissions/{submissionKey}` only.
  Future<void> submitServeFloor({
    required String submissionKey,
    required String car,
    required int floor,
    String? expectedControlInstanceId,
  }) async {
    final HostEndpoint? endpoint = _state.endpoint;
    final DevControlTransport? control = _controlTransport;
    if (_disposed || endpoint == null || control == null) {
      return;
    }

    // Hard UI-layer duplicate prevention: never POST twice for the same attempted key.
    final TrackedServeFloorOperation? existing = _state.trackedOperation;
    if (existing != null &&
        existing.submissionKey == submissionKey &&
        existing.submissionAttempted) {
      return;
    }

    final String? instanceId =
        expectedControlInstanceId ??
        switch (_state.controlCapabilities) {
          ControlSurfaceAvailable(:final controlInstanceId) =>
            controlInstanceId,
          _ => null,
        };

    final int opId = _operationId;
    final TrackedServeFloorOperation pending = TrackedServeFloorOperation(
      submissionKey: submissionKey,
      car: car,
      floor: floor,
      expectedControlInstanceId: instanceId,
      submissionAttempted: true,
      submitInFlight: true,
    );
    _setState(_state.copyWith(trackedOperation: pending));

    final String body = jsonEncode(<String, Object>{
      'submission_key': submissionKey,
      'operation': 'serve_floor',
      'car': car,
      'floor': floor,
    });

    final ControlHttpResponse response;
    try {
      response = await control.postServeFloorSubmit(endpoint, body);
    } catch (error) {
      if (_disposed ||
          _state.endpoint != endpoint ||
          _state.trackedOperation?.submissionKey != submissionKey) {
        return;
      }
      // Response lost / network interrupted after possible server acceptance:
      // first-class Unknown, NEVER automatic resend.
      _setState(
        _state.copyWith(
          trackedOperation: _state.trackedOperation!.copyWith(
            submitInFlight: false,
            responseLost: true,
            lastQueryError: '$error',
          ),
        ),
      );
      return;
    }

    if (_disposed ||
        _state.endpoint != endpoint ||
        _state.trackedOperation?.submissionKey != submissionKey) {
      return;
    }

    if (response.statusCode == 409) {
      String message =
          'Submission key "$submissionKey" conflicts with an existing immutable request.';
      if (response.contentType.toLowerCase().startsWith('application/json')) {
        try {
          final ControlErrorPayload err = DevControlV0Decoder.decodeErrorJson(
            response.body,
          );
          message = err.message;
        } on ControlDecodeException {
          // Retain clear default message.
        }
      }
      _setState(
        _state.copyWith(
          trackedOperation: _state.trackedOperation!.copyWith(
            submitInFlight: false,
            responseLost: false,
            conflictMessage: message,
            clearLastQueryError: true,
          ),
        ),
      );
      return;
    }

    if (response.statusCode == 400 || response.statusCode == 422) {
      String message =
          'Submission rejected by schema validation (HTTP ${response.statusCode}).';
      if (response.contentType.toLowerCase().startsWith('application/json')) {
        try {
          final ControlErrorPayload err = DevControlV0Decoder.decodeErrorJson(
            response.body,
          );
          message = '${err.errorCode}: ${err.message}';
        } on ControlDecodeException {
          // Retain clear default message.
        }
      }
      _setState(
        _state.copyWith(
          trackedOperation: _state.trackedOperation!.copyWith(
            submitInFlight: false,
            responseLost: false,
            validationErrorMessage: message,
            clearLastQueryError: true,
          ),
        ),
      );
      return;
    }

    if (response.statusCode < 200 || response.statusCode >= 300) {
      // 5xx or ambiguous server error after dispatch: treat as uncertain response loss, never resend.
      _setState(
        _state.copyWith(
          trackedOperation: _state.trackedOperation!.copyWith(
            submitInFlight: false,
            responseLost: true,
            lastQueryError:
                'Submit endpoint returned HTTP ${response.statusCode}; outcome uncertain.',
          ),
        ),
      );
      return;
    }

    if (!response.contentType.toLowerCase().startsWith('application/json')) {
      _setState(
        _state.copyWith(
          trackedOperation: _state.trackedOperation!.copyWith(
            submitInFlight: false,
            responseLost: true,
            lastQueryError:
                'Submit returned unexpected Content-Type "${response.contentType}".',
          ),
        ),
      );
      return;
    }

    try {
      final ProjectedSubmission projected =
          DevControlV0Decoder.decodeSubmissionJson(response.body);
      _setState(
        _state.copyWith(
          trackedOperation: _state.trackedOperation!.copyWith(
            expectedControlInstanceId:
                _state.trackedOperation!.expectedControlInstanceId ??
                projected.controlInstanceId,
            submitInFlight: false,
            responseLost: false,
            projection: projected,
            clearConflictMessage: true,
            clearValidationErrorMessage: true,
            clearLastQueryError: true,
          ),
        ),
      );
      if (opId == _operationId) {
        unawaited(_refreshCapabilitiesInternal(opId, endpoint));
      }
    } on ControlDecodeException catch (error) {
      _setState(
        _state.copyWith(
          trackedOperation: _state.trackedOperation!.copyWith(
            submitInFlight: false,
            responseLost: true,
            lastQueryError: error.message,
          ),
        ),
      );
    }
  }

  /// Queries latest server-projected knowledge for the tracked submission key via
  /// read-only `GET /api/dev-sim/v0/submissions/{submissionKey}`. Never resends.
  Future<void> queryTrackedSubmission() async {
    final HostEndpoint? endpoint = _state.endpoint;
    final DevControlTransport? control = _controlTransport;
    final TrackedServeFloorOperation? tracked = _state.trackedOperation;
    if (_disposed ||
        endpoint == null ||
        control == null ||
        tracked == null ||
        !tracked.submissionAttempted ||
        tracked.submitInFlight ||
        tracked.queryInFlight) {
      return;
    }

    final String key = tracked.submissionKey;
    final String? expectedInstanceId = tracked.expectedControlInstanceId;
    final int opId = _operationId;
    _setState(
      _state.copyWith(
        trackedOperation: tracked.copyWith(
          queryInFlight: true,
          clearLastQueryError: true,
        ),
      ),
    );

    final ControlHttpResponse response;
    try {
      response = await control.fetchSubmissionStatus(
        endpoint,
        key,
        expectedInstanceId: expectedInstanceId,
      );
    } catch (error) {
      if (_disposed ||
          _state.endpoint != endpoint ||
          _state.trackedOperation?.submissionKey != key) {
        return;
      }
      _setState(
        _state.copyWith(
          trackedOperation: _state.trackedOperation!.copyWith(
            queryInFlight: false,
            lastQueryError: 'Status query failed: $error',
          ),
        ),
      );
      return;
    }

    if (_disposed ||
        _state.endpoint != endpoint ||
        _state.trackedOperation?.submissionKey != key) {
      return;
    }

    if (response.statusCode == 404) {
      _setState(
        _state.copyWith(
          trackedOperation: _state.trackedOperation!.copyWith(
            queryInFlight: false,
            lastQueryError: 'Control surface is unavailable on the connected Host (HTTP 404). Prior submission knowledge unavailable.',
          ),
        ),
      );
      return;
    }

    if (response.statusCode < 200 ||
        response.statusCode >= 300 ||
        !response.contentType.toLowerCase().startsWith('application/json')) {
      _setState(
        _state.copyWith(
          trackedOperation: _state.trackedOperation!.copyWith(
            queryInFlight: false,
            lastQueryError:
                'Status query returned HTTP ${response.statusCode}.',
          ),
        ),
      );
      return;
    }

    try {
      ProjectedSubmission projected = DevControlV0Decoder.decodeSubmissionJson(
        response.body,
      );
      if (expectedInstanceId != null &&
          expectedInstanceId.isNotEmpty &&
          projected.controlInstanceId != expectedInstanceId) {
        projected = ProjectedSubmission(
          controlApiVersion: projected.controlApiVersion,
          developmentOnly: projected.developmentOnly,
          controlInstanceId: projected.controlInstanceId,
          ephemeralProcessMemory: projected.ephemeralProcessMemory,
          submissionKey: projected.submissionKey,
          operation: projected.operation,
          status: ProjectedOperationStatus.knowledgeUnavailable,
          duplicateSubmission: false,
          sawProgress: false,
          isActiveInEngine: false,
          knowledgeReason: 'control_process_restarted',
        );
      }

      final ProjectedOperationStatus? previousStatus =
          _state.trackedOperation!.projection?.status;
      _setState(
        _state.copyWith(
          trackedOperation: _state.trackedOperation!.copyWith(
            queryInFlight: false,
            responseLost: false,
            projection: projected,
            clearLastQueryError: true,
          ),
        ),
      );
      if (opId == _operationId && previousStatus != projected.status) {
        unawaited(_refreshCapabilitiesInternal(opId, endpoint));
      }
    } on ControlDecodeException catch (error) {
      _setState(
        _state.copyWith(
          trackedOperation: _state.trackedOperation!.copyWith(
            queryInFlight: false,
            lastQueryError: error.message,
          ),
        ),
      );
    }
  }

  void dismissTrackedOperation() {
    if (_state.trackedOperation == null) {
      return;
    }
    _setState(_state.copyWith(clearTrackedOperation: true));
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
