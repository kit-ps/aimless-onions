#!/bin/bash
set -euo pipefail

# Download a recent Tor consensus for the accuracy evaluation & aimless benchmarks
RECENT_CONSENSUS=$(date --utc '+%Y-%m-%d-00-00-00-consensus')
URL="https://collector.torproject.org/recent/relay-descriptors/consensuses/$RECENT_CONSENSUS"

curl -o "aimless-onions/tor-consensus" "$URL"

# Download the current set of Nym mixnodes
NYM_MIXNODE_API="https://validator.nymtech.net/api/v1/mixnodes/active"

curl -o "nym-mixnodes" "$NYM_MIXNODE_API"
