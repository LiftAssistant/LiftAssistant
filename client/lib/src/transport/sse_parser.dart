import 'dart:async';
import 'dart:convert';
import 'dart:typed_data';

import 'host_transport.dart';

const int _lf = 0x0A;
const int _cr = 0x0D;
const int _space = 0x20;
const int _sseLineFramingAllowanceBytes = 64;

/// Transforms a raw UTF-8 byte stream from `GET /api/experimental/v0/events`
/// into complete `snapshot` event JSON payload strings.
///
/// Follows SSE framing rules while enforcing [maxEventBytes] per event:
/// - ignores comment/keepalive lines beginning with `:` (such as `: keepalive`);
/// - supports `\n`, `\r\n`, and `\r` line endings across arbitrary chunk splits;
/// - concatenates multiple `data:` lines within one event using `\n`;
/// - emits payloads only when `event` is `"snapshot"` and `data` is non-empty;
/// - rejects oversized unterminated lines or accumulated `data:` payloads
///   exceeding [maxEventBytes] with [SnapshotTooLargeException].
Stream<String> parseSseSnapshotStream(
  Stream<List<int>> byteStream, {
  void Function()? onByteActivity,
  int maxEventBytes = maxSnapshotBytes,
}) {
  if (maxEventBytes <= 0) {
    throw ArgumentError.value(
      maxEventBytes,
      'maxEventBytes',
      'Must be positive.',
    );
  }

  late StreamController<String> controller;
  StreamSubscription<List<int>>? byteSubscription;

  final int maxLineBytes = maxEventBytes + _sseLineFramingAllowanceBytes;
  final BytesBuilder lineBuffer = BytesBuilder(copy: false);
  bool isFirstLine = true;
  bool lastWasCr = false;
  bool failed = false;

  String? currentEvent;
  final List<String> dataLines = <String>[];
  int dataBytes = 0;

  void failStream(Object error, [StackTrace? stackTrace]) {
    if (failed) {
      return;
    }
    failed = true;
    lineBuffer.clear();
    currentEvent = null;
    dataLines.clear();
    dataBytes = 0;
    if (!controller.isClosed) {
      controller.addError(error, stackTrace ?? StackTrace.current);
    }
    unawaited(byteSubscription?.cancel());
    if (!controller.isClosed) {
      unawaited(controller.close());
    }
  }

  void dispatchEvent() {
    if (currentEvent == 'snapshot' && dataLines.isNotEmpty) {
      if (!controller.isClosed) {
        controller.add(dataLines.join('\n'));
      }
    }
    currentEvent = null;
    dataLines.clear();
    dataBytes = 0;
  }

  void processCompletedLine() {
    Uint8List rawLine = lineBuffer.takeBytes();
    if (isFirstLine) {
      isFirstLine = false;
      if (rawLine.length >= 3 &&
          rawLine[0] == 0xEF &&
          rawLine[1] == 0xBB &&
          rawLine[2] == 0xBF) {
        rawLine = Uint8List.sublistView(rawLine, 3);
      }
    }

    if (rawLine.isEmpty) {
      dispatchEvent();
      return;
    }

    final String line;
    try {
      line = utf8.decode(rawLine);
    } on FormatException catch (error, stackTrace) {
      failStream(error, stackTrace);
      return;
    }

    if (line.startsWith(':')) {
      // SSE comment / keepalive frame.
      return;
    }

    final int colonIndex = line.indexOf(':');
    final String field;
    String value;
    if (colonIndex == -1) {
      field = line;
      value = '';
    } else {
      field = line.substring(0, colonIndex);
      value = line.substring(colonIndex + 1);
      if (value.startsWith(' ')) {
        value = value.substring(1);
      }
    }

    if (field == 'event') {
      currentEvent = value;
    } else if (field == 'data') {
      final int valueBytes;
      if (colonIndex == -1) {
        valueBytes = 0;
      } else if (rawLine.length > 5 && rawLine[5] == _space) {
        valueBytes = rawLine.length - 6;
      } else {
        valueBytes = rawLine.length - 5;
      }
      final int nextDataBytes = dataLines.isEmpty
          ? valueBytes
          : dataBytes + 1 + valueBytes;
      if (nextDataBytes > maxEventBytes) {
        failStream(SnapshotTooLargeException(limitBytes: maxEventBytes));
        return;
      }
      dataBytes = nextDataBytes;
      dataLines.add(value);
    }
  }

  void onChunk(List<int> chunk) {
    if (failed || chunk.isEmpty) {
      return;
    }
    onByteActivity?.call();

    int segmentStart = 0;
    for (int i = 0; i < chunk.length; i++) {
      final int byte = chunk[i];
      if (lastWasCr) {
        lastWasCr = false;
        if (byte == _lf) {
          segmentStart = i + 1;
          continue;
        }
      }

      if (byte == _cr || byte == _lf) {
        final int segmentLength = i - segmentStart;
        if (segmentLength > 0) {
          if (lineBuffer.length + segmentLength > maxLineBytes) {
            failStream(SnapshotTooLargeException(limitBytes: maxEventBytes));
            return;
          }
          lineBuffer.add(
            chunk is Uint8List
                ? Uint8List.sublistView(chunk, segmentStart, i)
                : chunk.sublist(segmentStart, i),
          );
        }
        if (byte == _cr) {
          lastWasCr = true;
        }
        processCompletedLine();
        if (failed) {
          return;
        }
        segmentStart = i + 1;
      } else if (lineBuffer.length + (i - segmentStart + 1) > maxLineBytes) {
        failStream(SnapshotTooLargeException(limitBytes: maxEventBytes));
        return;
      }
    }

    if (!failed && segmentStart < chunk.length) {
      final int trailingLength = chunk.length - segmentStart;
      if (lineBuffer.length + trailingLength > maxLineBytes) {
        failStream(SnapshotTooLargeException(limitBytes: maxEventBytes));
        return;
      }
      lineBuffer.add(
        segmentStart == 0
            ? chunk
            : (chunk is Uint8List
                  ? Uint8List.sublistView(chunk, segmentStart)
                  : chunk.sublist(segmentStart)),
      );
    }
  }

  controller = StreamController<String>(
    onListen: () {
      byteSubscription = byteStream.listen(
        onChunk,
        onError: (Object error, StackTrace stackTrace) {
          if (!failed && !controller.isClosed) {
            controller.addError(error, stackTrace);
          }
        },
        onDone: () {
          lineBuffer.clear();
          currentEvent = null;
          dataLines.clear();
          dataBytes = 0;
          if (!failed && !controller.isClosed) {
            unawaited(controller.close());
          }
        },
        cancelOnError: false,
      );
    },
    onPause: () => byteSubscription?.pause(),
    onResume: () => byteSubscription?.resume(),
    onCancel: () async {
      lineBuffer.clear();
      currentEvent = null;
      dataLines.clear();
      dataBytes = 0;
      await byteSubscription?.cancel();
    },
  );

  return controller.stream;
}
