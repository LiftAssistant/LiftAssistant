import 'package:flutter/foundation.dart';

const String experimentalApiVersion = 'experimental/v0';
const String snapshotPath = '/api/experimental/v0/snapshot';
const String eventsPath = '/api/experimental/v0/events';

@immutable
class InvalidHostUrlException implements Exception {
  const InvalidHostUrlException(this.message);

  final String message;

  @override
  String toString() => message;
}

@immutable
class HostEndpoint {
  const HostEndpoint._({
    required this.baseUri,
    required this.snapshotUri,
    required this.eventsUri,
  });

  factory HostEndpoint.parse(String rawInput) {
    final String trimmed = rawInput.trim();
    if (trimmed.isEmpty) {
      throw const InvalidHostUrlException(
        'Enter a LiftAssistant Host base URL (for example http://127.0.0.1:8080).',
      );
    }

    if (!trimmed.contains('://')) {
      throw const InvalidHostUrlException(
        'Host base URL must include explicit http:// (for example http://127.0.0.1:8080).',
      );
    }

    final Uri? uri = Uri.tryParse(trimmed);
    if (uri == null || !uri.hasScheme) {
      throw const InvalidHostUrlException('Invalid Host base URL syntax.');
    }

    if (uri.scheme.toLowerCase() == 'https') {
      throw const InvalidHostUrlException(
        'Host base URL must use explicit http:// (experimental/v0 has no built-in TLS).',
      );
    }
    if (uri.scheme.toLowerCase() != 'http') {
      throw InvalidHostUrlException(
        'Unsupported URL scheme "${uri.scheme}://"; Host base URL must use explicit http://.',
      );
    }

    if (uri.userInfo.isNotEmpty) {
      throw const InvalidHostUrlException(
        'Host base URL must not contain credentials (experimental/v0 is unauthenticated).',
      );
    }

    if (!uri.hasAuthority || uri.host.isEmpty) {
      throw const InvalidHostUrlException(
        'Host base URL must specify a host (for example http://127.0.0.1:8080).',
      );
    }

    if (trimmed.contains('?') ||
        trimmed.contains('#') ||
        uri.hasQuery ||
        uri.hasFragment) {
      throw const InvalidHostUrlException(
        'Host base URL must not contain query parameters or fragments.',
      );
    }

    // Reject empty port suffix such as "http://127.0.0.1:" or "http://[::1]:/"
    final int schemeSep = trimmed.indexOf('://');
    final String afterScheme = trimmed.substring(schemeSep + 3);
    final int slashIdx = afterScheme.indexOf('/');
    final String rawAuthority = slashIdx == -1
        ? afterScheme
        : afterScheme.substring(0, slashIdx);
    if (rawAuthority.endsWith(':')) {
      throw const InvalidHostUrlException(
        'Host base URL has an empty port after ":".',
      );
    }

    if (uri.hasPort && (uri.port < 1 || uri.port > 65535)) {
      throw InvalidHostUrlException(
        'Host port ${uri.port} is out of valid TCP range (1..65535).',
      );
    }

    if (uri.path.isNotEmpty && uri.path != '/') {
      throw const InvalidHostUrlException(
        'Host base URL must contain only http://host[:port] without a path.',
      );
    }

    final String formattedHost = uri.host.contains(':') && !uri.host.startsWith('[')
        ? '[${uri.host}]'
        : uri.host;
    final String portSuffix = uri.hasPort ? ':${uri.port}' : '';
    final Uri normalizedBase = Uri.parse('http://$formattedHost$portSuffix');

    return HostEndpoint._(
      baseUri: normalizedBase,
      snapshotUri: normalizedBase.replace(path: snapshotPath),
      eventsUri: normalizedBase.replace(path: eventsPath),
    );
  }

  final Uri baseUri;
  final Uri snapshotUri;
  final Uri eventsUri;

  String get origin => baseUri.toString();

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      (other is HostEndpoint && other.baseUri == baseUri);

  @override
  int get hashCode => baseUri.hashCode;

  @override
  String toString() => origin;
}
