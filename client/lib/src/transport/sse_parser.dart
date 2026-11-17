import 'dart:async';
import 'dart:convert';

/// Transforms a raw UTF-8 byte stream from `GET /api/experimental/v0/events`
/// into complete `snapshot` event JSON payload strings.
///
/// Follows SSE framing rules:
/// - ignores comment/keepalive lines beginning with `:` (such as `: keepalive`);
/// - supports `\n`, `\r\n`, and `\r` line endings across arbitrary chunk splits;
/// - concatenates multiple `data:` lines within one event using `\n`;
/// - emits payloads only when `event` is `"snapshot"` and `data` is non-empty.
Stream<String> parseSseSnapshotStream(
  Stream<List<int>> byteStream, {
  void Function()? onByteActivity,
}) {
  late StreamController<String> controller;
  StreamSubscription<String>? lineSubscription;

  String? currentEvent;
  final List<String> dataLines = <String>[];

  void dispatchEvent() {
    if (currentEvent == 'snapshot' && dataLines.isNotEmpty) {
      controller.add(dataLines.join('\n'));
    }
    currentEvent = null;
    dataLines.clear();
  }

  controller = StreamController<String>(
    onListen: () {
      final Stream<List<int>> monitoredBytes = byteStream.map((List<int> chunk) {
        if (chunk.isNotEmpty) {
          onByteActivity?.call();
        }
        return chunk;
      });

      lineSubscription = monitoredBytes
          .transform(utf8.decoder)
          .transform(const LineSplitter())
          .listen(
            (String line) {
              if (line.isEmpty) {
                dispatchEvent();
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
                dataLines.add(value);
              }
            },
            onError: (Object error, StackTrace stackTrace) {
              if (!controller.isClosed) {
                controller.addError(error, stackTrace);
              }
            },
            onDone: () {
              if (!controller.isClosed) {
                unawaited(controller.close());
              }
            },
            cancelOnError: false,
          );
    },
    onCancel: () async {
      await lineSubscription?.cancel();
    },
  );

  return controller.stream;
}
