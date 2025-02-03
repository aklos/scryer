from langgraph.graph import END, START, StateGraph
from utils.nodes import intro, onboarding, wait_for_response, respond, finished
from utils.state import State
from utils.tools import tools, ask_human
from langchain_core.tools import tool
from langgraph.prebuilt import ToolNode

tool_node = ToolNode(tools + [ask_human])

graph = StateGraph(State)

graph.add_node("intro", intro)
graph.add_node("agent", onboarding)
graph.add_node("action", tool_node)
graph.add_node("interrupt", wait_for_response)
graph.add_node("respond", respond)
graph.add_node("finished", finished)

def should_continue(state):
    messages = state["messages"]
    last_message = messages[-1]
    # If there is no function call, then we finish
    if not last_message.tool_calls:
        return "respond"
    # If tool call is asking Human, we return that node
    # You could also add logic here to let some system know that there's something that requires Human input
    # For example, send a slack message, etc
    elif len([x for x in last_message.tool_calls if x["name"] == "ask_human"]):
        return "interrupt"
    # Otherwise if there is, we continue
    else:
        return "action"

graph.add_edge(START, "intro")
graph.add_edge("intro", "agent")
graph.add_conditional_edges("agent", should_continue)
graph.add_edge("action", "agent")
graph.add_edge("interrupt", "agent")
graph.add_edge("respond", "finished")
graph.add_edge("finished", END)
