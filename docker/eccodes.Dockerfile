FROM rust:bookworm

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential \
        pkg-config \
        libeccodes-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /workspace
ENV CARGO_TERM_COLOR=always
