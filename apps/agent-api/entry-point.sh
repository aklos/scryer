#!/bin/bash
set -e

start_worker() {
    echo "Starting worker..."
    while true; do
        PYTHONPATH="$PYTHONPATH:/app/src" /app/.venv/bin/rq worker-pool default -n 1 || {
            echo "Worker stopped. Restarting..."
            sleep 30
        }
    done
}

start_worker &

# python src/main.py
exec /app/.venv/bin/python src/main.py