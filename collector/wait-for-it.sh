#!/usr/bin/env bash
# wait-for-it.sh

set -e

HOST=$1
shift
CMD=$@

while ! nc -z "$(echo $HOST | cut -d':' -f1)" "$(echo $HOST | cut -d':' -f2)"; do
  echo "Waiting for $HOST..."
  sleep 2
done

echo "$HOST is up. Starting the application..."
exec $CMD
