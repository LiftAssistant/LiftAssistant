import 'package:flutter/foundation.dart';

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
  int get hashCode =>
      Object.hash(previousRevision, newRevision, observedAt);
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
    );
  }
}
