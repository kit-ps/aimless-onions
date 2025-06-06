FROM rust:1.87-bookworm

RUN rustup default nightly

ENV DEBIAN_FRONTEND=noninteractive
RUN apt update &&\
    apt install -y curl wget zstd python3 python3-pip tor iproute2 tcpdump sudo &&\
    rm -rf /var/lib/apt/lists/*

WORKDIR /aimless-onions

COPY aimless-onions aimless-onions/
COPY scripts scripts/
COPY sphinx-benchmarks sphinx-benchmarks/
COPY tor-diff-size tor-diff-size/
COPY tor-measurement-script tor-measurement-script/
COPY Benchmark.ipynb \
     accuracy.py \
     doublechoose.py \
     geoip2-ipv4.csv \
     requirements.txt \
     .

RUN pip3 install --break-system-packages -r requirements.txt
