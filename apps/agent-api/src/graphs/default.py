from langgraph.graph import END, START, StateGraph
from utils.nodes import call_model, site_info, cohorts, funnels
from utils.state import State
from utils.tools import tools
from langgraph.prebuilt import ToolNode

# tool_node = ToolNode([])

graph = StateGraph(State)

graph.add_node("agent", call_model)
graph.add_node("site_info", site_info)
graph.add_node("cohorts", cohorts)
graph.add_node("funnels", funnels)

# graph.add_node("tools", tool_node)

# async def should_continue(state: State):
#     messages = state["messages"]
#     last_message = messages[-1]
#     if last_message.tool_calls:
#         return "tools"
#     return END

graph.add_edge(START, "agent")
graph.add_edge("agent", "site_info")
graph.add_edge("site_info", "cohorts")
graph.add_edge("cohorts", "funnels")
graph.add_edge("funnels", END)

# graph.add_conditional_edges("agent", should_continue)
# graph.add_edge("tools", "agent")