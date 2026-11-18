"""M5.13 Linux loopback proof. Run from the repository root with Python 3."""

import contextlib
import http.client
import json
import math
import os
from pathlib import Path
import queue
import socket
import subprocess
import threading
import time


API = "/api/experimental/v0"


def test_binary():
    build = subprocess.run(
        ["cargo", "test", "-p", "liftassistant-headless", "--locked", "--no-run",
         "--message-format=json"],
        check=True, stdout=subprocess.PIPE, text=True,
    )
    for line in build.stdout.splitlines():
        item = json.loads(line)
        if item.get("executable") and item["target"]["name"] == "liftassistant-headless":
            return item["executable"]
    raise RuntimeError("headless test executable missing")


def until(check, description, seconds=8):
    deadline = time.monotonic() + seconds
    while not check():
        if time.monotonic() >= deadline:
            raise AssertionError(f"timed out: {description}")
        time.sleep(0.01)


class Host:
    def __init__(self, binary, cars, sources, port):
        self.cars = cars
        self.sources = sources
        self.process = subprocess.Popen(
            [binary, "--ignored", "--exact", "failure_matrix::api_soak_child", "--nocapture"],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True,
            env=dict(os.environ, LIFTASSISTANT_SOAK_CARS=str(cars),
                     LIFTASSISTANT_SOAK_SOURCES=str(sources), LIFTASSISTANT_SOAK_PORT=str(port)),
        )
        self.output = queue.Queue()
        self.reader = threading.Thread(target=self._read)
        self.reader.start()

    def _read(self):
        for line in self.process.stdout:
            _, found, value = line.partition("SOAK ")
            if found:
                self.output.put(json.loads(value))
            elif "FAILED" in line:
                print(line.rstrip(), flush=True)
        self.output.put(None)

    def receive(self):
        result = self.output.get(timeout=10)
        assert result is not None, "Host proof process exited"
        return result

    def command(self, command):
        self.process.stdin.write(command + "\n")
        self.process.stdin.flush()
        return self.receive()

    def connect(self):
        return http.client.HTTPConnection("127.0.0.1", self.port, timeout=12)

    def snapshot(self):
        with contextlib.closing(self.connect()) as connection:
            connection.request("GET", f"{API}/snapshot")
            response = connection.getresponse()
            assert response.status == 200
            assert response.getheader("Cache-Control") == "no-store"
            assert response.getheader("Access-Control-Allow-Origin") is None
            body = response.read()
        model = json.loads(body)
        self.check(model)
        return model["revision"], len(body)

    def check(self, model):
        assert model["api_version"] == "experimental/v0"
        assert model["readiness"] == "read_only"
        assert len(model["operational_cars"]) == self.cars
        assert len(model["sources"]) == self.sources
        assert len(model["recent_diagnostics"]) == 256

    def resources(self):
        proc = Path(f"/proc/{self.process.pid}")
        status = dict(line.split(":", 1) for line in (proc / "status").read_text().splitlines())
        targets = []
        for fd in (proc / "fd").iterdir():
            try:
                targets.append(os.readlink(fd))
            except FileNotFoundError:
                pass  # Connection close raced the /proc sample.
        return (int(status["VmRSS"].split()[0]), len(targets),
                int(status["Threads"]), sum(item.startswith("socket:") for item in targets))

    def cpu_ticks(self):
        fields = Path(f"/proc/{self.process.pid}/stat").read_text().rsplit(")", 1)[1].split()
        return int(fields[11]) + int(fields[12])

    def stop(self):
        self.process.stdin.write("stop\n")
        self.process.stdin.flush()
        assert self.process.wait(timeout=5) == 0
        try:
            socket.create_connection(("127.0.0.1", self.port), timeout=1).close()
        except ConnectionRefusedError:
            return
        raise AssertionError("API listener survived Host shutdown")


@contextlib.contextmanager
def start_host(binary, cars, sources, port=0):
    host = Host(binary, cars, sources, port)
    try:
        host.port = host.receive()["port"]
        yield host
    finally:
        if host.process.poll() is None:
            host.process.kill()
        host.process.wait(timeout=5)
        host.reader.join(timeout=5)
        host.process.stdin.close()
        host.process.stdout.close()


class Stream:
    def __init__(self, host, pause=False):
        self.host = host
        self.connection = host.connect()
        self.rows = []
        self.reading = threading.Event()
        self.closed = threading.Event()
        self.error = None
        if not pause:
            self.reading.set()
        self.thread = threading.Thread(target=self._read)
        self.thread.start()

    def _read(self):
        try:
            self.connection.request("GET", f"{API}/events")
            response = self.connection.getresponse()
            assert response.status == 200
            assert response.getheader("Content-Type").startswith("text/event-stream")
            while line := response.readline():
                if not line.startswith(b"data: "):
                    continue
                received = time.time_ns()
                parsed_at = time.perf_counter_ns()
                model = json.loads(line[6:])
                parse_ns = time.perf_counter_ns() - parsed_at
                self.host.check(model)
                revision = model["revision"]
                assert not self.rows or revision > self.rows[-1][0]
                self.rows.append((revision, received, parse_ns))
                assert self.reading.wait(timeout=12)
        except Exception as error:
            if not self.closed.is_set():
                self.error = error
        finally:
            self.connection.close()

    def reached(self, revision):
        if self.error:
            raise self.error
        return bool(self.rows and self.rows[-1][0] >= revision)

    def close(self):
        self.closed.set()
        self.reading.set()
        if self.connection.sock is not None:
            try:
                self.connection.sock.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
        self.thread.join(timeout=13)
        assert not self.thread.is_alive()
        if self.error:
            raise self.error


def p95_ms(values):
    ordered = sorted(values)
    return round(ordered[math.ceil(len(ordered) * 0.95) - 1] / 1e6, 3)


def timed_wave(host, count, rate, steps, roll=False):
    streams = []
    try:
        for _ in range(count):
            streams.append(Stream(host))
        initial, size = host.snapshot()
        until(lambda: all(stream.reached(initial) for stream in streams), "all SSE clients ready")
        sent = {}
        cpu_start = host.cpu_ticks()
        started = time.monotonic()
        for index in range(steps):
            time.sleep(max(0, started + (index + 1) / rate - time.monotonic()))
            row = host.command("roll" if roll and index == steps // 2 else "tick")
            sent[row["revision"]] = row
        latest = row["revision"]
        until(lambda: all(stream.reached(latest) for stream in streams), "all clients caught up")
        elapsed = time.monotonic() - started
        cpu_pct = 100 * (host.cpu_ticks() - cpu_start) / os.sysconf("SC_CLK_TCK") / elapsed
        rows = [row for stream in streams for row in stream.rows if row[0] in sent]
        result = {
            "clients": count, "target_hz": rate, "actual_hz": round(steps / elapsed, 2),
            "snapshot_bytes": size, "samples": len(rows), "updates": steps,
            "update_publish_p95_ms": p95_ms([row["update_publish_ns"] for row in sent.values()]),
            "publish_receive_p95_ms": p95_ms([received - sent[rev]["publish_ns"]
                                              for rev, received, _ in rows]),
            "client_parse_p95_ms": p95_ms([parse for _, _, parse in rows]),
            "host_cpu_pct_one_core": round(cpu_pct, 1), "resources": host.resources(),
        }
        assert host.snapshot()[0] == latest, "client churn changed Host state"
        return result
    finally:
        for stream in streams:
            stream.close()


def slow_and_fast(host):
    fast, slow = Stream(host), Stream(host, pause=True)
    try:
        initial, _ = host.snapshot()
        until(lambda: fast.reached(initial) and slow.reached(initial), "initial slow and fast state")
        for _ in range(192):
            row = host.command("tick")
        latest = row["revision"]
        until(lambda: fast.reached(latest), "fast client unaffected by closed receive window")
        slow.reading.set()
        until(lambda: slow.reached(latest), "slow client catches up to complete current state")
        assert len(slow.rows) < 193, "paused consumer received the entire burst"
        print(f"coalescing burst=192 fast={len(fast.rows) - 1} slow={len(slow.rows) - 1}",
              flush=True)
        host.stop()  # Leave both SSE peers connected during the shutdown.
        until(lambda: not fast.thread.is_alive() and not slow.thread.is_alive(), "SSE peers closed")
        return latest
    finally:
        fast.close()
        slow.close()


def main():
    binary = test_binary()
    for cars, sources in [(4, 2), (32, 8), (128, 16)]:
        with start_host(binary, cars, sources) as host:
            initial, size = host.snapshot()
            serializations = [host.command("measure") for _ in range(15)]
            assert all(row["bytes"] == size for row in serializations)
            print(f"scale cars={cars} sources={sources} bytes={size} "
                  f"serde_p95_ms={p95_ms([row['serialize_ns'] for row in serializations])}",
                  flush=True)
            if cars != 32:
                print("scale_sse", timed_wave(host, 1, 10, 5), flush=True)
                host.stop()
                continue
            medium_initial, medium_port = initial, host.port
            time.sleep(0.2)
            baseline = host.resources()
            for count in [1, 5, 20]:
                for rate in [1, 10, 20]:
                    print("perf", timed_wave(host, count, rate, max(3, rate * 2)), flush=True)
                    until(lambda: host.resources()[1:] == baseline[1:], "perf resources released")
            started = time.monotonic()
            print(f"soak idle baseline RSS_KiB/FD/threads/sockets={baseline}", flush=True)
            for wave in range(5):
                result = timed_wave(host, 20, 20, 180, roll=True)
                until(lambda: host.resources()[1:] == baseline[1:], "soak resources released")
                print(f"soak wave={wave + 1} active={result['resources']} "
                      f"idle={host.resources()}", flush=True)
            print(f"soak elapsed_seconds={time.monotonic() - started:.2f}", flush=True)
            latest = slow_and_fast(host)
    with start_host(binary, 32, 8, medium_port) as restarted:
        first, _ = restarted.snapshot()
        assert first == medium_initial < latest, "revision must start over on Host restart"
        stream = Stream(restarted)
        try:
            until(lambda: stream.reached(first), "reconnected client receives new Host snapshot")
            restarted.stop()
        finally:
            stream.close()
        print(f"restart revision {latest} -> {first}; listener and streams released", flush=True)


if __name__ == "__main__":
    main()
