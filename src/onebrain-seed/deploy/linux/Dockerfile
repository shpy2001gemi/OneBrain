# Build stage
FROM rust:1.80-slim AS builder
WORKDIR /app
COPY . .
RUN cargo build --release -p onebrain-seed

# Runtime stage
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/onebrain-seed /usr/local/bin/

EXPOSE 4242
ENTRYPOINT ["onebrain-seed"]
CMD ["--port", "4242", "--name", "Seed", "--max-peers", "10000"]
