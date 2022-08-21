from rust:1.63 as builder

# cache dependencies by building project without source code
run cargo new --bin rps
workdir ./rps 
copy ./Cargo.toml ./Cargo.toml
run cargo build --release

# then, build the real application
run rm src/*.rs
add . ./
run rm ./target/release/deps/rps-*
run cargo install --path .

FROM debian:buster-slim
COPY --from=builder /usr/local/cargo/bin/rps /usr/local/bin/rps

run groupadd rps && useradd -g rps rps
user rps

expose 8000

cmd ["rps", "0.0.0.0:8000"]
