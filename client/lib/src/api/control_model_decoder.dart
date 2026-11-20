import 'dart:convert';

import 'package:flutter/foundation.dart';

import 'control_model.dart';
import 'host_url.dart';
import 'read_model.dart';

@immutable
sealed class ControlDecodeException implements Exception {
  const ControlDecodeException(this.message);

  final String message;

  @override
  String toString() => message;
}

@immutable
final class UnsupportedControlApiVersionException
    extends ControlDecodeException {
  const UnsupportedControlApiVersionException(this.actualVersion)
    : super(
        'Unsupported dev control API version "$actualVersion"; expected "$devControlApiVersion".',
      );

  final String actualVersion;
}

@immutable
final class MalformedControlPayloadException extends ControlDecodeException {
  const MalformedControlPayloadException(super.message);
}

final class DevControlV0Decoder {
  const DevControlV0Decoder._();

  static const int _minI16 = -32768;
  static const int _maxI16 = 32767;

  static ControlSurfaceAvailable decodeCapabilitiesJson(String rawJson) {
    final Map<String, Object?> root = _decodeVersionedRoot(rawJson);
    final bool developmentOnly = _requireBool(root, 'development_only', 'root');
    final String instanceId = _requireNonEmptyString(
      root,
      'control_instance_id',
      'root',
    );
    final bool ephemeral = _requireBool(
      root,
      'ephemeral_process_memory',
      'root',
    );
    final Map<String, Object?> auth = _requireMap(
      root['authority_context'],
      'root.authority_context',
    );
    final String authKind = _requireNonEmptyString(
      auth,
      'kind',
      'root.authority_context',
    );
    final bool multiUserAuth = _requireBool(
      auth,
      'multi_user_auth',
      'root.authority_context',
    );
    final String principal = _requireNonEmptyString(
      auth,
      'principal',
      'root.authority_context',
    );

    final List<Object?> rawCars = _requireList(root, 'cars', 'root');
    final Map<String, CarServeFloorCapability> carsById =
        <String, CarServeFloorCapability>{};
    for (int i = 0; i < rawCars.length; i++) {
      final CarServeFloorCapability cap = _parseCarCapability(
        rawCars[i],
        'cars[$i]',
      );
      carsById[cap.car] = cap;
    }

    return ControlSurfaceAvailable(
      controlApiVersion: devControlApiVersion,
      developmentOnly: developmentOnly,
      controlInstanceId: instanceId,
      ephemeralProcessMemory: ephemeral,
      authorityKind: authKind,
      multiUserAuth: multiUserAuth,
      principal: principal,
      carsById: Map<String, CarServeFloorCapability>.unmodifiable(carsById),
    );
  }

  static ServeFloorPreview decodePreviewJson(String rawJson) {
    final Map<String, Object?> root = _decodeVersionedRoot(rawJson);
    final String operation = _requireExplicitServeFloorOperation(root, 'root');
    final String? observedModeRaw = _optionalNullableString(
      root,
      'observed_operating_mode',
      'root',
    );
    return ServeFloorPreview(
      controlApiVersion: devControlApiVersion,
      developmentOnly: _requireBool(root, 'development_only', 'root'),
      controlInstanceId: _requireNonEmptyString(
        root,
        'control_instance_id',
        'root',
      ),
      advisory: _requireBool(root, 'advisory', 'root'),
      operation: operation,
      car: _requireNonEmptyString(root, 'car', 'root'),
      floor: _requireFloorI16(root, 'floor', 'root'),
      support: _parseSupport(_requireString(root, 'support', 'root')),
      controlPathAvailability: _parseAvailability(
        _requireString(root, 'control_path_availability', 'root'),
      ),
      controlPathReason: _optionalNullableString(
        root,
        'control_path_reason',
        'root',
      ),
      authorization: _parseAuthorization(
        _requireString(root, 'authorization', 'root'),
      ),
      eligible: _requireBool(root, 'eligible', 'root'),
      blocker: _parseOptionalBlocker(root['blocker'], 'root.blocker'),
      observedCarAvailable: _requireBool(
        root,
        'observed_car_available',
        'root',
      ),
      observedFloorReference: _optionalNullableFloorI16(
        root,
        'observed_floor_reference',
        'root',
      ),
      observedOperatingMode: observedModeRaw == null
          ? null
          : _parseOperatingMode(
              observedModeRaw,
              'root.observed_operating_mode',
            ),
    );
  }

  static ProjectedSubmission decodeSubmissionJson(String rawJson) {
    final Map<String, Object?> root = _decodeVersionedRoot(rawJson);
    final String operation = _requireExplicitServeFloorOperation(root, 'root');
    return ProjectedSubmission(
      controlApiVersion: devControlApiVersion,
      developmentOnly: _requireBool(root, 'development_only', 'root'),
      controlInstanceId: _requireNonEmptyString(
        root,
        'control_instance_id',
        'root',
      ),
      ephemeralProcessMemory: _requireBool(
        root,
        'ephemeral_process_memory',
        'root',
      ),
      submissionKey: _requireNonEmptyString(root, 'submission_key', 'root'),
      operation: operation,
      car: _optionalNullableString(root, 'car', 'root'),
      floor: _optionalNullableFloorI16(root, 'floor', 'root'),
      status: _parseProjectedStatus(_requireString(root, 'status', 'root')),
      duplicateSubmission: _requireBool(root, 'duplicate_submission', 'root'),
      intentId: _optionalNullableNonNegativeInt(root, 'intent_id', 'root'),
      receipt: _optionalNullableString(root, 'receipt', 'root'),
      reconciliation: _optionalNullableString(root, 'reconciliation', 'root'),
      divergedFloor: _optionalNullableFloorI16(root, 'diverged_floor', 'root'),
      sawProgress: _requireBool(root, 'saw_progress', 'root'),
      isActiveInEngine: _requireBool(root, 'is_active_in_engine', 'root'),
      blocker: _parseOptionalBlocker(root['blocker'], 'root.blocker'),
      knowledgeReason: _optionalNullableString(
        root,
        'knowledge_reason',
        'root',
      ),
    );
  }

  static ControlErrorPayload decodeErrorJson(String rawJson) {
    final Map<String, Object?> root = _decodeVersionedRoot(rawJson);
    return ControlErrorPayload(
      controlApiVersion: devControlApiVersion,
      controlInstanceId: _requireNonEmptyString(
        root,
        'control_instance_id',
        'root',
      ),
      errorCode: _requireNonEmptyString(root, 'error_code', 'root'),
      submissionKey: _optionalNullableString(root, 'submission_key', 'root'),
      message: _requireNonEmptyString(root, 'message', 'root'),
    );
  }

  static Map<String, Object?> _decodeVersionedRoot(String rawJson) {
    final Object? decoded;
    try {
      decoded = jsonDecode(rawJson);
    } on FormatException catch (error) {
      throw MalformedControlPayloadException(
        'Invalid JSON in control response: ${error.message}',
      );
    }
    final Map<String, Object?> root = _requireMap(decoded, 'root');
    final String apiVersion = _requireString(
      root,
      'control_api_version',
      'root',
    );
    if (apiVersion != devControlApiVersion) {
      throw UnsupportedControlApiVersionException(apiVersion);
    }
    return root;
  }

  static String _requireExplicitServeFloorOperation(
    Map<String, Object?> map,
    String path,
  ) {
    final String operation = _requireNonEmptyString(map, 'operation', path);
    if (operation != 'serve_floor') {
      throw MalformedControlPayloadException(
        'Unsupported operation "$operation" at $path.operation (expected "serve_floor").',
      );
    }
    return operation;
  }

  static CarServeFloorCapability _parseCarCapability(Object? raw, String path) {
    final Map<String, Object?> map = _requireMap(raw, path);
    final String car = _requireNonEmptyString(map, 'car', path);
    final Map<String, Object?> sf = _requireMap(
      map['serve_floor'],
      '$path.serve_floor',
    );
    final List<Object?> rawFloors = _requireList(
      sf,
      'executable_floors',
      '$path.serve_floor',
    );
    final List<int> floors = <int>[];
    for (int i = 0; i < rawFloors.length; i++) {
      final Object? item = rawFloors[i];
      if (item is! int || item < _minI16 || item > _maxI16) {
        throw MalformedControlPayloadException(
          'Invalid floor entry at $path.serve_floor.executable_floors[$i].',
        );
      }
      floors.add(item);
    }

    return CarServeFloorCapability(
      car: car,
      support: _parseSupport(
        _requireString(sf, 'support', '$path.serve_floor'),
      ),
      controlPathAvailability: _parseAvailability(
        _requireString(sf, 'control_path_availability', '$path.serve_floor'),
      ),
      controlPathReason: _optionalNullableString(
        sf,
        'control_path_reason',
        '$path.serve_floor',
      ),
      authorization: _parseAuthorization(
        _requireString(sf, 'authorization', '$path.serve_floor'),
      ),
      executableFloors: List<int>.unmodifiable(floors),
      activeIntentId: _optionalNullableNonNegativeInt(
        sf,
        'active_intent_id',
        '$path.serve_floor',
      ),
    );
  }

  static ServeFloorBlocker? _parseOptionalBlocker(Object? raw, String path) {
    if (raw == null) {
      return null;
    }
    final Map<String, Object?> map = _requireMap(raw, path);
    final String? modeRaw = _optionalNullableString(map, 'observed_mode', path);
    return ServeFloorBlocker(
      code: _requireNonEmptyString(map, 'code', path),
      detail: _requireNonEmptyString(map, 'detail', path),
      blockingIntentId: _optionalNullableNonNegativeInt(
        map,
        'blocking_intent_id',
        path,
      ),
      observedMode: modeRaw == null
          ? null
          : _parseOperatingMode(modeRaw, '$path.observed_mode'),
    );
  }

  static OperationSupport _parseSupport(String raw) {
    for (final OperationSupport candidate in OperationSupport.values) {
      if (candidate.wireValue == raw) {
        return candidate;
      }
    }
    throw MalformedControlPayloadException('Unknown support "$raw".');
  }

  static ControlPathAvailability _parseAvailability(String raw) {
    for (final ControlPathAvailability candidate
        in ControlPathAvailability.values) {
      if (candidate.wireValue == raw) {
        return candidate;
      }
    }
    throw MalformedControlPayloadException(
      'Unknown control_path_availability "$raw".',
    );
  }

  static ControlAuthorizationKind _parseAuthorization(String raw) {
    for (final ControlAuthorizationKind candidate
        in ControlAuthorizationKind.values) {
      if (candidate.wireValue == raw) {
        return candidate;
      }
    }
    throw MalformedControlPayloadException('Unknown authorization "$raw".');
  }

  static ProjectedOperationStatus _parseProjectedStatus(String raw) {
    for (final ProjectedOperationStatus candidate
        in ProjectedOperationStatus.values) {
      if (candidate.wireValue == raw) {
        return candidate;
      }
    }
    throw MalformedControlPayloadException('Unknown projected status "$raw".');
  }

  static CarOperatingMode _parseOperatingMode(String raw, String path) {
    for (final CarOperatingMode candidate in CarOperatingMode.values) {
      if (candidate.wireValue == raw) {
        return candidate;
      }
    }
    throw MalformedControlPayloadException(
      'Unknown operating mode "$raw" at $path.',
    );
  }

  static Map<String, Object?> _requireMap(Object? raw, String path) {
    if (raw is Map<String, Object?>) {
      return raw;
    }
    if (raw is Map) {
      final Map<String, Object?> out = <String, Object?>{};
      for (final MapEntry<Object?, Object?> entry in raw.entries) {
        final Object? key = entry.key;
        if (key is! String) {
          throw MalformedControlPayloadException('Non-string key at $path.');
        }
        out[key] = entry.value;
      }
      return out;
    }
    throw MalformedControlPayloadException('Expected JSON object at $path.');
  }

  static List<Object?> _requireList(
    Map<String, Object?> map,
    String key,
    String path,
  ) {
    if (!map.containsKey(key)) {
      throw MalformedControlPayloadException(
        'Missing required list "$key" at $path.',
      );
    }
    final Object? value = map[key];
    if (value is List<Object?>) {
      return value;
    }
    if (value is List) {
      return List<Object?>.from(value);
    }
    throw MalformedControlPayloadException(
      'Field "$key" at $path must be a list.',
    );
  }

  static String _requireString(
    Map<String, Object?> map,
    String key,
    String path,
  ) {
    if (!map.containsKey(key)) {
      throw MalformedControlPayloadException(
        'Missing required string "$key" at $path.',
      );
    }
    final Object? value = map[key];
    if (value is String) {
      return value;
    }
    throw MalformedControlPayloadException(
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
      throw MalformedControlPayloadException(
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
    throw MalformedControlPayloadException(
      'Field "$key" at $path must be a string or null.',
    );
  }

  static bool _requireBool(Map<String, Object?> map, String key, String path) {
    if (!map.containsKey(key)) {
      throw MalformedControlPayloadException(
        'Missing required bool "$key" at $path.',
      );
    }
    final Object? value = map[key];
    if (value is bool) {
      return value;
    }
    throw MalformedControlPayloadException(
      'Field "$key" at $path must be a bool.',
    );
  }

  static int _requireFloorI16(
    Map<String, Object?> map,
    String key,
    String path,
  ) {
    final Object? value = map[key];
    if (value is int && value >= _minI16 && value <= _maxI16) {
      return value;
    }
    throw MalformedControlPayloadException(
      'Field "$key" at $path must be an i16 floor integer.',
    );
  }

  static int? _optionalNullableFloorI16(
    Map<String, Object?> map,
    String key,
    String path,
  ) {
    final Object? value = map[key];
    if (value == null) {
      return null;
    }
    if (value is int && value >= _minI16 && value <= _maxI16) {
      return value;
    }
    throw MalformedControlPayloadException(
      'Field "$key" at $path must be an i16 floor integer or null.',
    );
  }

  static int? _optionalNullableNonNegativeInt(
    Map<String, Object?> map,
    String key,
    String path,
  ) {
    final Object? value = map[key];
    if (value == null) {
      return null;
    }
    if (value is int && value >= 0) {
      return value;
    }
    throw MalformedControlPayloadException(
      'Field "$key" at $path must be a non-negative integer or null.',
    );
  }
}
