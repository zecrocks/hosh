# Use Python 3.11-slim as the base image
FROM python:3.11-slim

# Install system dependencies
RUN apt-get update && apt-get install -y \
    wget \
    libsecp256k1-dev \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

# Set the working directory inside the container
WORKDIR /app

# Copy the application code into the container
COPY dash_app.py /app/dash_app.py

# Install Python dependencies
RUN pip install --no-cache-dir \
    pandas \
    "dash[diskcache]" \
    dash-bootstrap-components \
    requests \
    redis \
    nats-py>=2.6.0 \
    diskcache

# Expose the default port for the Dash app
EXPOSE 8050

# Command to run the Dash app
CMD ["python", "dash_app.py"]
