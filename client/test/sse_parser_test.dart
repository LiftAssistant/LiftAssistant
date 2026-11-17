import 'dart:async';
import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:liftassistant_client/src/transport/host_transport.dart';
import 'package:liftassistant_client/src/transport/sse_parser.dart';

void main() {
  group('parseSseSnapshotStream', () {
    test('parses snapshot events while ignoring keepalives and other event names', () async {
      final Stream<List<int>> bytes = Stream<List<int>>.fromIterable(<List<int>>[
        utf8.encode(': keepalive\n\n'),
        utf8.encode('event: heartbeat\ndata: ignored\n\n'),
        utf8.encode('event: snapshot\ndata: {"revision":1}\n\n'),
        utf8.encode('event:snapshot\ndata:{"revision":2,\ndata:"ok":true}\n\n'),
      ]);

      int byteActivities = 0;
      final List<String> snapshots = await parseSseSnapshotStream(
        bytes,
        onByteActivity: () => byteActivities++,
      ).toList();

      expect(byteActivities, 4);
      expect(snapshots, <String>[
        '{"revision":1}',
        '{"revision":2,\n"ok":true}',
      ]);
    });

    test('handles arbitrary CRLF, standalone CR, leading BOM, and multi-byte UTF-8 splits across chunks', () async {
      final List<int> bom = <int>[0xEF, 0xBB, 0xBF];
      final String rawFrame =
          ': keepalive\r\n\r\nevent: snapshot\r\ndata: {"source":"站点-α"}\r\n\r\n'
          'event: snapshot\rdata: {"cr":true}\r\r';
      final List<int> allBytes = <int>[...bom, ...utf8.encode(rawFrame)];

      // Feed literally 1 byte per chunk to stress UTF-8, BOM, CR, & CRLF boundary handling.
      final Stream<List<int>> singleByteChunks = Stream<List<int>>.fromIterable(
        allBytes.map((int b) => <int>[b]),
      );

      final List<String> snapshots =
          await parseSseSnapshotStream(singleByteChunks).toList();
      expect(snapshots, <String>[
        '{"source":"站点-α"}',
        '{"cr":true}',
      ]);
    });

    test('enforces exact UTF-8 maxEventBytes boundary across single-line and multi-line data frames', () async {
      // '{"s":"站点"}' is 14 UTF-8 bytes (10 UTF-16 code units).
      const String utf8Payload = '{"s":"站点"}';
      final int exactUtf8Bytes = utf8.encode(utf8Payload).length;
      expect(exactUtf8Bytes, 14);

      final Stream<List<int>> exactSingleLine = Stream<List<int>>.value(
        utf8.encode('event: snapshot\ndata: $utf8Payload\n\n'),
      );
      expect(
        await parseSseSnapshotStream(
          exactSingleLine,
          maxEventBytes: exactUtf8Bytes,
        ).toList(),
        <String>[utf8Payload],
      );

      // 1 byte tighter must reject multi-byte UTF-8 payload even though String.length (10) < 13.
      final Stream<List<int>> overByOneByte = Stream<List<int>>.value(
        utf8.encode('event: snapshot\ndata: $utf8Payload\n\n'),
      );
      await expectLater(
        parseSseSnapshotStream(
          overByOneByte,
          maxEventBytes: exactUtf8Bytes - 1,
        ).toList(),
        throwsA(
          isA<SnapshotTooLargeException>().having(
            (SnapshotTooLargeException e) => e.limitBytes,
            'limitBytes',
            exactUtf8Bytes - 1,
          ),
        ),
      );

      // Multi-line data joins with '\n' (+1 byte): 5 + 1 + 4 = 10 bytes.
      final Stream<List<int>> exactMultiLine = Stream<List<int>>.value(
        utf8.encode('event: snapshot\ndata: 12345\ndata: 6789\n\n'),
      );
      expect(
        await parseSseSnapshotStream(exactMultiLine, maxEventBytes: 10).toList(),
        <String>['12345\n6789'],
      );

      final Stream<List<int>> overMultiLine = Stream<List<int>>.value(
        utf8.encode('event: snapshot\ndata: 12345\ndata: 67890\n\n'),
      );
      await expectLater(
        parseSseSnapshotStream(overMultiLine, maxEventBytes: 10).toList(),
        throwsA(isA<SnapshotTooLargeException>()),
      );
    });

    test('aborts immediately on oversized unterminated line or undispatched non-snapshot data without waiting for EOF', () async {
      bool upstreamCancelled = false;
      final StreamController<List<int>> unterminated =
          StreamController<List<int>>(
            onCancel: () {
              upstreamCancelled = true;
            },
          );
      addTearDown(unterminated.close);

      final Future<List<String>> result = parseSseSnapshotStream(
        unterminated.stream,
        maxEventBytes: 32,
      ).toList();

      // Send a single line without any '\n' or '\r' exceeding line allowance.
      unterminated.add(utf8.encode('data: ${'x' * 200}'));

      await expectLater(result, throwsA(isA<SnapshotTooLargeException>()));
      expect(upstreamCancelled, isTrue);

      // Also verify non-snapshot or event-less data accumulation is bounded.
      final Stream<List<int>> nonSnapshotFlood = Stream<List<int>>.fromIterable(
        List<List<int>>.generate(
          10,
          (_) => utf8.encode('event: other\ndata: 0123456789\n'),
        ),
      );
      await expectLater(
        parseSseSnapshotStream(nonSnapshotFlood, maxEventBytes: 40).toList(),
        throwsA(isA<SnapshotTooLargeException>()),
      );
    });

    test('resets per-event byte accounting after each dispatched event for long-lived streams', () async {
      // 20 events * ~18 bytes = ~360 bytes total, each well within maxEventBytes: 24.
      final Stream<List<int>> longLived = Stream<List<int>>.fromIterable(
        List<List<int>>.generate(
          20,
          (int i) => utf8.encode(': keepalive\n\nevent: snapshot\ndata: {"rev":$i}\n\n'),
        ),
      );

      final List<String> snapshots = await parseSseSnapshotStream(
        longLived,
        maxEventBytes: 24,
      ).toList();
      expect(snapshots, hasLength(20));
      expect(snapshots.first, '{"rev":0}');
      expect(snapshots.last, '{"rev":19}');
    });
  });
}
