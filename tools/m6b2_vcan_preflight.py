"""Evidence v0 preflight for the pinned CANoopEnDemo panel on an isolated Linux bus."""

import argparse
from contextlib import ExitStack
import datetime
import hashlib
import json
from pathlib import Path
import platform
import resource
import shlex
import socket
import struct
import subprocess
import sys
import time
import uuid


INTERFACE = "vcan-m6b2"
DEMO_COMMIT = "e26f4ed600b55d2472d240bad85b422c21b84c7d"
STACK_COMMIT = "b421de2b80c69becb24f17e02512ba5e9188bae7"
CANDUMP_COMMIT = "834ceef1f1eabec67625389172eebf1050d2f42b"
PANEL = Path("Samples/CanOpenClient/CiA417/Linux")
PANEL_START = PANEL / "Source/Target/Linux/Start/main.cpp"
CAN_FRAME = struct.Struct("=IB3x8s")
NODE = 10  # Pin-specific test fixture, unrelated to LiftAssistant identity.
SDO_UPLOAD_DEVICE_TYPE = bytes.fromhex("40 00 10 00 00 00 00 00")
LOG_LIMIT = 64 * 1024


class Prerequisite(Exception):
    pass


def limit_log_size():
    resource.setrlimit(resource.RLIMIT_FSIZE, (LOG_LIMIT, LOG_LIMIT))


def stop(process):
    if process.poll() is not None:
        return
    process.terminate()
    try:
        process.wait(timeout=1)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=1)


def receive(bus, can_id, stage):
    deadline = time.monotonic() + 4
    while (remaining := deadline - time.monotonic()) > 0:
        bus.settimeout(remaining)
        try:
            identifier, length, data = CAN_FRAME.unpack(bus.recv(CAN_FRAME.size))
        except TimeoutError:
            break
        if identifier == can_id:
            return data[:length]
    raise TimeoutError(f"timed out waiting for {stage}")


def check_link(selected, interfaces):
    kind = selected.get("linkinfo", {}).get("info_kind")
    if kind != "vcan":
        raise Prerequisite(f"expected kernel link kind vcan for {INTERFACE}, got {kind!r}")
    if set(interfaces) != {"lo", INTERFACE}:
        raise Prerequisite("expected a private network namespace with only lo and the test vcan")
    if "UP" not in selected["flags"]:
        raise Prerequisite(f"{INTERFACE} is down")
    return selected["ifindex"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--peer", type=Path, required=True, help="prepared CANoopEnDemo source tree")
    parser.add_argument("--candump", type=Path, required=True)
    parser.add_argument("--evidence-root", type=Path, required=True, help="parent of evidence/m6b2")
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[1]
    peer = args.peer.resolve()
    logger = args.candump.resolve()
    run_id = f"{datetime.date.today().isoformat()}-preflight-{uuid.uuid4().hex[:8]}"
    bundle = args.evidence_root / "evidence" / "m6b2" / run_id
    bundle.mkdir(parents=True)
    events = []
    verdicts = {
        "A": {"status": "BLOCKED", "detail": "Official CiA 417 2.0.0 PDFs and checksums unavailable"},
        "B": {"status": "BLOCKED", "detail": "No authorized virtual interface verified"},
        "C": {"status": "BLOCKED", "detail": "No independent peer exchange"},
        **{case: {"status": "BLOCKED", "detail": "CiA 417 application profile run pending"}
           for case in "DEFGH"},
    }
    manifest = {
        "run_id": run_id,
        "scope": "SocketCAN and generic CANopen Device Type read",
        "profile_subset": None,
        "kernel": platform.release(),
        "python": platform.python_version(),
        "interface": INTERFACE,
        "verified_link_kind": None,
        "cia417_version": "2.0.0",
        "cia417_documents": [],
        "candidate_parts": ["417-1", "417-2", "417-3-1", "417-4"],
        "official_catalog": "https://www.can-cia.org/can-knowledge/cia-417-series-profile-for-lift-control-systems",
        "peer": {"repository": "https://github.com/xyntos-ch/CANoopEnDemo",
                 "expected_commit": DEMO_COMMIT, "expected_stack_commit": STACK_COMMIT},
    }

    with ((bundle / "commands.txt").open("x") as commands,
          (bundle / "stdout.log").open("x") as stdout,
          (bundle / "stderr.log").open("x") as stderr,
          (bundle / "can.log").open("x") as capture):
        print(shlex.join([sys.executable, *sys.argv]), file=commands)

        def output(arguments, cwd=repo):
            print(f"(cd {shlex.quote(str(cwd))} && {shlex.join(arguments)})", file=commands)
            return subprocess.run(
                arguments, cwd=cwd, stdout=subprocess.PIPE, stderr=stderr,
                text=True, timeout=5, check=True,
            ).stdout.strip()

        def launch(arguments, destination, stack):
            print(shlex.join(arguments), file=commands)
            process = subprocess.Popen(
                arguments, stdin=subprocess.DEVNULL, stdout=destination,
                stderr=stderr, preexec_fn=limit_log_size,
            )
            stack.callback(stop, process)
            return process

        case = "B"
        try:
            manifest["liftassistant_commit"] = output(["git", "rev-parse", "HEAD"])
            manifest["preflight_sha256"] = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
            manifest["rustc"] = output(["rustc", "--version"])
            manifest["cargo"] = output(["cargo", "--version"])
            interfaces = dict((name, index) for index, name in socket.if_nameindex())
            if INTERFACE not in interfaces:
                raise Prerequisite(f"{INTERFACE} is missing")
            links = json.loads(output(["ip", "-j", "-d", "link", "show", "dev", INTERFACE]))
            selected, = links
            manifest["link"] = selected
            ifindex = check_link(selected, interfaces)
            if ifindex != socket.if_nametoindex(INTERFACE):
                raise Prerequisite("interface index changed after link-kind verification")
            manifest["verified_link_kind"] = "vcan"
            events.append({"event": "virtual_interface_verified", "ifindex": ifindex})
            verdicts["B"] = {"status": "PASS", "detail": "Kernel reports vcan in the private namespace"}

            case = "C"
            demo_commit = output(["git", "rev-parse", "HEAD"], peer)
            stack_commit = output(["git", "rev-parse", "HEAD"], peer / "CANoopEn")
            if (demo_commit != DEMO_COMMIT or stack_commit != STACK_COMMIT
                    or output(["git", "diff", "HEAD", "--name-only"], peer) != PANEL_START.as_posix()):
                raise Prerequisite("peer pin or interface-only source change did not match")
            diff = output(["git", "diff", "HEAD", "--unified=0", "--no-ext-diff"], peer)
            changed = [line for line in diff.splitlines()
                       if line.startswith(("+", "-")) and not line.startswith(("+++", "---"))]
            if changed != [
                '-static constexpr const char* CanInterfaceName = "can1";',
                f'+static constexpr const char* CanInterfaceName = "{INTERFACE}";',
            ]:
                raise Prerequisite("peer pin or interface-only source change did not match")

            binary = peer / PANEL / "Make/build-m6b2/CanOpenClient"
            with binary.open("rb") as executable:
                binary_hash = hashlib.file_digest(executable, "sha256").hexdigest()
            manifest["peer"].update({
                "commit": demo_commit, "stack_commit": stack_commit, "role": "Linux call-panel sample",
                "local_change": f"SocketCAN bind target changed to {INTERFACE}",
                "binary_sha256": binary_hash,
            })
            candump_commit = output(["git", "rev-parse", "HEAD"], logger.parent)
            if candump_commit != CANDUMP_COMMIT:
                raise Prerequisite("candump source pin did not match")
            with logger.open("rb") as executable:
                manifest["candump"] = {
                    "commit": candump_commit,
                    "binary_sha256": hashlib.file_digest(executable, "sha256").hexdigest(),
                }
            with ExitStack() as stack:
                bus = stack.enter_context(socket.socket(socket.AF_CAN, socket.SOCK_RAW, socket.CAN_RAW))
                bus.bind((INTERFACE,))
                logger_process = launch([str(logger), "-L", "-n", "32", INTERFACE], capture, stack)
                panel_process = launch([str(binary)], stdout, stack)
                if receive(bus, 0x700 + NODE, "peer boot-up") != b"\x00":
                    raise AssertionError("unexpected initial peer boot-up state")
                events.append({"event": "peer_boot_up_observed", "pid": panel_process.pid, "node": NODE})

                # A captured startup frame establishes logger readiness before the request.
                deadline = time.monotonic() + 4
                while (bundle / "can.log").stat().st_size == 0:
                    if logger_process.poll() is not None or time.monotonic() >= deadline:
                        raise TimeoutError("CAN logger did not capture startup activity")
                    time.sleep(0.01)

                bus.settimeout(1)
                frame = CAN_FRAME.pack(0x600 + NODE, 8, SDO_UPLOAD_DEVICE_TYPE)
                if bus.send(frame) != len(frame):
                    raise OSError("short SocketCAN send")
                events.append({"event": "generic_sdo_read_submitted", "object": "1000:00"})
                response = receive(bus, 0x580 + NODE, "peer Device Type upload response")
                if response != bytes.fromhex("43 00 10 00 A1 01 00 00"):
                    raise AssertionError(f"unexpected Device Type upload response {response.hex()}")
                events.append({"event": "generic_sdo_read_response_observed", "object": "1000:00"})

                # Let candump flush both directions while the peer is still alive.
                deadline = time.monotonic() + 2
                while True:
                    recorded = (bundle / "can.log").read_text()
                    if "60A#4000100000000000" in recorded and "58A#43001000A1010000" in recorded:
                        break
                    if logger_process.poll() is not None or time.monotonic() >= deadline:
                        raise TimeoutError("CAN capture did not record both SDO directions")
                    time.sleep(0.01)
            verdicts["C"] = {"status": "PASS", "detail": "Pinned independent peer answered a generic CANopen read"}
        except (Prerequisite, AssertionError, OSError, subprocess.SubprocessError, ValueError) as error:
            status = ("BLOCKED" if isinstance(error, Prerequisite)
                      else "FAIL" if isinstance(error, AssertionError) else "ERROR")
            verdicts[case] = {"status": status, "detail": str(error)}
            events.append({"event": "preflight_stopped", "case": case, "status": status})
            print(str(error), file=stderr)

    verdicts["I"] = {"status": "PASS", "detail": "Preflight bundle retained with individual A-I verdicts"}
    (bundle / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    (bundle / "verdicts.json").write_text(json.dumps(dict(sorted(verdicts.items())), indent=2) + "\n")
    (bundle / "events.jsonl").write_text("".join(json.dumps(event) + "\n" for event in events))
    print(bundle)
    return 1  # This preflight cannot satisfy the CiA 417 application-profile matrix.


if __name__ == "__main__":
    sys.exit(main())
