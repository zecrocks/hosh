# hosh

Project Goal: Hosh checks the uptime of light wallet servers across popular blockchains, Bitcoin and Zcash.

# Context

## Major Files Directory List
- `checkers/`: Contains the checkers for the different blockchains.
- `contrib/`: Contains additional contributions and services, including backend and dashboard components.
- `discovery/`: Contains the discovery service for the different blockchains.
- `publisher/`: Contains the publisher service for the different blockchains.
- `web/`: Contains the web application files, including templates and static assets.
- `docker-compose.yml`: Configuration file for Docker Compose to manage multi-container applications.

## File Tree Structure

hosh/
├── .github/
│ └── workflows/
│ └── docker-compose-build.yml
├── checkers/
│ ├── btc/
│ │ ├── src/
│ │ │ └── routes/
│ │ │ │ ├── mod.rs
│ │ │ │ ├── api_info.rs
│ │ │ │ ├── health.rs
│ │ │ │ └── electrum.rs
│ │ │ └── worker/
│ │ │ │ ├── mod.rs
│ │ ├── Cargo.toml
│ │ ├── Cargo.lock
│ │ └── Dockerfile
│ ├── http/
│ │ └── src/
│ │ └── blockchain.rs
│ └── zec/
├── contrib/
│ ├── btc-backend-py/
│ │ ├── api.py
│ │ ├── Dockerfile
│ │ ├── entrypoint.sh
│ │ └── README.md
│ ├── dashboard/
│ │ ├── dash_app.py
│ │ ├── .gitignore
│ │ └── init.py
│ ├── example-nats/
│ │ └── README.md
│ └── example-zec/
│ └── src/
│ └── main.rs
├── discovery/
│ └── src/
│ └── main.rs
├── publisher/
│ ├── src/
│ │ ├── config.rs
│ │ ├── lib.rs
│ │ ├── models.rs
│ │ ├── publisher.rs
│ │ └── redis_store.rs
│ ├── Cargo.toml
│ └── Dockerfile
├── web/
│ ├── src/
│ │ └── main.rs
│ ├── static/
│ │ └── bootstrap.css
│ ├── templates/
│ │ ├── base.html
│ │ ├── blockchain_heights.html
│ │ ├── check.html
│ │ ├── index.html
│ │ └── layout.html
│ ├── Cargo.toml
│ └── Dockerfile
├── .dockerignore
├── .gitignore
├── CONTEXT.md
├── docker-compose.yml
├── LICENSE
└── README.md