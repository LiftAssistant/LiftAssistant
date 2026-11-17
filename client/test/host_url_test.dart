import 'package:flutter_test/flutter_test.dart';
import 'package:liftassistant_client/src/api/host_url.dart';

void main() {
  group('HostEndpoint.parse', () {
    test('accepts explicit http host with optional port and trailing slash', () {
      final HostEndpoint loopback = HostEndpoint.parse('http://127.0.0.1:8080');
      expect(loopback.origin, 'http://127.0.0.1:8080');
      expect(
        loopback.snapshotUri.toString(),
        'http://127.0.0.1:8080/api/experimental/v0/snapshot',
      );
      expect(
        loopback.eventsUri.toString(),
        'http://127.0.0.1:8080/api/experimental/v0/events',
      );

      final HostEndpoint withSlash = HostEndpoint.parse(
        '  http://192.168.10.42:9090/  ',
      );
      expect(withSlash.origin, 'http://192.168.10.42:9090');

      final HostEndpoint defaultPort = HostEndpoint.parse('http://localhost');
      expect(defaultPort.origin, 'http://localhost');

      final HostEndpoint ipv6 = HostEndpoint.parse('http://[::1]:8080/');
      expect(ipv6.origin, 'http://[::1]:8080');
      expect(
        ipv6.snapshotUri.toString(),
        'http://[::1]:8080/api/experimental/v0/snapshot',
      );
    });

    test('rejects empty, missing scheme, https, paths, queries, and credentials', () {
      expect(
        () => HostEndpoint.parse('   '),
        throwsA(isA<InvalidHostUrlException>()),
      );
      expect(
        () => HostEndpoint.parse('127.0.0.1:8080'),
        throwsA(isA<InvalidHostUrlException>()),
      );
      expect(
        () => HostEndpoint.parse('https://127.0.0.1:8080'),
        throwsA(
          isA<InvalidHostUrlException>().having(
            (InvalidHostUrlException e) => e.message,
            'message',
            contains('http://'),
          ),
        ),
      );
      expect(
        () => HostEndpoint.parse('ws://127.0.0.1:8080'),
        throwsA(isA<InvalidHostUrlException>()),
      );
      expect(
        () => HostEndpoint.parse('http://user:pass@127.0.0.1:8080'),
        throwsA(isA<InvalidHostUrlException>()),
      );
      expect(
        () => HostEndpoint.parse('http://127.0.0.1:8080/api'),
        throwsA(isA<InvalidHostUrlException>()),
      );
      expect(
        () => HostEndpoint.parse('http://127.0.0.1:8080?foo=bar'),
        throwsA(isA<InvalidHostUrlException>()),
      );
      expect(
        () => HostEndpoint.parse('http://127.0.0.1:8080#top'),
        throwsA(isA<InvalidHostUrlException>()),
      );
      expect(
        () => HostEndpoint.parse('http://127.0.0.1:'),
        throwsA(isA<InvalidHostUrlException>()),
      );
      expect(
        () => HostEndpoint.parse('http://127.0.0.1:0'),
        throwsA(isA<InvalidHostUrlException>()),
      );
      expect(
        () => HostEndpoint.parse('http://127.0.0.1:70000'),
        throwsA(isA<InvalidHostUrlException>()),
      );
    });
  });
}
