FROM rust:1.35 as build

RUN USER=root cargo new --bin binderslap

WORKDIR /binderslap

COPY Cargo.toml .
COPY Cargo.lock .

RUN cargo build --release && rm -rf ./src

COPY src/ ./src/

RUN rm ./target/release/deps/binderslap*

COPY DejaVuSans.ttf .

RUN cargo build --release

FROM rust:1.35-slim-stretch

COPY binderslap.gif . 
COPY --from=build /binderslap/target/release/binderslap .

CMD ["./binderslap"]