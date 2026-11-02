# AiReader sync server.
#   docker build -t aireader-server server
#   docker run -d -p 8080:8080 -v aireader-data:/data aireader-server

FROM rust:1.99-slim-bookworm AS build
WORKDIR /src
# Dependencies first, so editing the code doesn't rebuild them.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs && touch src/lib.rs \
    && cargo build --release --locked \
    && rm -rf src
COPY src ./src
RUN touch src/main.rs src/lib.rs && cargo build --release --locked

FROM debian:bookworm-slim
RUN useradd --system --uid 10001 --home-dir /data aireader \
    && mkdir /data && chown aireader /data
COPY --from=build /src/target/release/aireader-server /usr/local/bin/aireader-server
USER aireader
ENV AIREADER_DATA=/data AIREADER_LISTEN=0.0.0.0:8080
VOLUME /data
EXPOSE 8080
ENTRYPOINT ["aireader-server"]
