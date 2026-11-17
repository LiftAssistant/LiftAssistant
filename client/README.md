# LiftAssistant Client

Independent read-only Flutter graphical client for **LiftAssistant Host** (`experimental/v0` HTTP snapshot + SSE stream).

## Architectural boundary

- Communicates **only** with an explicitly enabled `liftassistant-headless` API listener over:
  - `GET /api/experimental/v0/snapshot`
  - `GET /api/experimental/v0/events` (SSE `event: snapshot`)
- Links zero LiftAssistant Rust crates (`liftassistant-model`, `liftassistant-core`, `liftassistant-engine`, `liftassistant-rmf`, `liftassistant-headless`) and owns zero elevator truth or dispatch logic.
- Synchronizes strictly by replacing its read model with complete Host snapshots, accepting Host-local revision sequence resets across Host restarts without persistent cursors or event-log replay.
- Strictly **read-only**: exposes zero call, target, door, mode, or controller write paths.

## Target platforms

- **Linux desktop** (`flutter build linux` / `flutter run -d linux`)
- **Web** (`flutter build web --no-web-resources-cdn` for self-contained local/LAN hosting)
- **Android** (`flutter build apk`)

### Web same-origin note

`liftassistant-headless` intentionally omits wildcard CORS headers on its unauthenticated `experimental/v0` HTTP endpoints. When running in a browser, serve `build/web` from the same origin (for example behind a local/LAN reverse proxy forwarding `/api/experimental/v0/*` to the Host listener) so browser Same-Origin Policy permits snapshot and SSE requests.

### Android loopback note

- From an Android Emulator to a Host bound on the development machine, use `http://10.0.2.2:<port>` (when the Host binds an accessible interface) or run `adb reverse tcp:8080 tcp:8080` and connect to `http://127.0.0.1:8080`.
- `usesCleartextTraffic="true"` is enabled specifically for the local/LAN unauthenticated `http://` scope of `experimental/v0`; no TLS or authentication is claimed.

## Validation

~~~bash
cd client
dart analyze --fatal-infos
flutter test
~~~
