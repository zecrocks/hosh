# Use Python 3.11-slim as the base image
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
RUN pip install --no-cache-dir \
    pycryptodomex \
    cryptography \
    flask \
    flasgger

# Install Electrum
RUN pip install --no-cache-dir https://download.electrum.org/$ELECTRUM_VERSION/Electrum-$ELECTRUM_VERSION.tar.gz

# Create working directory
WORKDIR /electrum

# Copy the Flask app code and entrypoint script
COPY entrypoint.sh /electrum/entrypoint.sh
COPY api.py /electrum/api.py

# Make the entrypoint script executable
RUN chmod +x /electrum/entrypoint.sh

# Expose Flask API port (default 5000)
EXPOSE 5000

# Use the entrypoint script
ENTRYPOINT ["/electrum/entrypoint.sh"]
