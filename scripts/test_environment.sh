#!/bin/bash
set -euo pipefail
( cd aimless-onions && cargo test )
( cd sphinx-benchmarks && SPHINX_MAX_PATH_LENGTH=5 cargo test )
curl --help >/dev/null
wget --help >/dev/null
jupyter notebook --help >/dev/null
python3 -c 'import matplotlib'

echo
echo
echo "All good, your environment looks fine."
