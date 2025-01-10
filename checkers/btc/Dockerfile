FROM python:3.11-slim

ENV PYTHONUNBUFFERED=1

RUN apt-get update && apt-get install -y \
    wget \
    redis-tools \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

RUN pip install --no-cache-dir \
    requests \
    redis \
    nats-py

WORKDIR /app

COPY worker.py /app/

CMD ["python", "worker.py"] 