#!/bin/bash

if [ -f .env ]; then
  export $(grep -v '^#' .env | xargs)
fi

REDIS_URL=${REDIS_URL:-"redis://default:password@localhost:6379"}

PYTHONPATH="$PYTHONPATH:/src" rq worker -u $REDIS_URL --with-scheduler