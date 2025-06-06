#!/usr/bin/python3
import datetime
import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

TOR = Path("/usr/bin/tor")
# You may hardcode this if the autodetection does not work:
NETWORK_DEVICE = None

def run_tor(tor_binary, tcpdump):
    transmitted_bytes = {}
    tor_args = [
        "--ignore-missing-torrc",
        "-f", "/dev/null",
        "HeartbeatPeriod", "1 seconds",
        "DataDirectory", os.getcwd(),
    ]
    cmdline = [tor_binary] + tor_args
    tor_process = subprocess.Popen(cmdline, stdout=subprocess.PIPE)

    # Do nothing while we're not fully bootstrapped yet
    for line in tor_process.stdout:
        if b'Bootstrapped 100%' in line:
            if tcpdump:
                tcpdump.terminate()
            break

    for line in tor_process.stdout:
        # Our wanted line is
        # Jul 17 15:56:48.000 [notice] While bootstrapping, fetched this many bytes: 651346 (consensus network-status fetch); 14103 (authority cert fetch); 11194240 (microdescriptor fetch)
        if b'While bootstrapping' in line:
            line = line.decode("ascii")
            _, data = line.rsplit(":", 1)
            parts = data.split(";")
            for part in parts:
                match = re.match("\\s*(\\d+) \\((.+)\\)", part)
                num_bytes, reason = match.group(1), match.group(2)
                num_bytes = int(num_bytes)
                transmitted_bytes[reason] = num_bytes
            break

    tor_process.terminate()
    tor_process.wait()

    return transmitted_bytes


def start_tcpdump():
    tcpdump = subprocess.Popen(
        ["sudo", "tcpdump", "-w", "packets.pcap", "-i", NETWORK_DEVICE, "inbound"],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.STDOUT,
    )
    return tcpdump


def finish_tcpdump(tcpdump):
    tcpdump.terminate()
    tcpdump.wait()
    total = 0
    if os.access("packets.pcap", os.R_OK):
        packets = subprocess.check_output(
            ["tcpdump", "-r", "packets.pcap", "tcp"],
            stderr=subprocess.STDOUT,
        )
        for line in packets.splitlines():
            if match := re.search(b"length (\\d+)", line):
                num_bytes = int(match.group(1))
                total += num_bytes
    return {"inbound": total}


def stat_files():
    file_sizes = {}
    wanted = [
        "cached-certs",
        "cached-microdesc-consensus",
        "cached-microdescs.new",
    ]
    for name in wanted:
        try:
            file_sizes[name] = os.stat(name).st_size
        except FileNotFoundError:
            pass
    return file_sizes


def detect_network_device():
    devices = subprocess.check_output(["ip", "addr"])
    for line in devices.split(b"\n"):
        if b"state UP" in line and b"LOOPBACK" not in line:
            device = line.split(b":")[1].strip().decode("ascii")
            return device

    for line in devices.split(b"\n"):
        # May happen in podman/docker
        if b"state UNKNOWN" in line and b"LOOPBACK" not in line:
            device = line.split(b":")[1].strip().decode("ascii")
            return device

    return None


def main():
    global NETWORK_DEVICE

    use_tcpdump = "--tcpdump" in sys.argv
    if not NETWORK_DEVICE and use_tcpdump:
        NETWORK_DEVICE = detect_network_device()
        print(f"Monitoring {NETWORK_DEVICE}", file=sys.stderr)

    result = {
        "when": datetime.datetime.now(datetime.UTC).isoformat(),
    }

    with tempfile.TemporaryDirectory() as base_dir:
        os.chdir(base_dir)
        if use_tcpdump:
            tcpdump = start_tcpdump()
        else:
            tcpdump = None
        result["tor"] = run_tor(TOR, tcpdump)
        if tcpdump:
            result["pcap"] = finish_tcpdump(tcpdump)
        else:
            result["pcap"] = {"inbound": 0}
        result["files"] = stat_files()
    print(json.dumps(result))


if __name__ == "__main__":
    main()
