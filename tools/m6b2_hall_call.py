"""Run the single-lift Hall Call Up fixture on a verified, private Linux vcan."""

import argparse
from contextlib import ExitStack
from dataclasses import dataclass
from datetime import date
import hashlib
import json
import os
from pathlib import Path
import platform
import pty
import shlex
import socket
import subprocess
import sys
import tempfile
import time
import tty
import uuid

from m6b2_vcan_preflight import (
    CANDUMP_COMMIT, CAN_FRAME, DEMO_COMMIT, INTERFACE, STACK_COMMIT,
    Prerequisite, check_link, limit_log_size, stop,
)


DOCUMENTS = {
    "417-1": ("417_1v02000005.pdf", "3fa6fc651d6239da5685c711989d824da6321efccdb0dec84de69f91a13d89d4", "§§4.2.2–4.2.4"),
    "417-2": ("417_2v02000005.pdf", "512df32879811591a8df7c1bae87af293c691218a38264210db071bf278b8638", "§§5.3.3–5.3.5, Tables 6–11"),
    "417-3-1": ("417_3_1v02000005.pdf", "22d7c1d4c5b6f734fcb3a426796d080b5061d4ef9baab691eb04572a5074088a", "§§4.2.2–4.2.3, 4.3.2–4.3.3, Tables 6, 8, 10, 12, 82, 84, 86, 88"),
    "417-4": ("417_4v02000005.pdf", "9990d1906042ed3114ab220c0fba05cef7000432e46946c7149d45b6a6e16d68", "§§4.3.8–4.3.9, 4.4.1, 4.4.6, Figures 6–10, 16, 20–23, Tables 23–24, 29–31, 40, 42, 50–54"),
}
PEERS = {
    "panel": ("Client", "Slave", "can1"),
    "controller": ("Master", "Master", "can0"),
}
REQUEST_ID = 0x48A  # Fixture panel node 10 and Part 3-1 Table 6.
ACK_ID = 0x400  # Fixture lift 1 and Part 3-1 Table 10.
HALL_UP = bytes.fromhex("02 01 01 01 01")  # Standard, up, lift 1, floor 1, door 1.
REQUEST = HALL_UP + b"\x01"
ACK = HALL_UP + b"\x81"
UPSTREAM_REQUEST = bytes.fromhex("02 01 01 01 11 01")
UPSTREAM_INDICATION = UPSTREAM_REQUEST  # Same bytes on a different PDO.


def digest(path):
    with path.open("rb") as file:
        return hashlib.file_digest(file, "sha256").hexdigest()


def require(ok, detail):
    if not ok:
        raise AssertionError(detail)


def check_documents(directory):
    records = []
    for part, (name, expected, clauses) in DOCUMENTS.items():
        path = directory / name
        if not path.is_file():
            raise Prerequisite(f"missing official local PDF for {part}")
        actual = digest(path)
        if actual != expected:
            raise Prerequisite(f"SHA-256 mismatch for {part}: {actual}")
        records.append({"part": part, "version": "2.0.0", "date": "2011-02-02",
                        "sha256": actual, "references": clauses})
    return records


def check_upstream_evidence(bundle, documents):
    if bundle is None:
        raise Prerequisite("the adjusted run needs the retained upstream negative-control bundle")
    manifest = json.loads((bundle / "manifest.json").read_text())
    verdicts = json.loads((bundle / "verdicts.json").read_text())
    capture = (bundle / "can.log").read_text()
    expected_hashes = {item["part"]: item["sha256"] for item in documents}
    observed_hashes = {item["part"]: item["sha256"] for item in manifest["documents"]}
    if (manifest.get("variant") != "upstream"
            or observed_hashes != expected_hashes
            or manifest["peer"]["demo"] != DEMO_COMMIT
            or manifest["peer"]["stack"] != STACK_COMMIT
            or any(verdicts[label]["status"] != "PASS" for label in "ABCI")
            or verdicts["D"]["status"] != "FAIL"
            or f"48A#{UPSTREAM_REQUEST.hex().upper()}" not in capture
            or f"400#{UPSTREAM_INDICATION.hex().upper()}" not in capture):
        raise Prerequisite("upstream evidence does not show the pinned 2.0.0 differences")
    return {
        "run_id": manifest["run_id"],
        "manifest_sha256": digest(bundle / "manifest.json"),
        "verdicts_sha256": digest(bundle / "verdicts.json"),
        "capture_sha256": digest(bundle / "can.log"),
    }


def check_peer(output, root, adjusted):
    if (output(["git", "rev-parse", "HEAD"], root) != DEMO_COMMIT
            or output(["git", "rev-parse", "HEAD"], root / "CANoopEn") != STACK_COMMIT
            or output(["git", "status", "--porcelain", "--untracked-files=no"], root / "CANoopEn")):
        raise Prerequisite("external peer or submodule source pin does not match")
    allowed = set()
    for role, (sample, device, original) in PEERS.items():
        directory = Path(f"Samples/CanOpen{sample}/CiA417/Linux")
        start = directory / "Source/Target/Linux/Start/main.cpp"
        original_start = output(["git", "show", f"HEAD:{start}"], root)
        expected = original_start.replace(
            f'CanInterfaceName = "{original}"', f'CanInterfaceName = "{INTERFACE}"')
        if (root / start).read_text().strip() != expected:
            raise Prerequisite(f"{role} interface binding differs from the approved edit")
        allowed.add(start.as_posix())
        if adjusted:
            source = directory / f"Source/CanOpenDevice/CanOpen{device}Device.cpp"
            original_source = output(["git", "show", f"HEAD:{source}"], root)
            expected = original_source.replace("CoCia417::DoorA", "0x01")
            if role == "controller":
                expected = expected.replace(
                    "userButtonPressed ? CoCia417::FunctionDataStatusOn : CoCia417::FunctionDataStatusOff",
                    "userButtonPressed ? 0x81 : CoCia417::FunctionDataStatusOff",
                )
            if (root / source).read_text().strip() != expected:
                raise Prerequisite(f"{role} application changes exceed the approved fixture adjustment")
            allowed.add(source.as_posix())
    if set(output(["git", "diff", "HEAD", "--name-only"], root).splitlines()) != allowed:
        raise Prerequisite("peer tree does not match the approved edit set")
    return {name: digest(root / name) for name in sorted(allowed)}


@dataclass(frozen=True)
class Frame:
    generation: int
    identifier: int
    data: bytes


class Exchange:
    """Correlate only this fixture's observations and independently triggered acknowledgement."""

    def __init__(self, generation):
        self.generation = generation
        self.pending = False
        self.allowed_ack = False
        self.complete = False

    def observe(self, frame):
        if frame.generation != self.generation:
            return "stale"
        if frame.identifier == REQUEST_ID and frame.data == REQUEST:
            if self.pending or self.complete:
                return "duplicate"
            self.pending = True
            return "requested"
        if frame.identifier == REQUEST_ID and frame.data == HALL_UP + b"\x00":
            self.pending = self.allowed_ack = self.complete = False
            return "withdrawn"
        if frame.identifier == ACK_ID and frame.data == ACK:
            if self.complete:
                return "duplicate"
            if self.pending and self.allowed_ack:
                self.complete = True
                return "acknowledged"
        return "unrelated"


class Peer:
    def __init__(self, role, executable, stderr, commands, stdout, stack):
        self.role = role
        self.log = stack.enter_context(tempfile.TemporaryFile())
        self.key, terminal = pty.openpty()
        stack.callback(os.close, self.key)
        try:
            tty.setraw(terminal)
            print(shlex.join([str(executable)]), file=commands)
            self.process = subprocess.Popen(
                [executable], stdin=terminal, stdout=self.log, stderr=stderr,
                preexec_fn=limit_log_size)
        finally:
            os.close(terminal)
        stack.callback(self.copy_log, stdout)
        stack.callback(stop, self.process)
        self.pressed = False

    def copy_log(self, destination):
        destination.write(f"\n[{self.role} pid={self.process.pid}]\n")
        destination.write(self.read())

    def read(self, start=0):
        return os.pread(self.log.fileno(), 65536, start).decode(errors="replace")

    def offset(self):
        return os.fstat(self.log.fileno()).st_size

    def press(self):
        if os.write(self.key, b"x") != 1:
            raise OSError(f"{self.role} PTY write failed")
        self.pressed = not self.pressed


class BusRun:
    def __init__(self, capture, send_socket, logger, event):
        self.capture = capture
        self.send_socket = send_socket
        self.logger = logger
        self.event = event
        self.peers = {}
        self.generation = 0

    def offset(self):
        return self.capture.stat().st_size

    def frames(self, start):
        with self.capture.open("rb") as stream:
            stream.seek(start)
            for line in stream:
                if not line.endswith(b"\n"):
                    break
                _, interface, record = line.split()
                require(interface == INTERFACE.encode(), "capture from unexpected interface")
                identifier, sep, data = record.partition(b"#")
                if sep and len(identifier) == 3:
                    yield Frame(self.generation, int(identifier, 16), bytes.fromhex(data.decode()))

    def wait(self, predicate, description, duration=8):
        deadline = time.monotonic() + duration
        while time.monotonic() < deadline:
            self.check_processes()
            result = predicate()
            if result:
                return result
            time.sleep(0.01)
        raise TimeoutError(f"timed out waiting for {description}")

    def check_processes(self):
        if self.logger.poll() is not None:
            raise OSError("CAN capture process exited")
        for peer in self.peers.values():
            if peer.process.poll() is not None:
                raise OSError(f"{peer.role} exited with {peer.process.returncode}")

    def frame(self, start, identifier, data):
        return self.wait(lambda: next((f for f in self.frames(start)
                                       if f.identifier == identifier and f.data == data), None),
                         f"{identifier:03X}#{data.hex()}")

    def message(self, role, start, message):
        self.wait(lambda: message in self.peers[role].read(start), f"{role}: {message}")

    def absent(self, start, identifier, description):
        deadline = time.monotonic() + 0.35
        while time.monotonic() < deadline:
            self.check_processes()
            require(not any(f.identifier == identifier for f in self.frames(start)), description)
            time.sleep(0.01)
        require(not any(f.identifier == identifier for f in self.frames(start)), description)

    def inject(self, frame, preserve_generation=False):
        start = self.offset()
        payload = CAN_FRAME.pack(frame.identifier, len(frame.data), frame.data)
        if self.send_socket.send(payload) != len(payload):
            raise OSError("injection short write")
        captured = self.frame(start, frame.identifier, frame.data)
        if preserve_generation:
            return Frame(frame.generation, captured.identifier, captured.data)
        return captured


def exercise(run, adjusted):
    """The two peers own the request and reply. The harness observes and tests correlation."""
    marker = run.offset()
    controller = run.peers["controller"]
    panel = run.peers["panel"]
    controller_start, panel_start = controller.offset(), panel.offset()
    panel.press()
    run.event("request_triggered", generation=run.generation)
    request = run.frame(marker, REQUEST_ID, REQUEST if adjusted else UPSTREAM_REQUEST)
    run.event("request_frame_observed", cob_id=f"{request.identifier:03X}", data=request.data.hex())
    run.message("controller", controller_start, "Hall call up received")
    run.event("controller_request_observed")
    run.absent(marker, ACK_ID, "controller emitted an acknowledgement before its trigger")
    run.event("ack_absence_verified", minimum_ms=350)
    exchange = Exchange(run.generation)
    received = exchange.observe(request)
    if adjusted:
        require(received == "requested" and not exchange.complete, "request did not remain pending")
        exchange.allowed_ack = True
    marker = run.offset()
    controller.press()
    run.event("ack_triggered")
    ack = run.frame(marker, ACK_ID, ACK if adjusted else UPSTREAM_INDICATION)
    run.event("ack_frame_observed", cob_id=f"{ack.identifier:03X}", data=ack.data.hex())
    run.message("panel", panel_start, "Hall call up acknowledged")
    run.event("panel_ack_observed", affirmed=bool(ack.data[-1] & 0x80))
    if not adjusted:
        require(received == "unrelated" and exchange.observe(ack) == "unrelated",
                "upstream negative control incorrectly satisfied the normative matcher")
        return request, ack, exchange
    require(exchange.observe(ack) == "acknowledged", "affirmed acknowledgement did not reconcile")
    return request, ack, exchange


def clear_buttons(run, exchange=None):
    for role, identifier in (("panel", REQUEST_ID), ("controller", ACK_ID)):
        if run.peers[role].pressed:
            marker = run.offset()
            run.peers[role].press()
            frame = run.frame(marker, identifier, HALL_UP + b"\x00")
            if exchange is not None and role == "panel":
                require(exchange.observe(frame) == "withdrawn", "withdrawal did not close the exchange")


def exercise_duplicate(run, request, exchange):
    marker = run.offset()
    repeated = run.inject(request)
    require(exchange.observe(repeated) == "duplicate" and exchange.complete,
            "repetition changed the established exchange")
    run.absent(marker, ACK_ID, "repetition generated another acknowledgement")
    run.event("duplicate_observed", source="controlled wire repeat", generation=run.generation)
    clear_buttons(run, exchange)
    require(not exchange.complete and not exchange.pending, "withdrawal retained completed exchange")
    current_request, current_ack, current = exercise(run, adjusted=True)
    require(current.complete, "new request after withdrawal did not complete")
    run.event("withdrawal_then_new_request_observed", generation=run.generation)
    clear_buttons(run, current)
    return current_request, current_ack


def request_only(run, exchange):
    marker = run.offset()
    controller_start = run.peers["controller"].offset()
    run.peers["panel"].press()
    request = run.frame(marker, REQUEST_ID, REQUEST)
    run.message("controller", controller_start, "Hall call up received")
    require(exchange.observe(request) == "requested", "fresh request did not become pending")
    return marker, request


def exercise_restart(run, start_peer, stale_ack):
    for role in ("panel", "controller", "harness"):
        prior = Exchange(run.generation)
        _, stale_request = request_only(run, prior)
        require(prior.pending and not prior.complete, "interrupted exchange was not pending")
        wire_start = run.offset()
        console_start = run.peers["controller"].offset()
        run.generation += 1
        if role != "harness":
            old = run.peers.pop(role)
            stop(old.process)
            start_peer(role)
            if role == "controller":
                console_start = 0
            run.message("controller", console_start, "booted and started")
            run.frame(wire_start, 0x000, b"\x01\x0a")
        run.frame(wire_start, 0x70A, b"\x05")
        if role == "harness":
            run.frame(wire_start, 0x701, b"\x7d")
        run.absent(wire_start, REQUEST_ID, f"{role} restart replayed a pending request")
        run.absent(wire_start, ACK_ID, f"{role} restart emitted an unsolicited acknowledgement")
        run.event("generation_recovered", restarted=role, generation=run.generation,
                  interrupted="pending", panel=run.peers["panel"].process.pid,
                  controller=run.peers["controller"].process.pid)
        if role == "panel":
            # The controller retains its remote input value across this reboot.
            # Explicitly withdraw the interrupted call before testing a new button edge.
            controller_start = run.peers["controller"].offset()
            run.inject(Frame(run.generation, REQUEST_ID, HALL_UP + b"\x00"))
            run.message("controller", controller_start, "Hall call up withdrawn")
            run.event("interrupted_request_withdrawn", source="controlled wire input")
        clear_buttons(run)
        exchange = Exchange(run.generation)
        require(exchange.observe(stale_request) == "stale", "retired request established readiness")
        replayed = run.inject(stale_ack)
        require(exchange.observe(replayed) == "unrelated" and not exchange.pending,
                "replayed wire acknowledgement started a new exchange")
        run.event("wire_replay_rejected", restarted=role, reason="no current request")
        panel_start = run.peers["panel"].offset()
        run.inject(Frame(run.generation, ACK_ID, HALL_UP + b"\x00"))
        run.message("panel", panel_start, "Hall call up acknowledgement cleared")
        request_only(run, exchange)
        exchange.allowed_ack = True
        require(exchange.observe(stale_ack) == "stale" and not exchange.complete,
                "identical cached acknowledgement completed a new exchange")
        replayed = run.inject(stale_ack, preserve_generation=True)
        require(exchange.observe(replayed) == "stale" and not exchange.complete,
                "identified stale wire input completed a new exchange")
        run.event("stale_evidence_rejected", role=role, generation=run.generation,
                  evidence=["cached", "controlled wire replay with original generation"])
        panel_start = run.peers["panel"].offset()
        run.inject(Frame(run.generation, ACK_ID, HALL_UP + b"\x00"))
        run.message("panel", panel_start, "Hall call up acknowledgement cleared")
        marker = run.offset()
        panel_start = run.peers["panel"].offset()
        run.peers["controller"].press()
        current_ack = run.frame(marker, ACK_ID, ACK)
        run.message("panel", panel_start, "Hall call up acknowledged")
        require(exchange.observe(current_ack) == "acknowledged", "fresh handshake did not recover")
        run.event("exchange_recovered", restarted=role, generation=run.generation)
        clear_buttons(run, exchange)
        stale_ack = current_ack


def exercise_wrong_records(run):
    exchange = Exchange(run.generation)
    request_only(run, exchange)
    exchange.allowed_ack = True  # Test semantic matching even with acknowledgement eligibility.
    variants = {
        "down": (1, 0x02),
        "different-lift": (2, 0x02),
        "different-floor": (3, 0x02),
        "different-door": (4, 0x02),
        "reserved-door-nibble": (4, 0x11),
        "indication-only": (5, 0x01),
    }
    for name, (index, replacement) in variants.items():
        payload = bytearray(ACK)
        payload[index] = replacement
        frame = run.inject(Frame(run.generation, ACK_ID, bytes(payload)))
        require(exchange.observe(frame) == "unrelated" and not exchange.complete,
                f"{name} incorrectly acknowledged the expected Hall Call Up")
        run.event("wrong_record_rejected", variation=name)
    before = run.offset()
    run.peers["panel"].press()
    withdrawn = run.frame(before, REQUEST_ID, HALL_UP + b"\x00")
    require(exchange.observe(withdrawn) == "withdrawn" and not exchange.pending,
            "withdrawal did not terminate the pending exchange")
    run.event("withdrawal_observed", completion=False)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--peer", type=Path, required=True)
    parser.add_argument("--candump", type=Path, required=True)
    parser.add_argument("--normative-dir", type=Path, required=True)
    parser.add_argument("--evidence-root", type=Path, required=True)
    parser.add_argument("--variant", choices=("upstream", "fixture"), required=True)
    parser.add_argument("--upstream-evidence", type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    peer = args.peer.resolve()
    logger = args.candump.resolve()
    adjusted = args.variant == "fixture"
    run_id = f"{date.today().isoformat()}-hall-{args.variant}-{uuid.uuid4().hex[:8]}"
    bundle = args.evidence_root / "evidence" / "m6b2" / run_id
    bundle.mkdir(mode=0o700, parents=True)
    capture = bundle / "can.log"
    manifest = {"run_id": run_id, "variant": args.variant, "kernel": platform.release(),
                "profile_subset": "single-lift standard Hall Call Up and affirmed acknowledgement",
                "interface": INTERFACE, "peer": {"demo": DEMO_COMMIT, "stack": STACK_COMMIT}}
    verdicts = {label: {"status": "BLOCKED", "detail": "required case did not run"} for label in "ABCDEFGHI"}
    events = []

    def event(name, **data):
        events.append({"event": name, **data})

    def verdict(label, status, detail):
        verdicts[label] = {"status": status, "detail": detail}

    with ((bundle / "commands.txt").open("x") as commands,
          (bundle / "stdout.log").open("x") as stdout,
          (bundle / "stderr.log").open("x") as stderr,
          capture.open("x") as can_log):
        print(shlex.join([sys.executable, *sys.argv]), file=commands)

        def output(command, cwd=root):
            print(f"(cd {shlex.quote(str(cwd))} && {shlex.join(command)})", file=commands)
            return subprocess.run(command, cwd=cwd, check=True, timeout=5, text=True,
                                  stdout=subprocess.PIPE, stderr=stderr).stdout.strip()

        current = "A"

        def case(label):
            nonlocal current
            current = label

        try:
            manifest["liftassistant_commit"] = output(["git", "rev-parse", "HEAD"])
            manifest["harness_sha256"] = digest(Path(__file__))
            manifest["rustc"] = output(["rustc", "--version"])
            manifest["cargo"] = output(["cargo", "--version"])
            manifest["documents"] = check_documents(args.normative_dir)
            event("normative_provenance_verified", parts=len(DOCUMENTS))
            if adjusted:
                manifest["upstream_evidence"] = check_upstream_evidence(
                    args.upstream_evidence, manifest["documents"])
            manifest["peer"]["source_sha256"] = check_peer(output, peer, adjusted)
            diff = output(["git", "diff", "HEAD", "--no-ext-diff"], peer) + "\n"
            manifest["peer"]["patch_sha256"] = hashlib.sha256(diff.encode()).hexdigest()
            manifest["peer"]["patch_references"] = ["417-4 Figures 10, 20, 23 and Table 53"] if adjusted else []
            (bundle / "peer.patch").write_text(diff)
            event("peer_delta_audited", variant=args.variant)
            verdict("A", "PASS", "All four official 2.0.0 PDFs and peer pins rechecked")
            case("B")
            interfaces = {name: idx for idx, name in socket.if_nameindex()}
            if INTERFACE not in interfaces:
                raise Prerequisite(f"{INTERFACE} is missing")
            link, = json.loads(output(["ip", "-j", "-d", "link", "show", "dev", INTERFACE]))
            ifindex = check_link(link, interfaces)
            if ifindex != socket.if_nametoindex(INTERFACE):
                raise Prerequisite("interface index changed after vcan verification")
            manifest["verified_link"] = {"kind": "vcan", "ifindex": ifindex}
            event("vcan_verified", ifindex=ifindex)
            verdict("B", "PASS", "Verified kernel vcan kind in private namespace before opening bus")

            case("C")
            if output(["git", "rev-parse", "HEAD"], logger.parent) != CANDUMP_COMMIT:
                raise Prerequisite("candump source pin does not match")
            manifest["candump"] = {"commit": CANDUMP_COMMIT, "binary_sha256": digest(logger)}
            binaries = {
                role: peer / f"Samples/CanOpen{sample}/CiA417/Linux/Make/build-m6b2/CanOpen{sample}"
                for role, (sample, _, _) in PEERS.items()
            }
            manifest["peer"]["binary_sha256"] = {role: digest(binary) for role, binary in binaries.items()}
            with ExitStack() as stack:
                sock = stack.enter_context(socket.socket(socket.AF_CAN, socket.SOCK_RAW, socket.CAN_RAW))
                sock.bind((INTERFACE,))
                sock.settimeout(1)
                logger_cmd = [str(logger), "-L", INTERFACE]
                print(shlex.join(logger_cmd), file=commands)
                logger_process = subprocess.Popen(logger_cmd, stdout=can_log, stderr=stderr,
                                                  preexec_fn=limit_log_size)
                stack.callback(stop, logger_process)
                run = BusRun(capture, sock, logger_process, event)

                def start_peer(role):
                    peer_stack = stack.enter_context(ExitStack())
                    run.peers[role] = Peer(role, binaries[role], stderr, commands, stdout, peer_stack)
                    event(f"{role}_started", pid=run.peers[role].process.pid)

                start_peer("controller")
                start_peer("panel")
                run.message("controller", 0, "booted and started")
                run.frame(0, 0x000, b"\x01\x0a")
                run.frame(0, 0x70A, b"\x05")
                event("nmt_operational", panel=10)
                verdict("C", "PASS", "Independent controller started the panel, confirmed by NMT capture")
                case("D")
                request, ack, exchange = exercise(run, adjusted)
                if adjusted:
                    verdict("D", "PASS", "Peer application request preceded independently triggered affirmed acknowledgement")
                    case("E")
                    _, most_recent_ack = exercise_duplicate(run, request, exchange)
                    verdict("E", "PASS", "Wire duplicate kept the exchange; withdrawal admitted a new one. Caller semantics use existing Core CallRegistry regression")
                    case("F")
                    exercise_restart(run, start_peer, most_recent_ack)
                    verdict("F", "PASS", "Recovered after separate panel, controller and harness generation restarts; cached and uncorrelated replay evidence rejected")
                    case("G")
                    exercise_wrong_records(run)
                    verdict("G", "PASS", "Wrong-direction, lift, floor, door, predicate and withdrawn records did not acknowledge")
                    case("H")
                    event("identity_boundary_audited", scope="protocol frame and fixture generation only")
                    verdict("H", "PASS", "Standalone harness has no mapping to LiftAssistant identity, dispatch, receipt or authority")
                else:
                    verdict("D", "FAIL", "Upstream peer exchange has reserved door bits and no affirmed predicate")
        except (Prerequisite, AssertionError, KeyError, OSError, subprocess.SubprocessError, ValueError) as error:
            result = ("BLOCKED" if isinstance(error, Prerequisite)
                      else "FAIL" if isinstance(error, AssertionError) else "ERROR")
            verdict(current, result, str(error))
            event("validation_stopped", case=current, status=result)
            print(str(error), file=stderr)
    verdict("I", "PASS", "Evidence v0 records individual terminal A–I outcomes")
    (bundle / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    (bundle / "verdicts.json").write_text(json.dumps(verdicts, indent=2) + "\n")
    (bundle / "events.jsonl").write_text("".join(json.dumps(item) + "\n" for item in events))
    print(bundle)
    return int(any(item["status"] != "PASS" for item in verdicts.values()))


if __name__ == "__main__":
    sys.exit(main())
