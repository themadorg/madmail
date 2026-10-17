FROM debian:trixie-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates passwd systemd libgcc-s1 scdoc groff-base \
    && rm -rf /var/lib/apt/lists/*
