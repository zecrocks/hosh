# Use Python 3.11 slim base image
FROM python:3.11-slim

# Set environment variables
ENV PYTHONUNBUFFERED=1

# Install dependencies
RUN apt-get update && apt-get install -y \
    wget \
    redis-tools \
    netcat-openbsd \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

# Install Python dependencies
RUN pip install --no-cache-dir \
    requests \
    redis \
    pycryptodomex \
    cryptography \
    nats-py

# Create working directory
WORKDIR /app

# Copy application files
COPY collector.py /app/collector.py

# Add the wait-for-it script to the root directory so it doesn't get overwritten when mounting
COPY wait-for-it.sh /wait-for-it.sh
RUN chmod +x /wait-for-it.sh

# Set the command to run the collector script with wait-for-it.sh
ENTRYPOINT ["/wait-for-it.sh", "btc-worker:5000", "--"]
CMD ["python", "/app/collector.py"]
