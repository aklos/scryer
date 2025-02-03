from langgraph.graph import END, START, StateGraph
from utils.nodes import chatbot, respond
from utils.state import State
from utils.tools import tools
from langgraph.prebuilt import ToolNode

tool_node = ToolNode(tools)

graph = StateGraph(State)

graph.add_node("agent", chatbot)
graph.add_node("tools", tool_node)
# graph.add_node("chatbot", chatbot)
graph.add_node("respond", respond)

async def should_continue(state: State):
    messages = state["messages"]
    last_message = messages[-1]
    if last_message.tool_calls:
        return "tools"
    return "respond"

graph.add_edge(START, "agent")
graph.add_conditional_edges("agent", should_continue)
graph.add_edge("tools", "agent")
# graph.add_edge("agent", "respond")
graph.add_edge("respond", END)