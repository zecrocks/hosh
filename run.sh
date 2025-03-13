#!/bin/bash
export GIT_HASH=$(git rev-parse HEAD)
docker compose "$@" 