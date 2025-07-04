# 🎯 Aimless Onions 🧅

This repository contains the code and tools we used to evaluate Aimless Onions
(PETS 2025).

Using this, you can re-run the benchmarks from the paper or use/modify our
prototype for further research.

> [!note]
> The following is a detailed list of requirements and steps to use the
> scripts. At the end of this README, there is a quick method to run everything
> via Docker and a prepared script.

## General structure

The main file is `Benchmarks.ipynb`, which is a Jupyter notebook that ties
together all results from the other "modules" and generates the graphs. It does
however not run the actual benchmarks to generate the results, those have to be
run beforehand.

The folder `aimless-onions/` contains our proof-of-concept implementation of
the Aimless Onions mix format, which we used to get the performance benchmarks
and onion sizes referenced in the paper.

The folder `sphinx-benchmarks/` contains the implementation of the Sphinx mix
format by Nym, which we used as a comparison point.

The folder `tor-diff-size/` contains the tool we've used to determine the size
of Tor consensus diffs.

The folder `tor-measurement-script/` contains the script we've used to
determine the amount of data Tor downloads when bootstrapping a new instance.

## Requirements

* No special hardware requirements
  * Tested on a laptop with a x64 CPU, 16 GiB of RAM, 3 GiB of storage
* A Linux system
  * Tested on Debian 12 and Fedora 42
* (optional) `podman`/`docker` to use our container
* Python with `matplotlib` and `jupyter`
* Rust nightly
* `curl`, `wget`
* (optional) `zstd` to estimate the compressed Tor consensus size
* (optional) `tor`, `tcpdump`, `iproute2` to measure the data needed for a
  consensus download

### Versions

For future reproducibility, we list the versions of the tools we've used here.
Other versions should work as well, as long as they are backwards compatible:

```
% podman --version
podman version 5.5.2

% python --version
Python 3.13.3

% jupyter --version
Selected Jupyter core packages...
IPython          : 8.36.0
ipykernel        : 6.29.3
ipywidgets       : 8.1.5
jupyter_client   : 8.6.1
jupyter_core     : 5.7.2
jupyter_server   : 2.15.0
jupyterlab       : 4.4.0
nbclient         : 0.10.2
nbconvert        : 7.16.4
nbformat         : 5.9.2
notebook         : 7.4.0
qtconsole        : not installed
traitlets        : 5.14.3

% cargo --version
cargo 1.88.0-nightly (0e93c5bf6 2025-04-05)

% curl --version
curl 8.11.1 (x86_64-redhat-linux-gnu) libcurl/8.11.1 OpenSSL/3.2.4 zlib/1.3.1.zlib-ng brotli/1.1.0 libidn2/2.3.8 libpsl/0.21.5 libssh/0.11.2/openssl/zlib nghttp2/1.64.0 OpenLDAP/2.6.9
Release-Date: 2024-12-11
Protocols: dict file ftp ftps gopher gophers http https imap imaps ipfs ipns ldap ldaps mqtt pop3 pop3s rtsp scp sftp smb smbs smtp smtps telnet tftp ws wss
Features: alt-svc AsynchDNS brotli GSS-API HSTS HTTP2 HTTPS-proxy IDN IPv6 Kerberos Largefile libz NTLM PSL SPNEGO SSL threadsafe TLS-SRP UnixSockets

% wget --version
GNU Wget2 2.2.0 - multithreaded metalink/file/website downloader

+digest +https +ssl/gnutls +ipv6 +iri +large-file +nls -ntlm -opie +psl -hsts
+iconv +idn2 +zlib -lzma +brotlidec +zstd -bzip2 -lzip +http2 +gpgme

Copyright (C) 2012-2015 Tim Ruehsen
Copyright (C) 2015-2024 Free Software Foundation, Inc.

License GPLv3+: GNU GPL version 3 or later
<http://www.gnu.org/licenses/gpl.html>.
This is free software: you are free to change and redistribute it.
There is NO WARRANTY, to the extent permitted by law.

Please send bug reports and questions to <bug-wget@gnu.org>.

% zstd --version
*** Zstandard CLI (64-bit) v1.5.7, by Yann Collet ***

% tor --version
Tor version 0.4.8.16.
This build of Tor is covered by the GNU General Public License (https://www.gnu.org/licenses/gpl-3.0.en.html)
Tor is running on Linux with Libevent 2.1.12-stable, OpenSSL 3.2.4, Zlib 1.3.1.zlib-ng, Liblzma 5.8.1, Libzstd 1.5.7 and Glibc 2.41 as libc.
Tor compiled with GCC version 15.0.1

% tcpdump --version
tcpdump version 4.99.5
libpcap version 1.10.5 (with TPACKET_V3)
OpenSSL 3.2.4 11 Feb 2025
64-bit build, 64-bit time_t

% ip -V
ip utility, iproute2-6.12.0, libbpf 1.5.0
```

### Setup

To set up (on a Debian system):

```bash
# Install the required packages
sudo apt install curl wget zstd python3 python3-pip python3-venv tor iproute2 tcpdump
# Install Rust (see rustup.rs)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup default nightly
# Create the Python virtual env (for the jupyter notebook)
python3 -m venv .venv
source .venv/bin/activate
# Install the Python requirements
pip install -r requirements.txt
# Test if everything works
./scripts/test_environment.sh
```

## Detailed walkthrough

(Note: The script `scripts/benches.sh` can be used to run all steps
automatically. It is referenced further below, in the section about the
containerized setup.)

1. (~1 sec) Download a recent Tor and Nym consensus:

   ```bash
   ./scripts/prepare-consensus.sh
   ```

2. (~2 min) Run the Sphinx benchmarks:

   ```bash
   ( cd sphinx-benchmarks && ./run.sh )
   ```

3. (~1 hour) Run the Aimless Onions benchmarks:

   ```bash
   cd aimless-onions
   cargo bench -- "client/wrap/3/./1024"
   cargo bench -- "client/wrap/./3/1024"
   cargo bench -- "client/wrap/3/3/.+"
   cargo bench -- "relay/unwrap/./1024"
   cargo bench -- "relay/unwrap/3/.+"
   cargo bench -- "relay/derive_cached/."
   cargo bench -- "authority/keygen"
   cd ..
   ```

4. (~2 min) Generate Aimless Onions sizes:

   ```bash
   ( cd aimless-onions && cargo run --bin=onion_sizes --release >onion_sizes.csv )
   ```

5. (~12 hours) Compute the benchmarks for the various hiearchy depths:

   ```bash
   ( cd aimless-onions && ./run_benchmarks.sh 1 64 optimized )
   ```

This will prepare all the data needed for the `Benchmarks.ipynb` notebook and
to generate the graphs. You can open the notebook with...

```bash
jupyter notebook Benchmark.ipynb
```

...and then select `Run` -> `Run All Cells`

In addition, there are some "hardcoded" values in the notebook:

6. `TOR_CONSENSUS_SIZE`: Size of a Tor consensus in bytes. Gathered using the
   `tor-measurement-script`:

   ```bash
   python3 tor-measurement-script/measure.py
   # to enable tcpdump:
   python3 tor-measurement-script/measure.py --tcpdump
   ```

   Note that the script requires a heartbeat from the Tor client. By default,
   the Tor client waits at least 30 minutes before giving a heartbeat (even if
   configured to be lower). To get a faster measurement, you can re-compile the
   client with a lower minimum heartbeat duration. We will not distribute this
   modification here.

   For the paper, we took hourly measurements over the course of a day and
   manually averaged the results.

   Note that you can get a good estimate by using a consensus file, compressing
   it with `zstd` and checking its size:

   ```bash
   zstd aimless-onions/tor-consensus
   du --bytes aimless-onions/tor-consensus.zst
   ```

7. `NYM_CONSENSUS_SIZE`: Size of the Nym "consensus" in bytes. Gathered by
   checking the size of `nym-mixnodes`:

   ```bash
   du --bytes nym-mixnodes
   ```

8. `TOR_DIFF_SIZE`: Value in bytes. Gathered via the tools in `tor-diff-size`:

   ```bash
   cd tor-diff-size
   python3 diffsize.py
   du --bytes consensuses/*.diff
   ```

## Docker option

We provide a way to run the benchmarks automatically in a containerized
environment, which has a system that is set up with the proper requirements.

The script `scripts/benches.sh` runs the required benchmarks and should be used
inside the container. It takes around **12 hours** and produces **1.5 GiB** of
data. It can also be used outside of the container, but you must ensure that
the requirements are installed beforehand.

If you the script outside of a container, simply run it and it should drop you
to the Jupyter notebook for graph generation. You can then proceed to run the
cells to generate our graphs.

For docker, we provide two methods: One quick method which keeps all
intermediate artifacts (raw data) in the container and only extracts the
graphs, and one method which keeps the raw data on the host system.

> [!note]
> If you run with SELinux enabled (default on Fedora), you must append a ":Z"
> after the mount paths (-v "...:Z").

### Container-to-Notebook

To run the benchmarks and start a Jupyter notebook to generate the graphs, you
can run the following `podman`/`docker` commands:

```bash
podman build -t aimless-onions .
podman run \
  -v "$(pwd)/results:/aimless-onions/results" \
  -p 8888:8888 \
  --name aimless-benches \
  aimless-onions \
  scripts/benches.sh
```

This command will take around 13 hours to finish. At the end, it will print out
a Jupyter URL to open. In there, open the `Benchmark.ipynb` file and run all
cells. With this, you will produce the figures in the paper (annotated in the
notebook). All figures are saved as PDF files in the `results/` directory.

> [!note]
> This command will not persist the actual benchmark results outside of the
> container (only the generated graphs). If you plan to re-use the intermediate
> results later, make sure to manually copy them out of the container.

### Container with persisted intermediate data 

This option runs all benchmarks for hierarchy depths 1 to 64. It persists the
intermediate data, so you can re-run the graph generation later:

```bash
# Step 1: Build the container
podman build -t aimless-onions .
# Step 2: Prepare directories
mkdir persistance
# Step 3: Generate data (~13 hours)
podman run \
  -v "$(pwd)/persistance:/aimless-onions/persistance" \
  --rm \
  --name aimless-benches \
  aimless-onions \
  scripts/benches.sh --persist --benches-only
# Step 4: Run the notebook to generate graphs (~5 min)
podman run \
  -v "$(pwd)/results:/aimless-onions/results" \
  -v "$(pwd)/persistance:/aimless-onions/persistance" \
  -p 8888:8888 \
  --rm \
  --name aimless-graphs \
  aimless-onions \
  scripts/benches.sh --load --jupyter-only
```

This script runs the benchmarks as described above in the detailed walkthrough.

### Stopping the container

If `Crtl-C` does not work to exit the container, you can use the following
commands:

```bash
podman kill aimless-benches
podman rm aimless-benches
podman kill aimless-graphs
podman rm aimless-graphs
```

### tcpdump measurement

The script we use to check Tor's data during bootstrapping optionally runs
`tcpdump` to verify the numbers that the Tor client self-reports. By default,
`tcpdump` is *not* used.

In order to enable `tcpdump`, add `--cap-add=NET_ADMIN --cap-add=NET_RAW` to
the `docker`/`podman` invocation, and add `--tcpdump` to the arguments of
`benches.sh`. For example, the resulting command would look like this:

```bash
podman run \
  -v "$(pwd)/results:/aimless-onions/results" \
  -p 8888:8888 \
  --name aimless-benches \
  --cap-add=NET_ADMIN --cap-add=NET_RAW \
  aimless-onions \
  scripts/benches.sh --tcpdump
```

Note that other applications that transmit data during the measurement will
influence the reported value.

## License

The code in `aimless-onions`, `tor-measurement-script` and `tor-diff-size` is
released under the terms of the MIT license (see `LICENSE`).

The code in `sphinx-benchmarks` is imported (and adapted) from
https://github.com/nymtech/sphinx. Its original README and LICENSE (Apache 2.0)
have been preserved.
