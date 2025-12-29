# hosh

Hosh brings awareness to the uptime of light wallet servers across popular blockchains.

Initially the project is focused on monitoring Bitcoin's Electrum servers, and Zcash's Lightwalletd infrastructure.

Demo: [hosh.zec.rocks](https://hosh.zec.rocks)

## Why monitor light wallet uptime?

Blockchains can become very large.
To improve user experience, many digital asset projects allow "light wallets" to connect to remote servers which store the full synchronized copy of a blockchain, and are presumed to be trustworthy.

Some digital asset wallets assume the perfect uptime of these servers.

## How?

```
docker compose up
```

Load http://localhost:8080/zec (Zcash) or http://localhost:8080/btc (Bitcoin) in your browser.

## Development

For the simplest development experience with hot-reload:

```sh
docker compose -f docker-compose-dev-all.yml up
```

This runs all roles (web, checkers, discovery) in a single container with `cargo watch` for automatic recompilation.

Alternatively, to work with separate containers in developer mode:

```sh
ln -s docker-compose-dev.yml docker-compose.override.yml
docker compose up
```

## Architecture

Hosh is built as a unified Rust binary with role-based execution:

```sh
# Run specific roles
./hosh --roles web
./hosh --roles checker-btc,checker-zec
./hosh --roles all  # runs all roles (default)
```


होश में रहना ही समझदारी की पहली सीढ़ी है।
