import argparse
import csv
import functools
import heapq
import itertools
import ipaddress
import json
import os
import random
import re
import subprocess
import socket
import sys
from dataclasses import dataclass
from fractions import Fraction
from math import floor
from pathlib import Path
from typing import Optional


URL = "https://raw.githubusercontent.com/datasets/geoip2-ipv4/refs/heads/main/data/geoip2-ipv4.csv"


class GeoIpLocator:
    def __init__(self):
        self.short_networks = []
        self.networks = {}

    def load(self, path):
        with open(path, "r") as geocsv:
            reader = csv.DictReader(geocsv)
            for line in reader:
                network = ipaddress.IPv4Network(line["network"])
                country_name = line["country_iso_code"]
                cont_code = line["continent_code"]

                if network.prefixlen < 8:
                    self.short_networks.append((network, (cont_code, country_name)))
                else:
                    first_octet = network.network_address.packed[0]
                    self.networks.setdefault(first_octet, []).append((network, (cont_code, country_name)))

    def locate(self, ip):
        ip = ipaddress.IPv4Address(ip)
        for network, location in self.short_networks:
            if ip in network:
                return location

        first_octet = ip.packed[0]
        for network, location in self.networks.get(first_octet, []):
            if ip in network:
                return location

        return (None, None)


if not os.path.exists("geoip2-ipv4.csv"):
    check_call(["wget", URL])

@dataclass
class Relay:
    name: str
    weight: int
    ip: str
    scaled_weight: Optional[Fraction] = None
    allocation_request: int = 0
    assigned_weight: Optional[Fraction] = None
    flags: tuple[int, int, int, int, int] = (0, 0, 0, 0, 0)

    def __eq__(self, other):
        return self.allocation_request == other.allocation_request

    def __lt__(self, other):
        # Reverse order (we need a max-heap)
        return self.allocation_request > other.allocation_request


def load_from_consensus(path):
    current_name = None
    relays = []
    with open(path, "r") as consensus_file:
        for line in consensus_file:
            if line.startswith("r "):
                _, current_name, _, _, _, _, ip, *_ = line.split(" ")
                flags = [0, 0, 0, 0, 0]
                valid = False
            elif line.startswith("s "):
                if "Fast" in line:
                    flags[0] = 1
                if "Guard" in line:
                    flags[1] = 1
                if "Stable" in line:
                    flags[2] = 1
                if "Exit" in line:
                    flags[3] = 1
                if "Running" in line and "Valid" in line:
                    valid = True
            elif match := re.match("w Bandwidth=(\\d+)", line):
                bandwidth = int(match.group(1))
                assert current_name
                if valid:
                    relays.append(Relay(current_name, bandwidth, ip, flags=tuple(flags)))
                current_name = None
    return relays


def load_from_nym_mixnodes(path):
    nodes = []
    with open(path, "r") as nym_file:
        data = json.load(nym_file)
        for node in data:
            current_name = node["bond_information"]["mix_node"]["identity_key"]
            bandwidth = 1
            ip = node["bond_information"]["mix_node"]["host"]
            try:
                ipaddress.IPv4Address(ip)
            except ipaddress.AddressValueError:
                ip = socket.gethostbyname(ip)
            flags = (0, 0, 0, 0, 0)
            relay = Relay(current_name, bandwidth, ip, flags=flags)
            nodes.append(relay)
    return nodes


def scale_weights(relays):
    total_weight = sum(relay.weight for relay in relays)
    for relay in relays:
        if total_weight:
            relay.scaled_weight = Fraction(relay.weight, total_weight)
        else:
            relay.scaled_weight = 0


def allocate_weights(relays, depth, overallocate=False):
    queue = relays[:]
    allocation_space_size = 1 << depth
    for relay in relays:
        relay.allocation_request = floor(allocation_space_size * relay.scaled_weight)
        relay.assigned_weight = Fraction(0)

    current_size = allocation_space_size
    num_free = 1

    heapq.heapify(queue)
    while queue and current_size:
        next_alloc = heapq.heappop(queue)

        if next_alloc.allocation_request < current_size:
            current_size >>= 1
            num_free *= 2
        else:
            next_alloc.allocation_request -= current_size
            assert next_alloc.allocation_request >= 0
            assert next_alloc.allocation_request < current_size
            next_alloc.assigned_weight += Fraction(current_size, allocation_space_size)
            num_free -= 1
        if next_alloc.allocation_request:
            heapq.heappush(queue, next_alloc)

    if not overallocate:
        return

    # Allocate remaining chunks to biggest relays, even if that overallocates
    while queue and num_free:
        next_alloc = heapq.heappop(queue)
        next_alloc.assigned_weight += Fraction(1, allocation_space_size)
        num_free -= 1


def mape(relays):
    """Mean Absolute Percentage Error"""
    nonzero_relays = [relay for relay in relays if relay.scaled_weight]
    if not nonzero_relays:
        return 0.0
    error_sum = sum(abs((r.scaled_weight - r.assigned_weight) / r.scaled_weight) for r in nonzero_relays)
    return 100 * error_sum / len(relays)


def strfloat(x):
    return format(float(x), ".20f")


@dataclass
class Result:
    depth: int
    mape_without: float
    mape_with: float


def evaluate(relays, depth_from=1, depth_to=64):
    scale_weights(relays)

    for depth in range(depth_from, depth_to + 1):
        allocate_weights(relays, depth)
        without_overalloc = float(mape(relays))
        allocate_weights(relays, depth, overallocate=True)
        with_overalloc = float(mape(relays))
        yield Result(depth, without_overalloc, with_overalloc)


def evaluate_buckets(buckets, depth_from=1, depth_to=64):
    relays = list(itertools.chain.from_iterable(buckets.values()))

    for bucket in buckets.values():
        scale_weights(bucket)

    for depth in range(depth_from, depth_to + 1):
        for bucket in buckets.values():
            allocate_weights(bucket, depth)
        without_overalloc = float(mape(relays))
        for bucket in buckets.values():
            allocate_weights(bucket, depth, overallocate=True)
        with_overalloc = float(mape(relays))
        yield Result(depth, without_overalloc, with_overalloc)


def evaluate_buckets_optimal(buckets, depth_from=1, depth_to=64):
    relays = list(itertools.chain.from_iterable(buckets.values()))

    for bucket in buckets.values():
        scale_weights(bucket)

    for depth in range(depth_from, depth_to + 1):
        for flags, bucket in buckets.items():
            if flags[1] in {"nan", "EU"}:
                allocate_weights(bucket, depth + 1)
            else:
                allocate_weights(bucket, depth)
        without_overalloc = float(mape(relays))
        for flags, bucket in buckets.items():
            if flags[1] in {"nan", "EU"}:
                allocate_weights(bucket, depth + 1, overallocate=True)
            else:
                allocate_weights(bucket, depth, overallocate=True)
        with_overalloc = float(mape(relays))
        yield Result(depth, without_overalloc, with_overalloc)


def bucketize(ip_locator, relays):
    buckets = {}
    for relay in relays:
        relay.flags = relay.flags[:4] + (ip_locator.locate(relay.ip)[1], )
        buckets.setdefault(relay.flags, []).append(relay)
    return buckets


def bucketize_coarse(ip_locator, relays):
    buckets = {}
    for relay in relays:
        flags = (relay.flags[3], ip_locator.locate(relay.ip)[0])
        buckets.setdefault(flags, []).append(relay)
    return buckets


def print_evaluation(evaluation):
    print("{:<5} | {:>22} | {:>22}".format("Depth", "w/o overalloc", "w/ overalloc"))
    print("-+-".join(["-" * 5, "-" * 22, "-" * 22]))
    for result in evaluation:
        print(f"{result.depth:<5} | {result.mape_without:>22} | {result.mape_with:>22}")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--consensus", help="Consensus file to use")
    parser.add_argument("--from", type=int, help="Starting depth", default=1, dest="from_")
    parser.add_argument("--to", type=int, help="Stopping depth", default=64)
    parser.add_argument("--subset", type=float, help="Share of the consensus to use", default=1.0)
    parser.add_argument("--nym", help="Use nym mixnodes instead of Tor consensus", action="store_true")

    args = parser.parse_args()

    if not args.consensus:
        if not args.nym:
            consensus_path = next(Path(".").rglob("????-??-??-??-??-??-consensus"))
        else:
            consensus_path = Path("nym-mixnodes")
        print(f"Info: using {consensus_path} as consensus")
    else:
        consensus_path = Path(args.consensus)

    if not args.nym:
        relays = load_from_consensus(consensus_path)
    else:
        relays = load_from_nym_mixnodes(consensus_path)

    if args.subset < 1.0:
        random.shuffle(relays)
        relays = relays[:int(args.subset * len(relays))]

    print(f"Info: using {len(relays)} relays")

    evaluation = evaluate(relays, depth_from=args.from_, depth_to=args.to)

    print_evaluation(evaluation)

    # Now we evaluate based on flags
    if not os.path.exists("geoip2-ipv4.csv"):
        subprocess.check_call(["wget", URL])
    loc = GeoIpLocator()
    loc.load("geoip2-ipv4.csv")

    print("")
    print("")
    print("")

    buckets = bucketize(loc, relays)
    print(f"Strict bucketization: we have {len(buckets)} buckets (out of {2**12})")

    evaluation = evaluate_buckets(buckets, depth_from=args.from_, depth_to=args.to)
    print_evaluation(evaluation)

    print("")
    print("")
    print("")

    buckets = bucketize_coarse(loc, relays)
    print(f"Coarse buckets (continent + exit flag): we have {len(buckets)} buckets (out of {7 * 2})")

    evaluation = evaluate_buckets(buckets, depth_from=args.from_, depth_to=args.to)
    print_evaluation(evaluation)

    print("")
    print("Optimal:")

    evaluation = evaluate_buckets_optimal(buckets, depth_from=args.from_, depth_to=args.to)
    print_evaluation(evaluation)


if __name__ == "__main__":
    main()
