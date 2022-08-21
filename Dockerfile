from rust:1.63 as builder

# cache dependencies by building project without source code
run cargo new --bin decide
workdir ./decide
copy ./Cargo.lock ./Cargo.lock
copy ./Cargo.toml ./Cargo.toml
run cargo build --release

# then, build the real application
run rm src/*.rs
add . ./
run rm ./target/release/deps/decide-*
run cargo install --path .

FROM debian:buster-slim
COPY --from=builder /usr/local/cargo/bin/decide /usr/local/bin/decide
copy static static

run groupadd decide && useradd -g decide decide
user decide

expose 8000

cmd ["decide", "0.0.0.0:8000"]
