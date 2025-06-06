#!/bin/bash
set -euo pipefail

START="${1:-1}"
STOP="${2:-64}"
MODE="${3:-full}"

echo "Going from $START to $STOP"

set_identity_type() {
    itype="$1"
    echo "Setting format::Identity = $itype"
    sed -i "s/pub type Identity = .*;/pub type Identity = $itype;/" src/format.rs
}

set_hierarchy_depth() {
    depth="$1"
    echo "Setting nodename::HIERARCHY_DEPTH = $depth"
    sed -i "s/pub const HIERARCHY_DEPTH: u8 = .*;/pub const HIERARCHY_DEPTH: u8 = $depth;/" src/nodename.rs
}

save_benchmarks() {
    depth="$1"
    target="criterion-results/depth-$1/"
    mkdir -p "$target"
    mv target/criterion/* "$target"
}

run_benchmarks() {
    if [ $MODE = "full" ] ; then
        cargo bench
    else
        cargo bench -- 'client/wrap/3/9/1024'
        cargo bench -- 'relay/unwrap/9/1024'
        cargo bench -- 'relay/derive_cached/9'
        cargo bench -- 'authority/keygen'
    fi
}

# We run all benchmarks with u64 as the depth
set_identity_type u64

for depth in $(seq $START $STOP) ; do
    if [ -x "criterion-results/depth-$depth" ] ; then
        echo "Skipping depth $depth because target folder exists"
    else
        if [ -x target/criterion ] ; then
            mv target/criterion target/.criterion.bck
        fi
        echo "Running benchmarks for depth $depth"
        set_hierarchy_depth $depth
        run_benchmarks
        save_benchmarks $depth
        if [ -x target/.criterion.bck ] ; then
            rm -r target/criterion
            mv target/.criterion.bck target/criterion
        fi
    fi
done
