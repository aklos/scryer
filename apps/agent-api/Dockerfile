# Use the Python image as the base for building dependencies
FROM python:3.12.8 AS builder

# Environment variables to improve behavior and prevent bytecode files
ENV PYTHONUNBUFFERED=1 \
    PYTHONDONTWRITEBYTECODE=1

# Set the working directory inside the container
WORKDIR /app

# Create a virtual environment and install dependencies
RUN python -m venv .venv
COPY requirements.txt ./
RUN .venv/bin/pip install -r requirements.txt

# Final stage using a slimmer image for the production app
FROM python:3.12.8-slim

# Set the working directory inside the container
WORKDIR /app

# Install libpq and related dependencies
RUN apt-get update && apt-get install --no-install-recommends -y \
    libpq5 \
    libpq-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy the virtual environment and app code from the builder stage
COPY --from=builder /app/.venv .venv/
COPY . .
# COPY rqworker@.service /etc/systemd/system/

# Activate the virtual environment and run the FastAPI app
# CMD ["/app/.venv/bin/python", "src/main.py"]
CMD ["/app/entry-point.sh"]