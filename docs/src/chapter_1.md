

# Hosh services



## Production services

Start up default services:

```sh
docker compose up
```

Services and their dependencies:

1. **tor** - Tor proxy for accessing .onion addresses
2. **chronicler** (ClickHouse) - Database for storing check results and targets
3. **web** (depends on: chronicler) - Web interface and API server
4. **discovery** (depends on: chronicler, web) - Discovers and registers new servers to monitor
5. **checker-btc** (depends on: tor, chronicler, web) - Polls web every 10 seconds for Bitcoin Electrum servers to check
6. **checker-zec** (depends on: chronicler, web) - Polls web every 10 seconds for Zcash Lightwalletd servers to check
7. **checker-http** (depends on: tor, chronicler, web) - Polls web every 10 seconds for HTTP block explorers to check

**Check Frequency:** Each server is checked every 5 minutes. Checkers poll for work every 10 seconds and the job query excludes servers checked within the last 5 minutes.


## dev services


Spin up these auxiliary services to aid in development

```sh
docker compose --profile dev up 
```

service | description| port
--------|------------|-----
redis | stores state of all known servers | 6379
dashboard | displays content in redis db | 8050
d2-visualizer | displays dependency graph of all services | 8000
nostr-alert | monitors APIs and sends Nostr alerts (dev mode with hot-reload) | -
