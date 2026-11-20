import 'package:flutter/foundation.dart';

import 'read_model.dart';

/// Distinct capability support axis (`supported != available != authorized != eligible`).
enum OperationSupport {
  unknown('unknown'),
  unsupported('unsupported'),
  supported('supported');

  const OperationSupport(this.wireValue);
  final String wireValue;
}

/// Distinct control-path availability axis.
enum ControlPathAvailability {
  available('available'),
  unavailable('unavailable');

  const ControlPathAvailability(this.wireValue);
  final String wireValue;
}

/// Actual authority context reported by the server — never fabricated as a generic `authorized = true`.
enum ControlAuthorizationKind {
  devProcessLocalGrant('dev_process_local_grant'),
  fenced('fenced'),
  unbound('unbound');

  const ControlAuthorizationKind(this.wireValue);
  final String wireValue;
}

/// UI presentation projection of a ServeFloor submission derived strictly from server/Engine facts.
///
/// This is NOT a canonical domain operation lifecycle enum; Engine remains the sole owner of
/// `IntentId`, receipt, reconciliation, and physical completion semantics.
enum ProjectedOperationStatus {
  notFound('not_found'),
  rejected('rejected'),
  accepted('accepted'),
  inProgress('in_progress'),
  completed('completed'),
  diverged('diverged'),
  knowledgeUnavailable('knowledge_unavailable');

  const ProjectedOperationStatus(this.wireValue);
  final String wireValue;
}

@immutable
final class CarServeFloorCapability {
  const CarServeFloorCapability({
    required this.car,
    required this.support,
    required this.controlPathAvailability,
    this.controlPathReason,
    required this.authorization,
    required this.executableFloors,
    this.activeIntentId,
  });

  final String car;
  final OperationSupport support;
  final ControlPathAvailability controlPathAvailability;
  final String? controlPathReason;
  final ControlAuthorizationKind authorization;

  /// Exact executable floors reported from server `ExecutionCapabilities`.
  /// Never substituted with BuildingTopology or observation floor lists.
  final List<int> executableFloors;
  final int? activeIntentId;

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      (other is CarServeFloorCapability &&
          other.car == car &&
          other.support == support &&
          other.controlPathAvailability == controlPathAvailability &&
          other.controlPathReason == controlPathReason &&
          other.authorization == authorization &&
          listEquals(other.executableFloors, executableFloors) &&
          other.activeIntentId == activeIntentId);

  @override
  int get hashCode => Object.hash(
    car,
    support,
    controlPathAvailability,
    controlPathReason,
    authorization,
    Object.hashAll(executableFloors),
    activeIntentId,
  );
}

@immutable
sealed class ControlSurfaceCapabilities {
  const ControlSurfaceCapabilities();
}

/// Host does not expose the optional dev-only control surface (HTTP 404); client remains read-only.
@immutable
final class ControlSurfaceAbsent extends ControlSurfaceCapabilities {
  const ControlSurfaceAbsent();

  @override
  bool operator ==(Object other) => other is ControlSurfaceAbsent;

  @override
  int get hashCode => (ControlSurfaceAbsent).hashCode;
}

/// Client could not determine control-surface support (network/protocol error).
@immutable
final class ControlSurfaceSupportUnknown extends ControlSurfaceCapabilities {
  const ControlSurfaceSupportUnknown({required this.reason});

  final String reason;

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      (other is ControlSurfaceSupportUnknown && other.reason == reason);

  @override
  int get hashCode => Object.hash(ControlSurfaceSupportUnknown, reason);
}

@immutable
final class ControlSurfaceAvailable extends ControlSurfaceCapabilities {
  const ControlSurfaceAvailable({
    required this.controlApiVersion,
    required this.developmentOnly,
    required this.controlInstanceId,
    required this.ephemeralProcessMemory,
    required this.authorityKind,
    required this.multiUserAuth,
    required this.principal,
    required this.carsById,
  });

  final String controlApiVersion;
  final bool developmentOnly;
  final String controlInstanceId;
  final bool ephemeralProcessMemory;
  final String authorityKind;
  final bool multiUserAuth;
  final String principal;
  final Map<String, CarServeFloorCapability> carsById;

  CarServeFloorCapability? capabilityForCar(String carId) => carsById[carId];

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      (other is ControlSurfaceAvailable &&
          other.controlApiVersion == controlApiVersion &&
          other.developmentOnly == developmentOnly &&
          other.controlInstanceId == controlInstanceId &&
          other.ephemeralProcessMemory == ephemeralProcessMemory &&
          other.authorityKind == authorityKind &&
          other.multiUserAuth == multiUserAuth &&
          other.principal == principal &&
          mapEquals(other.carsById, carsById));

  @override
  int get hashCode => Object.hash(
    controlApiVersion,
    developmentOnly,
    controlInstanceId,
    ephemeralProcessMemory,
    authorityKind,
    multiUserAuth,
    principal,
    Object.hashAllUnordered(carsById.entries),
  );
}

@immutable
final class ServeFloorBlocker {
  const ServeFloorBlocker({
    required this.code,
    required this.detail,
    this.blockingIntentId,
    this.observedMode,
  });

  final String code;
  final String detail;
  final int? blockingIntentId;
  final CarOperatingMode? observedMode;

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      (other is ServeFloorBlocker &&
          other.code == code &&
          other.detail == detail &&
          other.blockingIntentId == blockingIntentId &&
          other.observedMode == observedMode);

  @override
  int get hashCode => Object.hash(code, detail, blockingIntentId, observedMode);
}

/// Server-owned advisory preview for a candidate ServeFloor request.
/// Final admission is always re-checked by Engine on submit.
@immutable
final class ServeFloorPreview {
  const ServeFloorPreview({
    required this.controlApiVersion,
    required this.developmentOnly,
    required this.controlInstanceId,
    required this.advisory,
    required this.operation,
    required this.car,
    required this.floor,
    required this.support,
    required this.controlPathAvailability,
    this.controlPathReason,
    required this.authorization,
    required this.eligible,
    this.blocker,
    required this.observedCarAvailable,
    this.observedFloorReference,
    this.observedOperatingMode,
  });

  final String controlApiVersion;
  final bool developmentOnly;
  final String controlInstanceId;
  final bool advisory;
  final String operation;
  final String car;
  final int floor;
  final OperationSupport support;
  final ControlPathAvailability controlPathAvailability;
  final String? controlPathReason;
  final ControlAuthorizationKind authorization;
  final bool eligible;
  final ServeFloorBlocker? blocker;
  final bool observedCarAvailable;
  final int? observedFloorReference;
  final CarOperatingMode? observedOperatingMode;

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      (other is ServeFloorPreview &&
          other.controlApiVersion == controlApiVersion &&
          other.developmentOnly == developmentOnly &&
          other.controlInstanceId == controlInstanceId &&
          other.advisory == advisory &&
          other.operation == operation &&
          other.car == car &&
          other.floor == floor &&
          other.support == support &&
          other.controlPathAvailability == controlPathAvailability &&
          other.controlPathReason == controlPathReason &&
          other.authorization == authorization &&
          other.eligible == eligible &&
          other.blocker == blocker &&
          other.observedCarAvailable == observedCarAvailable &&
          other.observedFloorReference == observedFloorReference &&
          other.observedOperatingMode == observedOperatingMode);

  @override
  int get hashCode => Object.hash(
    controlApiVersion,
    developmentOnly,
    controlInstanceId,
    advisory,
    operation,
    car,
    floor,
    support,
    controlPathAvailability,
    controlPathReason,
    authorization,
    eligible,
    blocker,
    observedCarAvailable,
    observedFloorReference,
    observedOperatingMode,
  );
}

@immutable
final class ProjectedSubmission {
  const ProjectedSubmission({
    required this.controlApiVersion,
    required this.developmentOnly,
    required this.controlInstanceId,
    required this.ephemeralProcessMemory,
    required this.submissionKey,
    required this.operation,
    this.car,
    this.floor,
    required this.status,
    required this.duplicateSubmission,
    this.intentId,
    this.receipt,
    this.reconciliation,
    this.divergedFloor,
    required this.sawProgress,
    required this.isActiveInEngine,
    this.blocker,
    this.knowledgeReason,
  });

  final String controlApiVersion;
  final bool developmentOnly;
  final String controlInstanceId;
  final bool ephemeralProcessMemory;
  final String submissionKey;
  final String operation;
  final String? car;
  final int? floor;
  final ProjectedOperationStatus status;
  final bool duplicateSubmission;

  /// Engine-owned operation identity when accepted; null on rejection or restart knowledge loss.
  final int? intentId;
  final String? receipt;
  final String? reconciliation;
  final int? divergedFloor;
  final bool sawProgress;
  final bool isActiveInEngine;
  final ServeFloorBlocker? blocker;
  final String? knowledgeReason;

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      (other is ProjectedSubmission &&
          other.controlApiVersion == controlApiVersion &&
          other.developmentOnly == developmentOnly &&
          other.controlInstanceId == controlInstanceId &&
          other.ephemeralProcessMemory == ephemeralProcessMemory &&
          other.submissionKey == submissionKey &&
          other.operation == operation &&
          other.car == car &&
          other.floor == floor &&
          other.status == status &&
          other.duplicateSubmission == duplicateSubmission &&
          other.intentId == intentId &&
          other.receipt == receipt &&
          other.reconciliation == reconciliation &&
          other.divergedFloor == divergedFloor &&
          other.sawProgress == sawProgress &&
          other.isActiveInEngine == isActiveInEngine &&
          other.blocker == blocker &&
          other.knowledgeReason == knowledgeReason);

  @override
  int get hashCode => Object.hash(
    controlApiVersion,
    developmentOnly,
    controlInstanceId,
    ephemeralProcessMemory,
    submissionKey,
    operation,
    car,
    floor,
    status,
    duplicateSubmission,
    intentId,
    receipt,
    reconciliation,
    divergedFloor,
    sawProgress,
    isActiveInEngine,
    blocker,
    knowledgeReason,
  );
}

@immutable
final class ControlErrorPayload {
  const ControlErrorPayload({
    required this.controlApiVersion,
    required this.controlInstanceId,
    required this.errorCode,
    this.submissionKey,
    required this.message,
  });

  final String controlApiVersion;
  final String controlInstanceId;
  final String errorCode;
  final String? submissionKey;
  final String message;

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      (other is ControlErrorPayload &&
          other.controlApiVersion == controlApiVersion &&
          other.controlInstanceId == controlInstanceId &&
          other.errorCode == errorCode &&
          other.submissionKey == submissionKey &&
          other.message == message);

  @override
  int get hashCode => Object.hash(
    controlApiVersion,
    controlInstanceId,
    errorCode,
    submissionKey,
    message,
  );
}
