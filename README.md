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

Load http://localhost:8080 in your browser.

## Development


To work in developer mode, create a symlink to `docker-compose-dev.yml`

```sh
ln -s docker-compose-dev.yml docker-compose.override.yml
```

This will build and run dev versions for all containers.


होश में रहना ही समझदारी की पहली सीढ़ी है।
