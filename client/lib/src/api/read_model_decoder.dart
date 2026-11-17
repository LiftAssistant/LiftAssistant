import 'dart:convert';

import 'package:flutter/foundation.dart';

import 'host_url.dart';
import 'read_model.dart';

@immutable
sealed class ReadModelDecodeException implements Exception {
  const ReadModelDecodeException(this.message);

  final String message;

  @override
  String toString() => message;
}

@immutable
final class UnsupportedApiVersionException extends ReadModelDecodeException {
  const UnsupportedApiVersionException(this.actualVersion)
      : super(
          'Unsupported Host API version "$actualVersion"; LiftAssistant Client requires "$experimentalApiVersion".',
        );

  final String actualVersion;
}

@immutable
final class MalformedReadModelException extends ReadModelDecodeException {
  const MalformedReadModelException(super.message);
}

final class ExperimentalV0Decoder {
  const ExperimentalV0Decoder._();

  static const int _minI16 = -32768;
  static const int _maxI16 = 32767;

  static ReadModelSnapshot decodeJson(String rawJson) {
    final Object? decoded;
    try {
      decoded = jsonDecode(rawJson);
    } on FormatException catch (error) {
      throw MalformedReadModelException(
        'Invalid JSON in Host response: ${error.message}',
      );
    }

    final Map<String, Object?> root = _requireMap(decoded, 'root');

    // Validate api_version FIRST before inspecting any nested schema fields.
    final String apiVersion = _requireString(root, 'api_version', 'root');
    if (apiVersion != experimentalApiVersion) {
      throw UnsupportedApiVersionException(apiVersion);
    }

    final int revision = _requireNonNegativeInt(root, 'revision', 'root');
    final HostLifecycle lifecycle = _parseLifecycle(
      _requireString(root, 'lifecycle', 'root'),
    );
    final HostReadiness readiness = _parseReadiness(
      _requireString(root, 'readiness', 'root'),
    );

    final List<Object?> rawCars = _requireList(
      root,
      'operational_cars',
      'root',
    );
    final List<OperationalCar> cars = List<OperationalCar>.unmodifiable(
      <OperationalCar>[
        for (int i = 0; i < rawCars.length; i++)
          _parseCar(rawCars[i], 'operational_cars[$i]'),
      ],
    );

    final List<Object?> rawSources = _requireList(root, 'sources', 'root');
    final List<SourceHealth> sources = List<SourceHealth>.unmodifiable(
      <SourceHealth>[
        for (int i = 0; i < rawSources.length; i++)
          _parseSource(rawSources[i], 'sources[$i]'),
      ],
    );

    final List<Object?> rawDiagnostics = _requireList(
      root,
      'recent_diagnostics',
      'root',
    );
    final List<DiagnosticEntry> diagnostics = List<DiagnosticEntry>.unmodifiable(
      <DiagnosticEntry>[
        for (int i = 0; i < rawDiagnostics.length; i++)
          _parseDiagnostic(rawDiagnostics[i], 'recent_diagnostics[$i]'),
      ],
    );

    return ReadModelSnapshot(
      apiVersion: apiVersion,
      revision: revision,
      lifecycle: lifecycle,
      readiness: readiness,
      operationalCars: cars,
      sources: sources,
      recentDiagnostics: diagnostics,
    );
  }

  static OperationalCar _parseCar(Object? raw, String path) {
    final Map<String, Object?> map = _requireMap(raw, path);
    return OperationalCar(
      id: _requireNonEmptyString(map, 'id', path),
      location: _parseLocation(map['location'], '$path.location'),
      motion: _parseMotion(map['motion'], '$path.motion'),
      door: _parseDoor(_requireString(map, 'door', path), '$path.door'),
      operatingMode: _parseOperatingMode(
        _requireString(map, 'operating_mode', path),
        '$path.operating_mode',
      ),
      reportedDestination: _parseReportedDestination(
        map['reported_destination'],
        '$path.reported_destination',
      ),
    );
  }

  static CarLocation _parseLocation(Object? raw, String path) {
    final Map<String, Object?> map = _requireMap(raw, path);
    final String kind = _requireString(map, 'kind', path);
    switch (kind) {
      case 'floor':
        return FloorLocation(
          floor: _requireFloorI16(map, 'floor', path),
        );
      case 'continuous':
        return ContinuousLocation(
          floorPosition: _requireFiniteDouble(map, 'floor_position', path),
          nearestFloor: _requireFloorI16(map, 'nearest_floor', path),
        );
      default:
        throw MalformedReadModelException(
          'Unknown location kind "$kind" at $path.',
        );
    }
  }

  static CarMotion _parseMotion(Object? raw, String path) {
    final Map<String, Object?> map = _requireMap(raw, path);
    final String state = _requireString(map, 'state', path);
    switch (state) {
      case 'stopped':
        return const StoppedMotion();
      case 'moving':
        final String direction = _requireString(map, 'direction', path);
        return MovingMotion(
          direction: _parseDirection(direction, '$path.direction'),
        );
      default:
        throw MalformedReadModelException(
          'Unknown motion state "$state" at $path.',
        );
    }
  }

  static MotionDirection _parseDirection(String raw, String path) {
    for (final MotionDirection candidate in MotionDirection.values) {
      if (candidate.wireValue == raw) {
        return candidate;
      }
    }
    throw MalformedReadModelException('Unknown direction "$raw" at $path.');
  }

  static CarDoorState _parseDoor(String raw, String path) {
    for (final CarDoorState candidate in CarDoorState.values) {
      if (candidate.wireValue == raw) {
        return candidate;
      }
    }
    throw MalformedReadModelException('Unknown door state "$raw" at $path.');
  }

  static CarOperatingMode _parseOperatingMode(String raw, String path) {
    for (final CarOperatingMode candidate in CarOperatingMode.values) {
      if (candidate.wireValue == raw) {
        return candidate;
      }
    }
    throw MalformedReadModelException(
      'Unknown operating mode "$raw" at $path.',
    );
  }

  static ReportedDestination _parseReportedDestination(
    Object? raw,
    String path,
  ) {
    final Map<String, Object?> map = _requireMap(raw, path);
    final String kind = _requireString(map, 'kind', path);
    switch (kind) {
      case 'known':
        return KnownReportedDestination(
          floor: _requireFloorI16(map, 'floor', path),
        );
      case 'none':
        return const NoReportedDestination();
      case 'unknown':
        return const UnknownReportedDestination();
      default:
        throw MalformedReadModelException(
          'Unknown reported_destination kind "$kind" at $path.',
        );
    }
  }

  static SourceHealth _parseSource(Object? raw, String path) {
    final Map<String, Object?> map = _requireMap(raw, path);
    final List<Object?> rawObservations = _requireList(
      map,
      'observations',
      path,
    );
    return SourceHealth(
      source: _requireNonEmptyString(map, 'source', path),
      session: _parseSourceSession(map['session'], '$path.session'),
      transport: _parseTransportState(
        _requireString(map, 'transport', path),
        '$path.transport',
      ),
      payload: _parsePayloadActivity(
        _requireString(map, 'payload', path),
        '$path.payload',
      ),
      observations: List<SourceObservation>.unmodifiable(<SourceObservation>[
        for (int i = 0; i < rawObservations.length; i++)
          _parseSourceObservation(
            rawObservations[i],
            '$path.observations[$i]',
          ),
      ]),
    );
  }

  static SourceSession _parseSourceSession(Object? raw, String path) {
    final Map<String, Object?> map = _requireMap(raw, path);
    final String state = _requireString(map, 'state', path);
    switch (state) {
      case 'inactive':
        return const InactiveSourceSession();
      case 'active':
        return ActiveSourceSession(
          session: _requireNonEmptyString(map, 'session', path),
        );
      default:
        throw MalformedReadModelException(
          'Unknown source session state "$state" at $path.',
        );
    }
  }

  static TransportState _parseTransportState(String raw, String path) {
    for (final TransportState candidate in TransportState.values) {
      if (candidate.wireValue == raw) {
        return candidate;
      }
    }
    throw MalformedReadModelException(
      'Unknown transport state "$raw" at $path.',
    );
  }

  static PayloadActivity _parsePayloadActivity(String raw, String path) {
    for (final PayloadActivity candidate in PayloadActivity.values) {
      if (candidate.wireValue == raw) {
        return candidate;
      }
    }
    throw MalformedReadModelException(
      'Unknown payload activity "$raw" at $path.',
    );
  }

  static SourceObservation _parseSourceObservation(Object? raw, String path) {
    final Map<String, Object?> map = _requireMap(raw, path);
    return SourceObservation(
      car: _requireNonEmptyString(map, 'car', path),
      role: _parseBindingRole(_requireString(map, 'role', path), '$path.role'),
      availability: _parseAvailability(
        _requireString(map, 'availability', path),
        '$path.availability',
      ),
    );
  }

  static BindingRole _parseBindingRole(String raw, String path) {
    for (final BindingRole candidate in BindingRole.values) {
      if (candidate.wireValue == raw) {
        return candidate;
      }
    }
    throw MalformedReadModelException('Unknown binding role "$raw" at $path.');
  }

  static ObservationAvailability _parseAvailability(String raw, String path) {
    for (final ObservationAvailability candidate
        in ObservationAvailability.values) {
      if (candidate.wireValue == raw) {
        return candidate;
      }
    }
    throw MalformedReadModelException(
      'Unknown observation availability "$raw" at $path.',
    );
  }

  static DiagnosticEntry _parseDiagnostic(Object? raw, String path) {
    final Map<String, Object?> map = _requireMap(raw, path);
    final String? roleRaw = _optionalNullableString(map, 'role', path);
    return DiagnosticEntry(
      sequence: _requireNonNegativeInt(map, 'sequence', path),
      layer: _parseDiagnosticLayer(
        _requireString(map, 'layer', path),
        '$path.layer',
      ),
      id: _requireNonEmptyString(map, 'id', path),
      source: _optionalNullableString(map, 'source', path),
      session: _optionalNullableString(map, 'session', path),
      car: _optionalNullableString(map, 'car', path),
      role: roleRaw == null ? null : _parseBindingRole(roleRaw, '$path.role'),
      nativeEntity: _optionalNullableString(map, 'native_entity', path),
    );
  }

  static DiagnosticLayer _parseDiagnosticLayer(String raw, String path) {
    for (final DiagnosticLayer candidate in DiagnosticLayer.values) {
      if (candidate.wireValue == raw) {
        return candidate;
      }
    }
    throw MalformedReadModelException(
      'Unknown diagnostic layer "$raw" at $path.',
    );
  }

  static HostLifecycle _parseLifecycle(String raw) {
    for (final HostLifecycle candidate in HostLifecycle.values) {
      if (candidate.wireValue == raw) {
        return candidate;
      }
    }
    throw MalformedReadModelException('Unknown Host lifecycle "$raw".');
  }

  static HostReadiness _parseReadiness(String raw) {
    for (final HostReadiness candidate in HostReadiness.values) {
      if (candidate.wireValue == raw) {
        return candidate;
      }
    }
    throw MalformedReadModelException('Unknown Host readiness "$raw".');
  }

  static Map<String, Object?> _requireMap(Object? raw, String path) {
    if (raw is Map<String, Object?>) {
      return raw;
    }
    if (raw is Map) {
      final Map<String, Object?> converted = <String, Object?>{};
      for (final MapEntry<Object?, Object?> entry in raw.entries) {
        final Object? key = entry.key;
        if (key is! String) {
          throw MalformedReadModelException('Non-string key at $path.');
        }
        converted[key] = entry.value;
      }
      return converted;
    }
    throw MalformedReadModelException('Expected JSON object at $path.');
  }

  static List<Object?> _requireList(
    Map<String, Object?> map,
    String key,
    String path,
  ) {
    if (!map.containsKey(key)) {
      throw MalformedReadModelException('Missing required list "$key" at $path.');
    }
    final Object? value = map[key];
    if (value is List<Object?>) {
      return value;
    }
    if (value is List) {
      return List<Object?>.from(value);
    }
    throw MalformedReadModelException('Field "$key" at $path must be a list.');
  }

  static String _requireString(
    Map<String, Object?> map,
    String key,
    String path,
  ) {
    if (!map.containsKey(key)) {
      throw MalformedReadModelException(
        'Missing required string "$key" at $path.',
      );
    }
    final Object? value = map[key];
    if (value is String) {
      return value;
    }
    throw MalformedReadModelException(
      'Field "$key" at $path must be a string.',
    );
  }

  static String _requireNonEmptyString(
    Map<String, Object?> map,
    String key,
    String path,
  ) {
    final String value = _requireString(map, key, path);
    if (value.isEmpty) {
      throw MalformedReadModelException(
        'Field "$key" at $path must not be empty.',
      );
    }
    return value;
  }

  static String? _optionalNullableString(
    Map<String, Object?> map,
    String key,
    String path,
  ) {
    final Object? value = map[key];
    if (value == null) {
      return null;
    }
    if (value is String) {
      return value;
    }
    throw MalformedReadModelException(
      'Field "$key" at $path must be a string or null.',
    );
  }

  static int _requireInt(
    Map<String, Object?> map,
    String key,
    String path,
  ) {
    if (!map.containsKey(key)) {
      throw MalformedReadModelException(
        'Missing required integer "$key" at $path.',
      );
    }
    final Object? value = map[key];
    if (value is int) {
      return value;
    }
    throw MalformedReadModelException(
      'Field "$key" at $path must be an integer.',
    );
  }

  static int _requireNonNegativeInt(
    Map<String, Object?> map,
    String key,
    String path,
  ) {
    final int value = _requireInt(map, key, path);
    if (value < 0) {
      throw MalformedReadModelException(
        'Field "$key" at $path must be non-negative (got $value).',
      );
    }
    return value;
  }

  static int _requireFloorI16(
    Map<String, Object?> map,
    String key,
    String path,
  ) {
    final int value = _requireInt(map, key, path);
    if (value < _minI16 || value > _maxI16) {
      throw MalformedReadModelException(
        'Floor "$key" at $path is outside i16 range ($value).',
      );
    }
    return value;
  }

  static double _requireFiniteDouble(
    Map<String, Object?> map,
    String key,
    String path,
  ) {
    if (!map.containsKey(key)) {
      throw MalformedReadModelException(
        'Missing required number "$key" at $path.',
      );
    }
    final Object? value = map[key];
    if (value is num && value.isFinite) {
      return value.toDouble();
    }
    throw MalformedReadModelException(
      'Field "$key" at $path must be a finite number.',
    );
  }
}
