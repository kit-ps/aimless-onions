#!/bin/bash
set -euo pipefail

LOAD=false
PERSIST=false
JUPYTER=true
BENCHES=true
TCPDUMP=""

while (( $# )) ; do
    case "$1" in
        --load ) LOAD=true ; shift ;;
        --persist ) PERSIST=true ; shift ;;
        --benches-only ) JUPYTER=false ; shift ;;
        --jupyter-only ) BENCHES=false ; shift ;;
        --tcpdump ) TCPDUMP="--tcpdump" ; shift ;;
    esac
done

if [ "$LOAD" == true ] ; then
    cp -ar persistance/aimless-onions/criterion-results aimless-onions || :
    cp -ar persistance/aimless-onions/target aimless-onions || :
    cp -ar persistance/sphinx-benchmarks/target sphinx-benchmarks || :
    cp -ar persistance/tor-consensus aimless-onions/tor-consensus || :
    cp -ar persistance/nym-mixnodes nym-mixnodes || :
    cp -ar persistance/onion_sizes.csv aimless-onions || :
    cp -ar persistance/sphinx_onion_sizes.csv sphinx-benchmarks || :
fi

if [ "$BENCHES" == true ] ; then
    scripts/prepare-consensus.sh

    ( cd sphinx-benchmarks && ./run.sh )

    # Here, we only run what we're interested in
    (
        cd aimless-onions
        cargo bench -- client/wrap/3/./1024
        cargo bench -- client/wrap/./3/1024
        cargo bench -- client/wrap/3/3/.+
        cargo bench -- relay/unwrap/./1024
        cargo bench -- relay/unwrap/3/.+
        cargo bench -- relay/derive_cached/.
        cargo bench -- authority/keygen

        ./run_benchmarks.sh 1 64 optimized

        cargo run --bin=onion_sizes --release >onion_sizes.csv
    )

    (
        cd tor-diff-size
        python3 diffsize.py
    )

    zstd aimless-onions/tor-consensus

    echo
    echo
    echo "Tor consensus size (estimate):"
    du --bytes aimless-onions/tor-consensus.zst
    echo
    echo
    echo "Tor consensus size (transmitted data):"
    echo "(can take 30 minutes to appear)"
    python3 tor-measurement-script/measure.py "$TCPDUMP"
    echo
    echo
    echo "Nym consensus size:"
    du --bytes nym-mixnodes
    echo
    echo
    echo "Consensus diff sizes:"
    du --bytes tor-diff-size/consensuses/*.diff
    echo
    echo
fi

if [ "$PERSIST" == true ] ; then
    mkdir -p persistance/aimless-onions/target
    mkdir -p persistance/sphinx-benchmarks/target
    cp -ar aimless-onions/criterion-results persistance/aimless-onions/ || :
    cp -ar aimless-onions/target/criterion persistance/aimless-onions/target/ || :
    cp -ar sphinx-benchmarks/target/criterion persistance/sphinx-benchmarks/target/ || :
    cp -ar aimless-onions/tor-consensus persistance/ || :
    cp -ar nym-mixnodes persistance/ ||:
    cp -ar aimless-onions/onion_sizes.csv persistance || :
    cp -ar sphinx-benchmarks/sphinx_onion_sizes.csv persistance || :
fi

if [ "$JUPYTER" == true ] ; then
    # We expect this to be in a container, so we're fine with 0.0.0.0 and root
    jupyter notebook --allow-root --ip 0.0.0.0 Benchmark.ipynb
fi
