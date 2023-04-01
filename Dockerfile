from rust:1.67 as builder

# cache dependencies by building project without source code
run cargo new --bin decide
run cargo new --lib api
copy ./Cargo.toml ./Cargo.toml
copy ./Cargo.lock ./Cargo.lock
copy ./decide/Cargo.toml ./decide/Cargo.toml
copy ./api/Cargo.toml ./api/Cargo.toml
run cargo build --release --locked

# then, build the real application
run rm decide/src/*.rs api/src/*.rs
add ./decide ./decide
add ./api ./api
# FIXME: there's gotta be something better than this to force cargo to rebuild
run rm ./target/release/**/*decide*
run cargo install --path decide --locked

# build the client
from node:18 as clientbuilder
env NODE_ENV=production
workdir ./app
copy ./client/package.json ./client/package-lock.json ./
run npm install --omit=dev
copy ./client ./
run npm run build

FROM debian:buster-slim
env DEBIAN_FRONTEND=noninteractive
run apt update && apt install -y dumb-init && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/cargo/bin/decide /usr/local/bin/decide
COPY --from=clientbuilder app/dist static

run groupadd decide && useradd -g decide decide
user decide

expose 8000

entrypoint ["/usr/bin/dumb-init", "--"]
cmd ["/usr/local/bin/decide", "0.0.0.0:8000"]
