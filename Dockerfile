FROM rust:1.85-slim-bookworm AS builder

RUN apt-get update && \
    apt-get install -y musl-tools && \
    rm -rf /var/lib/apt/lists/* && \
    rustup target add x86_64-unknown-linux-musl

WORKDIR /build
COPY . .

RUN cargo build -p vac_cli --release --target x86_64-unknown-linux-musl

FROM gcr.io/distroless/static-debian12:nonroot

COPY --from=builder /build/target/x86_64-unknown-linux-musl/release/vac /usr/local/bin/vac

ENTRYPOINT ["/usr/local/bin/vac"]
CMD ["--help"]
