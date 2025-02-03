FROM flyio/postgres-flex:15.2

# Install build dependencies
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        build-essential \
        curl \
        postgresql-server-dev-all \
        postgresql-15-pgvector

# Clean up build dependencies and temporary files
RUN apt-get remove -y build-essential curl postgresql-server-dev-all && \
    apt-get autoremove -y && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/* && \
    rm -rf /pgvector.tar.gz /pgvector-${PGVECTOR_VERSION}

# FIXME: Might resolve idle disconnect issue:
# RUN echo "max_connections = 100" >> /var/lib/postgresql/data/postgresql.conf && \
#     echo "idle_in_transaction_session_timeout = 0" >> /var/lib/postgresql/data/postgresql.conf && \
#     echo "tcp_keepalives_idle = 60" >> /var/lib/postgresql/data/postgresql.conf && \
#     echo "tcp_keepalives_interval = 10" >> /var/lib/postgresql/data/postgresql.conf