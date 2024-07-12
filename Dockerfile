# Builder
FROM ubuntu:22.04 as builder

ENV DEBIAN_FRONTEND=noninteractive
RUN apt update && apt install -y cargo && rm -rf /var/lib/apt/lists/* && mkdir /build

COPY Cargo.toml /build/Cargo.toml
COPY src /build/src/
COPY NOTICE /build/NOTICE
RUN cargo install --root / --path /build

# Runtime
FROM nvidia/cuda:12.4.1-base-ubuntu22.04 AS runtime

# mongeu (or rather the NVML crate we're using) is looking for a shared object
# without suffix. The base-container only provides one with suffix.
RUN ln -s /usr/lib/x86_64-linux-gnu/libnvidia-ml.so.1 /usr/lib/x86_64-linux-gnu/libnvidia-ml.so

COPY --from=builder /bin/mongeu /bin/mongeu

CMD ["/bin/mongeu"]
