# Artifact Appendix

Paper title: **Aimless Onions: Mixing without Topology Information**

Artifacts HotCRP Id: **10**

Requested Badge: **Reproduced**

## Description

Our artifact contains the prototype implementation of Aimless Onions which
we've benchmarked against the Sphinx implementation of Nym. Additionally, we
include small tools that help us determine Tor's consensus (diff) size.

### Security/Privacy Issues and Ethical Concerns (All badges)

Our artifact does not pose any risk to the security or privacy of the
reviewer's machine.

One of the measurement scripts runs `tcpdump` to measure how much data Tor
downloads for its bootstrapping. Only the amount of data is extracted from the
dump and printed to the console. No further processing is done with the capture
file. This step is skipped by default and only runs if requested. 

## Basic Requirements (Only for Functional and Reproduced badges)

Our artifact requires the following:

### Hardware Requirements

Our artifact has no special hardware requirements.

We tested on a standard x64 CPU with 4 GiB RAM and ~5 GiB of storage needed.

### Software Requirements

Our artifact runs on a Linux system (tested on Fedora 42 and Debian 12). It requires:

* Python 3 with `matplotlib` and `jupyter`
* Rust (nightly)
* `curl`, `wget`
* (optionally) `tcpdump`, `iproute2`
* `tor`

We do provide a `Dockerfile`, in which case the only software requirement is a
`docker`/`podman` installation.

### Estimated Time and Storage Consumption

Time: Around 13 hours.

Storage: Around 5 GiB.

## Environment 

### Accessibility (All badges)

We provide our artifact at https://github.com/kit-ps/aimless-onions.
We use the tag `pets-2025` to mark the version of the artifact evaluation.

### Set up the environment (Only for Functional and Reproduced badges)

We suggest to use the container method to set up the environment:

```bash
git clone https://github.com/kit-ps/aimless-onions
cd aimless-onions
podman build -t aimless-onions .
```

### Testing the Environment (Only for Functional and Reproduced badges)

You can test the environment by running Aimless Onion's unit tests:

```bash
podman run --rm -ti aimless-onions scripts/test_environment.sh
```

## Artifact Evaluation (Only for Functional and Reproduced badges)

This sections includes the steps required to validate our claims.

### Main Results and Claims

Our paper makes the following claims:

#### Main Result 1: The allocation's MAPE is below 10% at 30 bits.

We evaluate the mean absolute percentage error between a relay's allocated
weight and its desired weight in different scenarios. This is described in
Section 9.1 and Figure 3.

#### Main Result 2: The time for onion creation, key derivation and key generation scales with hierarchy depth.

We benchmark those operations for various sizes of the hierarchy space and
expect the time to increase (roughly linearly) w.r.t. hierarchy depth. This is
described in Section 9.3 and Figure 4.

#### Main Result 3: Aimless is ~450 times slower than Sphinx.

We benchmark the onion operations for a fixed hierarchy depth, but for varying
path lengths/payload sizes/authority counts. We detail our findings in Section
9.4, Table 2, Table 3 and Figure 5.

Generally, we see that Aimless Onions is much slower than Sphinx, the time
scales linearly with authority count and path length, but a node can still
process ~33 onions per second.

#### Main Result 4: Aimless saves bandwidth if fewer than 105 onions per hour are sent

This is detailed in Section 9.6 and Figure 6.

As a sub-result, we find that the size of a Tor consensus (compressed, as it is
transmitted) is around 632 KiB, and a consensus diff is around 415 KiB (on
average over the last 10 consensuses).

### Experiments 

We refer to the README for a detailed walkthrough of the steps. We suggest
using the "Container with persisted state" method described at the end. Our
experiments produce all data up-front, and then a Jupyter notebook is used to
generate all graphs.

The exception to this are the sub-results of Result 4 (i.e. the Tor and Nym
consensus sizes). Those values are printed directly on the console by the
script, and are hardcoded/manually entered in the notebook:

```
Tor consensus size (estimate):
684490  aimless-onions/tor-consensus.zst


Tor consensus size (transmitted data):
(can take 30 minutes to appear)
{"when": "2025-06-05T13:58:09.450866+00:00", "tor": {"consensus network-status fetch": 655168, "authority cert fetch": 13371, "microdescriptor fetch": 4878018}, "pcap": {"inbound": 0}, "files": {"cached-certs": 20442, "cached-microdesc-consensus": 3094229, "cached-microdescs.new": 29505362}}


Nym consensus size:
100884  nym-mixnodes


Consensus diff sizes:
638110  tor-diff-size/consensuses/2025-06-05-04-00-00-consensus.diff
572335  tor-diff-size/consensuses/2025-06-05-05-00-00-consensus.diff
505286  tor-diff-size/consensuses/2025-06-05-06-00-00-consensus.diff
445942  tor-diff-size/consensuses/2025-06-05-07-00-00-consensus.diff
389098  tor-diff-size/consensuses/2025-06-05-08-00-00-consensus.diff
319915  tor-diff-size/consensuses/2025-06-05-09-00-00-consensus.diff
262193  tor-diff-size/consensuses/2025-06-05-10-00-00-consensus.diff
176799  tor-diff-size/consensuses/2025-06-05-11-00-00-consensus.diff
87700   tor-diff-size/consensuses/2025-06-05-12-00-00-consensus.diff
66      tor-diff-size/consensuses/2025-06-05-13-00-00-consensus.diff
```

All values are in bytes. For the transmitted data, we are interested in the
`"tor" -> "consensus network-status fetch"` value.

## Limitations (Only for Functional and Reproduced badges)

As our artifact uses up-to-date Tor and Nym consensuses, variations in their
size are to be expected. The notebook initially contains the values that we
have used for the paper submission.
