import 'package:flutter/foundation.dart';

import '../api/control_model.dart';
import '../api/host_url.dart';
import '../api/read_model.dart';

enum ConnectionPhase {
  disconnected,
  connecting,
  attachingStream,
  live,
  reconnecting,
  incompatibleApi,
  protocolError,
}

enum ClientErrorKind {
  invalidUrl,
  unreachable,
  crossOriginBlockedOrUnreachable,
  streamDisconnected,
  unsupportedApiVersion,
  malformedResponse,
  httpStatusError,
  unexpectedContentType,
}

@immutable
final class ClientConnectionError {
  const ClientConnectionError({
    required this.kind,
    required this.message,
    this.unsupportedVersion,
    this.httpStatusCode,
  });

  final ClientErrorKind kind;
  final String message;
  final String? unsupportedVersion;
  final int? httpStatusCode;

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      (other is ClientConnectionError &&
          other.kind == kind &&
          other.message == message &&
          other.unsupportedVersion == unsupportedVersion &&
          other.httpStatusCode == httpStatusCode);

  @override
  int get hashCode =>
      Object.hash(kind, message, unsupportedVersion, httpStatusCode);
}

@immutable
final class RevisionResetNotice {
  const RevisionResetNotice({
    required this.previousRevision,
    required this.newRevision,
    required this.observedAt,
  });

  final int previousRevision;
  final int newRevision;
  final DateTime observedAt;

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      (other is RevisionResetNotice &&
          other.previousRevision == previousRevision &&
          other.newRevision == newRevision &&
          other.observedAt == observedAt);

  @override
  int get hashCode => Object.hash(previousRevision, newRevision, observedAt);
}

/// Client-side tracking of one ServeFloor submission correlation attempt.
///
/// The client never owns completion truth or operation lifecycle: [projection] is
/// strictly server-projected Engine state queried by [submissionKey].
@immutable
final class TrackedServeFloorOperation {
  const TrackedServeFloorOperation({
    required this.submissionKey,
    required this.car,
    required this.floor,
    this.expectedControlInstanceId,
    required this.submissionAttempted,
    this.submitInFlight = false,
    this.queryInFlight = false,
    this.responseLost = false,
    this.localKnowledgeReason,
    this.conflictMessage,
    this.validationErrorMessage,
    this.projection,
    this.lastQueryError,
  });

  final String submissionKey;
  final String car;
  final int floor;
  final String? expectedControlInstanceId;
  final bool submissionAttempted;
  final bool submitInFlight;
  final bool queryInFlight;
  final bool responseLost;
  final String? localKnowledgeReason;
  final String? conflictMessage;
  final String? validationErrorMessage;
  final ProjectedSubmission? projection;
  final String? lastQueryError;

  bool get isProcessRestarted {
    final ProjectedSubmission? current = projection;
    if (current == null) {
      return false;
    }
    if (current.knowledgeReason == 'control_process_restarted') {
      return true;
    }
    if (expectedControlInstanceId != null &&
        expectedControlInstanceId!.isNotEmpty &&
        current.controlInstanceId != expectedControlInstanceId) {
      return true;
    }
    return false;
  }

  ProjectedOperationStatus get effectiveStatus {
    if (localKnowledgeReason != null || isProcessRestarted) {
      return ProjectedOperationStatus.knowledgeUnavailable;
    }
    final ProjectedSubmission? current = projection;
    if (current != null) {
      return current.status;
    }
    return ProjectedOperationStatus.knowledgeUnavailable;
  }

  String? get effectiveKnowledgeReason {
    if (localKnowledgeReason != null) {
      return localKnowledgeReason;
    }
    if (isProcessRestarted) {
      return 'control_process_restarted';
    }
    final ProjectedSubmission? current = projection;
    if (current != null) {
      return current.knowledgeReason;
    }
    if (responseLost) {
      return 'response_lost';
    }
    return null;
  }

  /// Whether a live SSE update warrants a read-only GET submission query once
  /// no query is already in flight. Never triggers resend of POST.
  bool get needsAutoQueryIfIdle {
    if (!submissionAttempted ||
        submitInFlight ||
        localKnowledgeReason != null ||
        conflictMessage != null ||
        validationErrorMessage != null ||
        isProcessRestarted) {
      return false;
    }
    if (responseLost || projection == null) {
      return true;
    }
    final ProjectedSubmission current = projection!;
    return current.status == ProjectedOperationStatus.accepted ||
        current.status == ProjectedOperationStatus.inProgress ||
        (current.status == ProjectedOperationStatus.knowledgeUnavailable &&
            current.knowledgeReason == 'delivery_or_receipt_indeterminate');
  }

  bool get shouldAutoQueryOnLiveUpdate =>
      !queryInFlight && needsAutoQueryIfIdle;

  TrackedServeFloorOperation copyWith({
    bool? submissionAttempted,
    bool? submitInFlight,
    bool? queryInFlight,
    bool? responseLost,
    String? localKnowledgeReason,
    bool clearLocalKnowledgeReason = false,
    String? conflictMessage,
    String? expectedControlInstanceId,
    bool clearConflictMessage = false,
    String? validationErrorMessage,
    bool clearValidationErrorMessage = false,
    ProjectedSubmission? projection,
    bool clearProjection = false,
    String? lastQueryError,
    bool clearLastQueryError = false,
  }) {
    return TrackedServeFloorOperation(
      submissionKey: submissionKey,
      car: car,
      floor: floor,
      expectedControlInstanceId:
          expectedControlInstanceId ?? this.expectedControlInstanceId,
      submissionAttempted: submissionAttempted ?? this.submissionAttempted,
      submitInFlight: submitInFlight ?? this.submitInFlight,
      queryInFlight: queryInFlight ?? this.queryInFlight,
      responseLost: responseLost ?? this.responseLost,
      localKnowledgeReason: clearLocalKnowledgeReason
          ? null
          : (localKnowledgeReason ?? this.localKnowledgeReason),
      conflictMessage: clearConflictMessage
          ? null
          : (conflictMessage ?? this.conflictMessage),
      validationErrorMessage: clearValidationErrorMessage
          ? null
          : (validationErrorMessage ?? this.validationErrorMessage),
      projection: clearProjection ? null : (projection ?? this.projection),
      lastQueryError: clearLastQueryError
          ? null
          : (lastQueryError ?? this.lastQueryError),
    );
  }
}

@immutable
final class ClientState {
  const ClientState({
    required this.rawUrlInput,
    required this.phase,
    this.endpoint,
    this.snapshot,
    this.lastUpdatedAt,
    this.error,
    this.reconnectAttempt = 0,
    this.nextRetryDelay,
    this.snapshotsReceived = 0,
    this.lastRevisionReset,
    this.controlCapabilities,
    this.trackedOperation,
  });

  factory ClientState.initial({String initialUrlInput = ''}) {
    return ClientState(
      rawUrlInput: initialUrlInput,
      phase: ConnectionPhase.disconnected,
    );
  }

  final String rawUrlInput;
  final HostEndpoint? endpoint;
  final ConnectionPhase phase;
  final ReadModelSnapshot? snapshot;
  final DateTime? lastUpdatedAt;
  final ClientConnectionError? error;
  final int reconnectAttempt;
  final Duration? nextRetryDelay;
  final int snapshotsReceived;
  final RevisionResetNotice? lastRevisionReset;
  final ControlSurfaceCapabilities? controlCapabilities;
  final TrackedServeFloorOperation? trackedOperation;

  /// True only while actively synchronized with the target Host.
  /// When reconnecting or disconnected, any retained [snapshot] is historical
  /// local context and must never be presented as live Host truth.
  bool get isSnapshotLive =>
      snapshot != null &&
      (phase == ConnectionPhase.live ||
          phase == ConnectionPhase.attachingStream);

  bool get hasRetainedStaleSnapshot => snapshot != null && !isSnapshotLive;

  bool get isActivelyManaged =>
      phase == ConnectionPhase.connecting ||
      phase == ConnectionPhase.attachingStream ||
      phase == ConnectionPhase.live ||
      phase == ConnectionPhase.reconnecting;

  ClientState copyWith({
    String? rawUrlInput,
    HostEndpoint? endpoint,
    bool clearEndpoint = false,
    ConnectionPhase? phase,
    ReadModelSnapshot? snapshot,
    bool clearSnapshot = false,
    DateTime? lastUpdatedAt,
    bool clearLastUpdatedAt = false,
    ClientConnectionError? error,
    bool clearError = false,
    int? reconnectAttempt,
    Duration? nextRetryDelay,
    bool clearNextRetryDelay = false,
    int? snapshotsReceived,
    RevisionResetNotice? lastRevisionReset,
    bool clearLastRevisionReset = false,
    ControlSurfaceCapabilities? controlCapabilities,
    bool clearControlCapabilities = false,
    TrackedServeFloorOperation? trackedOperation,
    bool clearTrackedOperation = false,
  }) {
    return ClientState(
      rawUrlInput: rawUrlInput ?? this.rawUrlInput,
      endpoint: clearEndpoint ? null : (endpoint ?? this.endpoint),
      phase: phase ?? this.phase,
      snapshot: clearSnapshot ? null : (snapshot ?? this.snapshot),
      lastUpdatedAt: clearLastUpdatedAt
          ? null
          : (lastUpdatedAt ?? this.lastUpdatedAt),
      error: clearError ? null : (error ?? this.error),
      reconnectAttempt: reconnectAttempt ?? this.reconnectAttempt,
      nextRetryDelay: clearNextRetryDelay
          ? null
          : (nextRetryDelay ?? this.nextRetryDelay),
      snapshotsReceived: snapshotsReceived ?? this.snapshotsReceived,
      lastRevisionReset: clearLastRevisionReset
          ? null
          : (lastRevisionReset ?? this.lastRevisionReset),
      controlCapabilities: clearControlCapabilities
          ? null
          : (controlCapabilities ?? this.controlCapabilities),
      trackedOperation: clearTrackedOperation
          ? null
          : (trackedOperation ?? this.trackedOperation),
    );
  }
}
