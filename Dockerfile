FROM rust:1.85-slim-bookworm as builder

WORKDIR /usr/src/vac
COPY . .

# Build with optimizations
RUN cargo build --release --workspace

# Runtime image
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/src/vac/target/release/vac /usr/local/bin/vac

ENTRYPOINT ["vac"]
CMD ["--help"]
