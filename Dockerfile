from rust:1.63 as builder

# cache dependencies by building project without source code
run cargo new --bin decide
workdir ./decide
copy ./Cargo.lock ./Cargo.lock
copy ./Cargo.toml ./Cargo.toml
run cargo build --release --locked

# then, build the real application
run rm src/*.rs
add ./src ./src
run rm ./target/release/deps/decide-*
run cargo install --path . --locked

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
