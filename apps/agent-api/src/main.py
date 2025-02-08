import os
from dotenv import load_dotenv
load_dotenv()

import uuid
import json
import asyncio
import httpx
from datetime import datetime
from IPython.display import Image, display
from fastapi import FastAPI, HTTPException, Request, Header
from fastapi.responses import StreamingResponse
from psycopg_pool import AsyncConnectionPool
from langgraph.checkpoint.postgres.aio import AsyncPostgresSaver
from langgraph.errors import GraphInterrupt
from langgraph.types import Command
from pydantic import BaseModel
from sse_starlette.sse import EventSourceResponse
from datetime import timedelta
from langchain_core.messages import AIMessage, HumanMessage, ToolMessage
from utils.redis import redis_client
from utils.store import store, pool
from utils.state import State
from graphs.default import graph as workflow

PG_CONN_STRING = os.getenv("DATABASE_URL")
SECRET_KEY = os.getenv("SECRET_KEY")
APP_URL = os.getenv("APP_URL")
KEEP_ALIVE_INTERVAL = 300

api = FastAPI()

async_pool = None
checkpointer = None
default_agent = None

async def keep_alive_task():
    while True:
        if async_pool:
            async with async_pool.connection() as conn:
                try:
                    await conn.execute("SELECT 1")  # Lightweight keep-alive ping
                except Exception as e:
                    print(f"Keep-alive ping failed: {e}")
        if pool:
            with pool.connection() as conn:
                try:
                    conn.execute("SELECT 1")  # Lightweight keep-alive ping
                except Exception as e:
                    print(f"Keep-alive ping failed: {e}")
        await asyncio.sleep(KEEP_ALIVE_INTERVAL)

@api.on_event("startup")
async def startup_event():
    global async_pool, checkpointer, default_agent
    async_pool = AsyncConnectionPool(conninfo=PG_CONN_STRING, max_size=20)
    await async_pool.open()
    checkpointer = AsyncPostgresSaver(async_pool)
    default_agent = workflow.compile(checkpointer=checkpointer, store=store)

    asyncio.create_task(keep_alive_task())

    # try:
    #     import utils.migrations
    #     tables_to_truncate = [
    #         "checkpoint_blobs",
    #         "checkpoint_writes",
    #         "checkpoint_migrations",
    #         "checkpoints",
    #         "store",
    #         "store_vectors",
    #         "store_migrations",
    #         "vector_migrations",
    #     ]
    #     async with async_pool.connection() as conn:
    #         async with conn.cursor() as cursor:
    #              # Disable foreign key constraints temporarily
    #             await cursor.execute("SET session_replication_role = 'replica';")

    #             # Truncate tables
    #             for table in tables_to_truncate:
    #                 await cursor.execute(f"TRUNCATE TABLE {table} RESTART IDENTITY CASCADE;")
                
    #             # Re-enable foreign key constraints
    #             await cursor.execute("SET session_replication_role = 'origin';")
    #         await conn.commit()
    # except Exception as exception:
    #     pass
    
@api.on_event("shutdown")
async def shutdown_event():
    await async_pool.close()

class ThreadRequest(BaseModel):
    clerk_id: str
    message: str

class CheckInRequest(BaseModel):
    clerk_id: str
    timestamp: str
    tz_offset: int
    check_in_id: str
    reason: str
    is_reminder: bool

class SystemRequest(BaseModel):
    clerk_id: str

async def verify_secret_key(secret_key: str):
    if secret_key != SECRET_KEY:
        raise HTTPException(status_code=401, detail="Invalid or missing secret key")

# @api.post("/run-thread")
# async def run_thread(request: ThreadRequest, x_secret_key: str = Header(None)):
#     await verify_secret_key(x_secret_key)
#     try:
#         thread_id = redis_client.get(request.clerk_id)
#         if not thread_id:
#             thread_id = str(uuid.uuid4())
#             redis_client.setex(request.clerk_id, timedelta(hours=6), thread_id)
#         else:
#             thread_id = thread_id.decode()

#         try:
#             state = await checkpointer.aload(thread_id)
#             state["timestamp"] = (datetime.utcnow()).isoformat()
#         except Exception:
#             state = State(
#                 messages=[],
#                 clerk_id=request.clerk_id,
#                 timestamp=(datetime.utcnow()).isoformat(),
#             )

#         state["messages"].append(HumanMessage(content=request.message))

#         result = await default_agent.ainvoke(state, config={"configurable": {"thread_id": thread_id}})

#         return {"response": result["messages"][-1]}
#     except Exception as e:
#         raise HTTPException(status_code=500, detail=str(e))

# @api.post("/run-thread-stream")
# async def run_thread_stream(request: ThreadRequest, x_secret_key: str = Header(None)):
#     await verify_secret_key(x_secret_key)
#     try:
#         thread_id = redis_client.get(request.clerk_id)
#         if not thread_id:
#             thread_id = str(uuid.uuid4())
#             redis_client.setex(request.clerk_id, timedelta(hours=6), thread_id)
#         else:
#             thread_id = thread_id.decode()

#         try:
#             state = await checkpointer.aload(thread_id)
#             state["timestamp"] = (datetime.utcnow()).isoformat()
#         except Exception:
#             state = State(
#                 messages=[],
#                 clerk_id=request.clerk_id,
#                 timestamp=(datetime.utcnow()).isoformat(),
#             )
        
#         updated_state = await default_agent.aget_state(config={"configurable": {"thread_id": thread_id}})

#         if len(updated_state.tasks):
#             state = Command(resume=request.message)
#         else:
#             state["messages"].append(HumanMessage(content=request.message))

#         async def event_generator():
#             async for msg, metadata in default_agent.astream(state, config={"configurable": {"thread_id": thread_id}}, stream_mode="messages", debug=True):
#                 if isinstance(msg, AIMessage) and len(msg.additional_kwargs.get("tool_calls") or []):
#                     for tool_call in msg.additional_kwargs.get("tool_calls"):
#                         if tool_call.get("id"):
#                             yield {"data": json.dumps({"tool_call": tool_call})}
#                 if metadata.get("langgraph_node") == "agent":
#                     yield {"data": json.dumps({"response": msg.content})}
#                 if metadata.get("langgraph_node") == "respond":
#                     yield {"data": json.dumps({"structured_response": msg.content})}
#                 if isinstance(msg, ToolMessage):
#                     yield {"data": json.dumps({"tool_response": { "content": msg.content, "tool_call_id": msg.tool_call_id }})}

#         return EventSourceResponse(event_generator())

#     except Exception as e:
#         raise HTTPException(status_code=500, detail=str(e))


@api.post("/run-report-stream")
async def run_thread_stream(request: ThreadRequest, x_secret_key: str = Header(None)):
    await verify_secret_key(x_secret_key)
    try:
        thread_id = str(uuid.uuid4())
        # thread_id = redis_client.get(request.clerk_id)
        # if not thread_id:
        #     thread_id = str(uuid.uuid4())
        #     redis_client.setex(request.clerk_id, timedelta(hours=6), thread_id)
        # else:
        #     thread_id = thread_id.decode()

        try:
            state = await checkpointer.aload(thread_id)
            state["timestamp"] = (datetime.utcnow()).isoformat()
        except Exception:
            state = State(
                messages=[],
                clerk_id=request.clerk_id,
                timestamp=(datetime.utcnow()).isoformat(),
            )
        
        updated_state = await default_agent.aget_state(config={"configurable": {"thread_id": thread_id}})

        if len(updated_state.tasks):
            state = Command(resume=request.message)
        else:
            state["messages"].append(HumanMessage(content=request.message))

        async def event_generator():
            async for msg, metadata in default_agent.astream(state, config={"configurable": {"thread_id": thread_id}}, stream_mode="messages", debug=True):
                if isinstance(msg, AIMessage) and len(msg.additional_kwargs.get("tool_calls") or []):
                    for tool_call in msg.additional_kwargs.get("tool_calls"):
                        if tool_call.get("id"):
                            yield {"data": json.dumps({"tool_call": tool_call})}
                if isinstance(msg, AIMessage):
                    yield {"data": json.dumps({"response": msg.content})}
                # if metadata.get("langgraph_node") == "respond":
                #     yield {"data": json.dumps({"structured_response": msg.content})}
                if isinstance(msg, ToolMessage):
                    yield {"data": json.dumps({"tool_response": { "content": msg.content, "tool_call_id": msg.tool_call_id }})}

        return EventSourceResponse(event_generator())

    except Exception as e:
        print(e)
        raise HTTPException(status_code=500, detail=str(e))


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(api, host="0.0.0.0", port=8000)