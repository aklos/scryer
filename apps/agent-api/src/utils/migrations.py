import os
import psycopg
from langgraph.store.postgres import PostgresStore
from langgraph.checkpoint.postgres import PostgresSaver
from langchain.embeddings import init_embeddings

PG_CONN_STRING = os.getenv("DATABASE_URL")

with psycopg.connect(PG_CONN_STRING, autocommit=True) as conn:
    store = PostgresStore(
        conn=conn,
        index={
            "dims": 1536,
            "embed": init_embeddings("openai:text-embedding-3-small"),
            "fields": ["text"]  # specify which fields to embed. Default is the whole serialized value
        }
    )
    store.setup()
    checkpointer = PostgresSaver(conn)
    checkpointer.setup()  # Run setup outside of a transaction block