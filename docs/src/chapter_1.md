

# Hosh services



## Production services

Start up default services in this order (dependencies in parenthesis)

```sh
docker compose up
```

1. btc-backend (tor, nats)
2. dashboard (redis) - clear db
3. web (redis)
4. discovery (redis, btc-backend) - all servers will appear offline on web at first
5. checker-btc (btc-backend, redis, nats) - listening for work from nats at hosh.check.btc
6. publisher (redis, nats, checker-btc, checker-zec) - starts publishing check requests


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
