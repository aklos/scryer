import os
from langgraph.store.postgres import PostgresStore
from psycopg_pool import ConnectionPool
from langchain.embeddings import init_embeddings

PG_CONN_STRING = os.getenv("DATABASE_URL")

pool = ConnectionPool(conninfo=PG_CONN_STRING, max_size=20)

store = PostgresStore(
    conn=pool,
    index={
        "dims": 1536,
        "embed": init_embeddings("openai:text-embedding-3-small"),
        "fields": ["text"]
    }
)