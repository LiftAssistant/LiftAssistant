# LiftAssistant Client

Independent Flutter graphical client for **LiftAssistant Host** (`experimental/v0` HTTP snapshot + SSE stream), with an optional development-only Simulator `ServeFloor` operator workflow when connected to an explicitly opted-in loopback `sim-dev-server`.

## Architectural boundary

- Against normal `liftassistant-headless` Hosts, communicates **only** over the strictly read-only boundary:
  - `GET /api/experimental/v0/snapshot`
  - `GET /api/experimental/v0/events` (SSE `event: snapshot`)
  and exposes zero call, target, door, mode, or controller write controls.
- When connected to an explicitly opted-in loopback `sim-dev-server` (`cargo run -p liftassistant-headless --example sim-dev-server -- --listen 127.0.0.1:8080 --enable-dev-sim-control`), also consumes the isolated development-only `/api/dev-sim/v0/*` endpoints for Simulator-only `ServeFloor` preview, review, client-correlated submission (`submission_key` -> Engine `IntentId`), and read-only reconnect status projection without automatic resend.
- Links zero LiftAssistant Rust crates (`liftassistant-model`, `liftassistant-core`, `liftassistant-engine`, `liftassistant-rmf`, `liftassistant-headless`) and owns zero elevator truth, dispatch logic, or second operation lifecycle.
- Remains backend-agnostic: exposes no mode control, no Setting/Parameter surface, no RMF/LCE/real-hardware write path, and no command affordance on configured-but-unavailable cars.
- Synchronizes read state strictly by replacing its read model with complete Host snapshots, accepting Host-local revision sequence resets across Host restarts without persistent cursors or event-log replay.

## Target platforms

- **Linux desktop** (`flutter build linux` / `flutter run -d linux`)
- **Web** (`flutter build web --no-web-resources-cdn` for self-contained local/LAN hosting)
- **Android** (`flutter build apk`)

### Web same-origin note

`liftassistant-headless` intentionally omits wildcard CORS headers on its unauthenticated `experimental/v0` HTTP endpoints. When running in a browser, serve `build/web` from the same origin (for example behind a local/LAN reverse proxy forwarding `/api/experimental/v0/*` — and `/api/dev-sim/v0/*` only for local Simulator development — to the Host listener) so browser Same-Origin Policy permits requests.

### Android loopback note

- From an Android Emulator to a Host bound on the development machine, use `http://10.0.2.2:<port>` (when the Host binds an accessible interface) or run `adb reverse tcp:8080 tcp:8080` and connect to `http://127.0.0.1:8080`.
- `usesCleartextTraffic="true"` is enabled specifically for the local/LAN unauthenticated `http://` scope of `experimental/v0`; no TLS or authentication is claimed.

## Validation

~~~bash
cd client
dart analyze --fatal-infos
flutter test --concurrency=1
~~~
