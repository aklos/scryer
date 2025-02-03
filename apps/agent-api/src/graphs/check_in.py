from langgraph.graph import END, START, StateGraph
from utils.nodes import check_in
from utils.state import State

graph = StateGraph(State)

graph.add_node("agent", check_in)

graph.add_edge(START, "agent")
graph.add_edge("agent", END)