FROM rust
WORKDIR /app

RUN cargo install cargo-watch

CMD cargo watch -x run