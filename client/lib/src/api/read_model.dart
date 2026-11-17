import 'package:flutter/foundation.dart';

enum HostLifecycle {
  starting('starting'),
  synchronizing('synchronizing'),
  readyReadOnly('ready_read_only'),
  degraded('degraded');

  const HostLifecycle(this.wireValue);
  final String wireValue;
}

enum HostReadiness {
  notReady('not_ready'),
  readOnly('read_only');

  const HostReadiness(this.wireValue);
  final String wireValue;
}

@immutable
sealed class CarLocation {
  const CarLocation();
}

/// Indicates that the Host knows the car is associated with [floor],
/// NOT that it possesses a continuous physical coordinate.
@immutable
final class FloorLocation extends CarLocation {
  const FloorLocation({required this.floor});

  final int floor;

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      (other is FloorLocation && other.floor == floor);

  @override
  int get hashCode => Object.hash(FloorLocation, floor);

  @override
  String toString() => 'FloorLocation(floor: $floor)';
}

/// Indicates that the Host possesses genuine continuous position evidence.
@immutable
final class ContinuousLocation extends CarLocation {
  const ContinuousLocation({
    required this.floorPosition,
    required this.nearestFloor,
  });

  final double floorPosition;
  final int nearestFloor;

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      (other is ContinuousLocation &&
          other.floorPosition == floorPosition &&
          other.nearestFloor == nearestFloor);

  @override
  int get hashCode =>
      Object.hash(ContinuousLocation, floorPosition, nearestFloor);

  @override
  String toString() =>
      'ContinuousLocation(floorPosition: $floorPosition, nearestFloor: $nearestFloor)';
}

enum MotionDirection {
  up('up'),
  down('down');

  const MotionDirection(this.wireValue);
  final String wireValue;
}

@immutable
sealed class CarMotion {
  const CarMotion();
}

@immutable
final class StoppedMotion extends CarMotion {
  const StoppedMotion();

  @override
  bool operator ==(Object other) => other is StoppedMotion;

  @override
  int get hashCode => (StoppedMotion).hashCode;

  @override
  String toString() => 'StoppedMotion()';
}

@immutable
final class MovingMotion extends CarMotion {
  const MovingMotion({required this.direction});

  final MotionDirection direction;

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      (other is MovingMotion && other.direction == direction);

  @override
  int get hashCode => Object.hash(MovingMotion, direction);

  @override
  String toString() => 'MovingMotion(direction: ${direction.wireValue})';
}

enum CarDoorState {
  closed('closed'),
  opening('opening'),
  open('open'),
  closing('closing'),
  obstructed('obstructed'),
  fault('fault');

  const CarDoorState(this.wireValue);
  final String wireValue;
}

enum CarOperatingMode {
  normal('normal'),
  independent('independent'),
  inspection('inspection'),
  fireService('fire_service'),
  outOfService('out_of_service');

  const CarOperatingMode(this.wireValue);
  final String wireValue;
}

/// Destination evidence reported by an external observation source.
/// Never represents LiftAssistant desired target, call assignment, or command.
@immutable
sealed class ReportedDestination {
  const ReportedDestination();
}

@immutable
final class KnownReportedDestination extends ReportedDestination {
  const KnownReportedDestination({required this.floor});

  final int floor;

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      (other is KnownReportedDestination && other.floor == floor);

  @override
  int get hashCode => Object.hash(KnownReportedDestination, floor);

  @override
  String toString() => 'KnownReportedDestination(floor: $floor)';
}

@immutable
final class NoReportedDestination extends ReportedDestination {
  const NoReportedDestination();

  @override
  bool operator ==(Object other) => other is NoReportedDestination;

  @override
  int get hashCode => (NoReportedDestination).hashCode;

  @override
  String toString() => 'NoReportedDestination()';
}

@immutable
final class UnknownReportedDestination extends ReportedDestination {
  const UnknownReportedDestination();

  @override
  bool operator ==(Object other) => other is UnknownReportedDestination;

  @override
  int get hashCode => (UnknownReportedDestination).hashCode;

  @override
  String toString() => 'UnknownReportedDestination()';
}

@immutable
final class OperationalCar {
  const OperationalCar({
    required this.id,
    required this.location,
    required this.motion,
    required this.door,
    required this.operatingMode,
    required this.reportedDestination,
  });

  final String id;
  final CarLocation location;
  final CarMotion motion;
  final CarDoorState door;
  final CarOperatingMode operatingMode;
  final ReportedDestination reportedDestination;

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      (other is OperationalCar &&
          other.id == id &&
          other.location == location &&
          other.motion == motion &&
          other.door == door &&
          other.operatingMode == operatingMode &&
          other.reportedDestination == reportedDestination);

  @override
  int get hashCode => Object.hash(
    id,
    location,
    motion,
    door,
    operatingMode,
    reportedDestination,
  );
}

@immutable
sealed class SourceSession {
  const SourceSession();
}

@immutable
final class InactiveSourceSession extends SourceSession {
  const InactiveSourceSession();

  @override
  bool operator ==(Object other) => other is InactiveSourceSession;

  @override
  int get hashCode => (InactiveSourceSession).hashCode;

  @override
  String toString() => 'InactiveSourceSession()';
}

@immutable
final class ActiveSourceSession extends SourceSession {
  const ActiveSourceSession({required this.session});

  final String session;

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      (other is ActiveSourceSession && other.session == session);

  @override
  int get hashCode => Object.hash(ActiveSourceSession, session);

  @override
  String toString() => 'ActiveSourceSession(session: $session)';
}

enum TransportState {
  unknown('unknown'),
  subscriberCreated('subscriber_created'),
  discoveryObserved('discovery_observed');

  const TransportState(this.wireValue);
  final String wireValue;
}

enum PayloadActivity {
  awaiting('awaiting'),
  active('active'),
  stale('stale');

  const PayloadActivity(this.wireValue);
  final String wireValue;
}

/// Authority role of an explicit `(source, car)` binding.
///
/// - [operational]: permitted to enter LiftAssistant operational truth.
/// - [auxiliary]: inspectable and diagnostic only; does NOT enter operational
///   truth and does NOT act as an automatic backup or failover source.
enum BindingRole {
  operational('operational'),
  auxiliary('auxiliary');

  const BindingRole(this.wireValue);
  final String wireValue;
}

enum ObservationAvailability {
  awaiting('awaiting'),
  incomplete('incomplete'),
  unresolved('unresolved'),
  resolved('resolved'),
  revoked('revoked');

  const ObservationAvailability(this.wireValue);
  final String wireValue;
}

@immutable
final class SourceObservation {
  const SourceObservation({
    required this.car,
    required this.role,
    required this.availability,
  });

  final String car;
  final BindingRole role;
  final ObservationAvailability availability;

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      (other is SourceObservation &&
          other.car == car &&
          other.role == role &&
          other.availability == availability);

  @override
  int get hashCode => Object.hash(car, role, availability);
}

@immutable
final class SourceHealth {
  const SourceHealth({
    required this.source,
    required this.session,
    required this.transport,
    required this.payload,
    required this.observations,
  });

  final String source;
  final SourceSession session;
  final TransportState transport;
  final PayloadActivity payload;
  final List<SourceObservation> observations;

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      (other is SourceHealth &&
          other.source == source &&
          other.session == session &&
          other.transport == transport &&
          other.payload == payload &&
          listEquals(other.observations, observations));

  @override
  int get hashCode => Object.hash(
    source,
    session,
    transport,
    payload,
    Object.hashAll(observations),
  );
}

enum DiagnosticLayer {
  transport('transport'),
  source('source'),
  mapping('mapping'),
  observation('observation'),
  readiness('readiness');

  const DiagnosticLayer(this.wireValue);
  final String wireValue;
}

/// Machine-readable diagnostic evidence observed by LiftAssistant Host.
/// Does NOT assert a confirmed elevator equipment fault, root cause, or repair action.
@immutable
final class DiagnosticEntry {
  const DiagnosticEntry({
    required this.sequence,
    required this.layer,
    required this.id,
    this.source,
    this.session,
    this.car,
    this.role,
    this.nativeEntity,
  });

  final int sequence;
  final DiagnosticLayer layer;
  final String id;
  final String? source;
  final String? session;
  final String? car;
  final BindingRole? role;
  final String? nativeEntity;

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      (other is DiagnosticEntry &&
          other.sequence == sequence &&
          other.layer == layer &&
          other.id == id &&
          other.source == source &&
          other.session == session &&
          other.car == car &&
          other.role == role &&
          other.nativeEntity == nativeEntity);

  @override
  int get hashCode => Object.hash(
    sequence,
    layer,
    id,
    source,
    session,
    car,
    role,
    nativeEntity,
  );
}

@immutable
final class ReadModelSnapshot {
  const ReadModelSnapshot({
    required this.apiVersion,
    required this.revision,
    required this.lifecycle,
    required this.readiness,
    required this.operationalCars,
    required this.sources,
    required this.recentDiagnostics,
  });

  final String apiVersion;
  final int revision;
  final HostLifecycle lifecycle;
  final HostReadiness readiness;
  final List<OperationalCar> operationalCars;
  final List<SourceHealth> sources;
  final List<DiagnosticEntry> recentDiagnostics;

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      (other is ReadModelSnapshot &&
          other.apiVersion == apiVersion &&
          other.revision == revision &&
          other.lifecycle == lifecycle &&
          other.readiness == readiness &&
          listEquals(other.operationalCars, operationalCars) &&
          listEquals(other.sources, sources) &&
          listEquals(other.recentDiagnostics, recentDiagnostics));

  @override
  int get hashCode => Object.hash(
    apiVersion,
    revision,
    lifecycle,
    readiness,
    Object.hashAll(operationalCars),
    Object.hashAll(sources),
    Object.hashAll(recentDiagnostics),
  );
}
