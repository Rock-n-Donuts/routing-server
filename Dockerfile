FROM rust
WORKDIR /app

RUN cargo install cargo-watch
RUN rustup component add rustfmt

CMD cargo watch -x run