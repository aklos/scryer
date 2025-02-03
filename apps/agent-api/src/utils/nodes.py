import json
from datetime import datetime, timedelta, timezone
from langchain_openai import ChatOpenAI
from langgraph.errors import GraphInterrupt
from langchain_core.runnables import RunnableConfig
from langgraph.store.postgres import PostgresStore
from langchain_core.messages import BaseMessage, AIMessage, SystemMessage, HumanMessage
from langgraph.types import Command, interrupt
from utils.state import State, StructuredOutput
from utils.tools import tools, ask_human
from utils.store import store
from utils.prompts import CHATBOT_SYSTEM_PROMPT, CHECK_IN_SYSTEM_PROMPT, ONBOARDING_SYSTEM_PROMPT
from langgraph.graph import END

model = ChatOpenAI(
    model="gpt-4o-mini", temperature=0.7, streaming=True
)

model_with_tools = ChatOpenAI(
    model="gpt-4o-mini", temperature=0
).bind_tools(tools=tools, strict=True, parallel_tool_calls=True)

model_with_tools_seq = ChatOpenAI(
    model="gpt-4o-mini", temperature=0
).bind_tools(tools=tools + [ask_human], strict=True, parallel_tool_calls=False)

model_with_structured_output = ChatOpenAI(
    model="gpt-4o-mini", temperature=0.7
).with_structured_output(StructuredOutput)

def get_local_time(tz_offset: int):
    now_utc = datetime.utcnow()
    offset_timedelta = timedelta(minutes=-tz_offset)
    user_timezone = timezone(offset_timedelta)
    user_time = now_utc.replace(tzinfo=timezone.utc).astimezone(user_timezone)
    return user_time.isoformat()

async def chatbot(state: State, config: RunnableConfig):
    messages = state['messages']

    memories = store.get((state["clerk_id"],), "summary")
    base_namespace = (state["clerk_id"],)
    topic_namespace = (state["clerk_id"], "topic",)
    core_memories = store.get(base_namespace, "core_memory")
    tasks = store.get(base_namespace, "tasks")
    topics = store.search(topic_namespace, limit=100)
    check_ins = store.get(base_namespace, "check_ins")
    relevant_topics = store.search(
        topic_namespace, 
        query=messages[-1].content, 
        limit=1
    )
    relevant_topic = relevant_topics[0] if len(relevant_topics) else None

    system_message = CHATBOT_SYSTEM_PROMPT.format(
        current_time=state["timestamp"],
        user_time=get_local_time(state["user_tz_offset"]),
        core_memories=core_memories.value["data"] if core_memories else [],
        known_topics=[{"id": x.key, "title": x.value.get("title")} for x in topics],
        relevant_topic=relevant_topic.value if relevant_topic else {}, 
        check_ins=check_ins.value["entries"] if check_ins else [],
        tasks=tasks.value["data"] if tasks else []
    )

    response = await model_with_tools.ainvoke(
        [
            SystemMessage(system_message),
            *messages
        ]
    )
    return {"messages": [response]}

async def respond(state: State):
    messages = state['messages']

    memories = store.get((state["clerk_id"],), "summary")
    base_namespace = (state["clerk_id"],)
    topic_namespace = (state["clerk_id"], "topic",)
    core_memories = store.get(base_namespace, "core_memory")
    tasks = store.get(base_namespace, "tasks")
    topics = store.search(topic_namespace, limit=100)
    check_ins = store.get(base_namespace, "check_ins")
    relevant_topics = store.search(
        topic_namespace, 
        query=messages[-1].content, 
        limit=1
    )
    relevant_topic = relevant_topics[0] if len(relevant_topics) else None

    system_message = """
        Read the last few messages and decide if any UI elements or action plan should be displayed to the user.

        Rules:
            - Provide up to 2 prompt suggestions from the perspective of the user to quickly reply to questions.
                - Prompts must be something *the user can say*.
                - Max length of prompt is 36 characters.
                - Don't give prompt suggestions unless last message is a direct question with limited number of possible answers.
            - Provide "at a glance" action plan summary of what the user should know and prepare to do for today.
        
        Example 1:
            Last AI Message: "How can I assist you today? Do you have any tasks or goals you want to discuss?"
            UI elements:
              - Suggestion: "What are my current tasks for today?"
              - Suggestion: "I'd like to add a new task"
        
        Context: ###
            Current system time: "{current_time}"
            User's current time (with timezone): "{user_time}"
            Core memories: {core_memories}
            Known topics: {known_topics}
            Relevant topic context: {relevant_topic}
            Scheduled check-ins: {check_ins}
            Tasks and goals: {tasks}
        ###
    """.format(
        current_time=state["timestamp"],
        user_time=get_local_time(state["user_tz_offset"]),
        core_memories=core_memories.value["data"] if core_memories else [],
        known_topics=[{"id": x.key, "title": x.value.get("title")} for x in topics],
        relevant_topic=relevant_topic.value if relevant_topic else {}, 
        check_ins=check_ins.value["entries"] if check_ins else [],
        tasks=tasks.value["data"] if tasks else []
    )
    response = model_with_structured_output.invoke(
        [
            SystemMessage(system_message),
            *messages
        ]
    )
    return {"final_response": response}

async def check_in(state: State):
    messages = state['messages']

    memories = store.get((state["clerk_id"],), "summary")
    base_namespace = (state["clerk_id"],)
    topic_namespace = (state["clerk_id"], "topic",)
    core_memories = store.get(base_namespace, "core_memory")
    tasks = store.get(base_namespace, "tasks")
    topics = store.search(topic_namespace, limit=100)
    check_ins = store.get(base_namespace, "check_ins")
    relevant_topics = store.search(
        topic_namespace, 
        query=messages[-1].content, 
        limit=1
    )
    relevant_topic = relevant_topics[0] if len(relevant_topics) else None

    system_message = CHECK_IN_SYSTEM_PROMPT.format(
        current_time=state["timestamp"],
        user_time=get_local_time(state["user_tz_offset"]),
        core_memories=core_memories.value["data"] if core_memories else [],
        known_topics=[{"id": x.key, "title": x.value.get("title")} for x in topics],
        relevant_topic=relevant_topic.value if relevant_topic else {}, 
        check_ins=check_ins.value["entries"] if check_ins else [],
        tasks=tasks.value["data"] if tasks else []
    )

    response = await model_with_structured_output.ainvoke(
        [
            SystemMessage(system_message),
            *messages
        ]
    )

    return {"final_response": response}

async def wait_for_response(state: State):
    ask_human_calls = [x for x in state["messages"][-1].tool_calls if x["name"] == "ask_human"]
    tool_call_id = ask_human_calls[0]["id"]
    response = interrupt(ask_human_calls[0]["args"]["question"])
    tool_message = [{"tool_call_id": tool_call_id, "type": "tool", "content": response}]
    return {"messages": tool_message}

async def intro(state: State):
    messages = state["messages"]

    system_message = """
        - Welcome the user to Touchbase
        - Succinctly explain to them the following:
            - You remember everything important!
            - You are a taskmaster built to simplify and organize their responsibilities and goals.
            - You can help them achieve goals by actively participating in organizing, planning, managing, and communicating.
            - You are capable of initiating conversations with the user when appropriate, for reminders and nudges. (feature currently only available on Telegram)
        - Finally, explain that you have a couple of questions to get started.
    """
    # - You have three prime directives: 
    # 1) Be an agent the user can emotionally resonate with. 
    # 2) Implicitly track tasks and goals. 
    # 3) Proactive approach to communication.

    response = await model.ainvoke([
        SystemMessage(system_message),
    ])
    
    return {"messages": [response]}

async def onboarding(state: State):
    messages = state["messages"]

    memories = store.get((state["clerk_id"],), "summary")
    base_namespace = (state["clerk_id"],)
    topic_namespace = (state["clerk_id"], "topic",)
    core_memories = store.get(base_namespace, "core_memory")
    tasks = store.get(base_namespace, "tasks")
    topics = store.search(topic_namespace, limit=100)
    check_ins = store.get(base_namespace, "check_ins")
    relevant_topics = store.search(
        topic_namespace, 
        query=messages[-1].content, 
        limit=1
    )
    relevant_topic = relevant_topics[0] if len(relevant_topics) else None

    system_message = ONBOARDING_SYSTEM_PROMPT.format(
        current_time=state["timestamp"],
        user_time=get_local_time(state["user_tz_offset"]),
        core_memories=core_memories.value["data"] if core_memories else [],
        known_topics=[{"id": x.key, "title": x.value.get("title")} for x in topics],
        relevant_topic=relevant_topic.value if relevant_topic else {}, 
        check_ins=check_ins.value["entries"] if check_ins else [],
        tasks=tasks.value["data"] if tasks else []
    )

    response = await model_with_tools_seq.ainvoke([ 
        SystemMessage(system_message),
        *messages,
    ])
    
    return {"messages": [response]}

async def finished(state: State):
    return {"messages": state["messages"]}