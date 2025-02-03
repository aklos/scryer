from typing import Annotated

import uuid
import json
import hashlib
import re
from datetime import datetime, timedelta
from openai import OpenAI
from pydantic import BaseModel
from enum import Enum
from typing import List, Optional
from langchain_openai import ChatOpenAI
from langchain_core.tools import tool, InjectedToolArg
from langgraph.prebuilt import InjectedState, InjectedStore
from utils.state import State
from utils.tasks import schedule_task, cancel_task
from langgraph.store.postgres import PostgresStore

class MemoryClass(Enum):
    CORE: "core"
    TOPIC: "topic"

class Operation(Enum):
    CREATE: "create"
    UPDATE: "update"
    DELETE: "delete"

class Memory(BaseModel):
    id: str
    content: str
    system_timestamp: str


class Task(BaseModel):
    id: str
    content: str
    status: str
    created_at: str
    due_at: str

@tool
async def ask_human(
    question: str
):
    """Ask the human a question before continuing."""
    return question

@tool
async def manage_memory(
    memory_class: str,
    operation: str,
    content: Optional[str],
    topic_id: Optional[str],
    memory_id: Optional[str],
    state: Annotated[dict, InjectedState],
    store: Annotated[PostgresStore, InjectedStore]
):
    """Manage memories about the user.

    ***Important: you cannot create topics with this tool.***

    Args:
        memory_class: the memory class to manage. Options: "core", "topic".
        operation: the action to perform. Options: "create", "update", "delete".
        content: memory content for creation or update.
        topic_id: the UUID of an existing topic for managing its memory.
        memory_id: ID of the memory to update or delete
    """
    namespace = (state["clerk_id"],) if memory_class != "topic" else (state["clerk_id"], "topic",)
    memory_id = memory_id if memory_id else str(uuid.uuid4())
    topic_id = topic_id if topic_id else str(uuid.uuid4())
    timestamp = datetime.utcnow().isoformat()

    if memory_class == "core":
        key = "core_memory"
    elif memory_class == "topic":
        key = topic_id
    
    store_entry = store.get(namespace, key)
    store_value = store_entry.value if store_entry else {"data": []}

    if operation == "create":
        store_value["data"].append(
            Memory(
                id=memory_id,
                content=content,
                system_timestamp=timestamp,
            ).dict()
        )
    elif operation == "update":
        memory_index = next((i for i, d in enumerate(store_value["data"]) if d["id"] == memory_id), -1)
        if memory_index != -1:
            if content:
                store_value["data"][memory_index]["content"] = content
        else:
            return "memory not found"
    elif operation == "delete":
        store_value["data"] = [x for x in store_value["data"] if x["id"] != memory_id]

    store.put(namespace, key, store_value)

    return "managed memory"


@tool
async def manage_tasks_or_goals(
    operation: str,
    due_at: Optional[str],
    status: Optional[str],
    content: Optional[str],
    entry_id: Optional[str],
    state: Annotated[dict, InjectedState],
    store: Annotated[PostgresStore, InjectedStore]
):
    """Manage tasks or goals for the user.

    "due_at" is *required* when creating a task/goal. If there's no reasonable due/expiry date, defaults to 7 days ahead.

    Args:
        operation: the action to perform. Options: "create", "update", "delete".
        content: task/goal definition for creation or update.
        entry_id: ID of the task/goal to update or delete.
        status: task status. Options: "todo", "doing", "finished".
        due_at: UTC timestamp in ISO format (e.g., "2025-01-20T14:30:00Z"). Required for creating tasks/goals.
    """
    namespace = (state["clerk_id"],)
    entry_id = entry_id if entry_id else str(uuid.uuid4())
    timestamp = datetime.utcnow().isoformat()

    key = "tasks"
    
    store_entry = store.get(namespace, key)
    store_value = store_entry.value if store_entry else {"data": []}

    if operation == "create":
        if due_at is None:
            due_at = (datetime.utcnow() + timedelta(days=7)).isoformat()

        store_value["data"].append(
            Task(
                id=entry_id,
                content=content,
                status=status,
                created_at=timestamp,
                due_at=due_at
            ).dict()
        )
    elif operation == "update":
        task_index = next((i for i, d in enumerate(store_value["data"]) if d["id"] == entry_id), -1)
        if task_index != -1:
            if content:
                store_value["data"][task_index]["content"] = content
            if status:
                store_value["data"][task_index]["status"] = status
            if due_at:
                store_value["data"][task_index]["due_at"] = due_at
        else:
            return "task not found"
    elif operation == "delete":
        store_value["data"] = [x for x in store_value["data"] if x["id"] != entry_id]

    store.put(namespace, key, store_value)

    return "managed task"
    
    
@tool
async def manage_topic(
    operation: str,
    topic_title: str,
    topic_id: Optional[str],
    state: Annotated[dict, InjectedState],
    store: Annotated[PostgresStore, InjectedStore]
):
    """Manage topics about the user.

    Topics must be created before adding memories to them.

    Args:
        operation: the action to perform. Options: "create", "update", "delete".
        topic_id: the UUID of an existing topic for updating or deletion.
        topic_title: title for the topic.
    """
    namespace = (state["clerk_id"], "topic",)
    topic_id = topic_id if topic_id else str(uuid.uuid4())
    key = topic_id 

    store_entry = store.get(namespace, key)
    store_value = store_entry.value if store_entry else {"data": [], "title": None}

    if operation == "create":
        store_value["title"] = topic_title
        store.put(namespace, key, store_value)
    elif operation == "update":
        if topic_title:
            store_value["title"] = topic_title
        store.put(namespace, key, store_value)
    elif operation == "delete":
        store.delete(namespace, key)

    return "managed topic"

@tool
async def schedule_check_in(
    specified_time: str,
    reason: str,
    is_reminder: bool,
    state: Annotated[dict, InjectedState],
    store: Annotated[PostgresStore, InjectedStore]
):
    """Schedule a check-in with the user in the future.
    The check-in will be confirmed when it's time to do it.
    
    Args:
        specified_time: UTC timestamp in ISO format (e.g., "2025-01-20T14:30:00Z"). Max 1 week from now.
        reason: explanation for why you want to do this, and what you aim to accomplish.
        is_reminder: check-ins could be proactive or explicit reminders.
    """
    check_in_id = str(uuid.uuid4())

    schedule_task(
        specified_time,
        state["clerk_id"],
        check_in_id
    )

    check_ins = store.get((state["clerk_id"],), "check_ins")
    if not check_ins:
        store.put(
            (state["clerk_id"],),
            "check_ins",
            {"entries": [{"id": check_in_id, "specified_time": specified_time, "tz_offset": state["user_tz_offset"], "reason": reason, "is_reminder": is_reminder}]}
        )
    else:
        check_ins.value["entries"].append({
            "id": check_in_id,
            "specified_time": specified_time,
            "tz_offset": state["user_tz_offset"],
            "reason": reason,
            "is_reminder": is_reminder
        })
        store.put((state["clerk_id"],), "check_ins", check_ins.value)
    return "scheduled check-in"

@tool
async def cancel_check_in(
    check_in_id: str,
    state: Annotated[dict, InjectedState],
    store: Annotated[PostgresStore, InjectedStore]
):
    """Remove a scheduled check-in.
    
    Args:
        check_in_id: the check-in entry ID
    """

    cancel_task(check_in_id)

    check_ins = store.get((state["clerk_id"],), "check_ins")
    check_ins.value["entries"] = [x for x in check_ins.value["entries"] if x["id"] != check_in_id]
    store.put((state["clerk_id"],), "check_ins", check_ins.value)

    return "cancelled check-in"


tools = [manage_topic, manage_memory, manage_tasks_or_goals, schedule_check_in, cancel_check_in]