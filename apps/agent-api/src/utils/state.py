from typing import Annotated, List, Optional
from enum import Enum
from pydantic import BaseModel, Field
from typing_extensions import TypedDict
from langgraph.graph.message import add_messages

class Component(str, Enum):
    PROMPT_SUGGESTION = "prompt_suggestion"

# class ActionType(str, Enum):
#     GOAL = "goal"
#     TASK = "task"

# class ActionTimeScope(str, Enum):
#     TODAY = "today"
#     THIS_WEEK = "this_week"

class UIElement(BaseModel):
    component: Component
    label: str
    value: str

# class ActionElement(BaseModel):
#     type: ActionType
#     time_scope: ActionTimeScope
#     label: str

class StructuredOutput(BaseModel):
    """Respond to the user with this"""
    confirm_check_in: bool = Field(description="When given a [CHECK-IN CONFIRMATION] prompt, decide whether to proceed")
    check_in_message: str = Field(description="Your check-in message to the user, if checking in.")
    ui_elements: List[UIElement] = Field(description="List of UI elements to show the user to help them respond")
    action_plan: List[str] = Field(description="Generate an action plan, so the user can see it at-a-glance.")

class State(TypedDict):
    clerk_id: str
    timestamp: str
    user_tz_offset: int
    messages: Annotated[list, add_messages]
    # check_in_timestamp: str
    # check_in_reason: str
    final_response: StructuredOutput