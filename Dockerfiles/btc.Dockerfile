# Dockerfile
FROM python:3.11-slim

# Set environment variables
ENV ELECTRUM_VERSION=4.5.2 

# Install dependencies
RUN apt-get update && apt-get install -y \
    wget \
    libsecp256k1-dev \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

# Install Python dependencies
RUN pip install pycryptodomex cryptography

# Install Electrum
RUN pip install https://download.electrum.org/$ELECTRUM_VERSION/Electrum-$ELECTRUM_VERSION.tar.gz

# Create working directory
WORKDIR /electrum

# Command to dump peers
CMD ["sh", "-c", "electrum --testnet -o network getpeers"]


