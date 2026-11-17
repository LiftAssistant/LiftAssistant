import 'dart:async';
import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
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

    test('handles arbitrary CRLF and multi-byte UTF-8 splits across chunks', () async {
      final String rawFrame =
          ': keepalive\r\n\r\nevent: snapshot\r\ndata: {"source":"站点-α"}\r\n\r\n';
      final List<int> allBytes = utf8.encode(rawFrame);

      // Feed literally 1 byte per chunk to stress UTF-8 & CRLF boundary handling.
      final Stream<List<int>> singleByteChunks = Stream<List<int>>.fromIterable(
        allBytes.map((int b) => <int>[b]),
      );

      final List<String> snapshots =
          await parseSseSnapshotStream(singleByteChunks).toList();
      expect(snapshots, <String>['{"source":"站点-α"}']);
    });
  });
}
