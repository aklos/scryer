from typing import Annotated, List, Optional
from enum import Enum
from pydantic import BaseModel, Field
from typing_extensions import TypedDict
from langgraph.graph.message import add_messages

class FunnelType(str, Enum):
    LEAD_FUNNEL = "lead_funnel"
    CONVERSION_FUNNEL = "conversion_funnel"
    NOT_APPLICABLE = "not_applicable"

class Funnel(BaseModel):
    id: str
    type: FunnelType
    page_journey: List[str]
    label: str
    rationale: str


class FunnelOutput(BaseModel):
    """Respond to the user with this."""
    funnels: List[Funnel] = Field(description="Explanations of each funnel")

class State(TypedDict):
    clerk_id: str
    timestamp: str
    site_url: str
    messages: Annotated[list, add_messages]
    site_summary: str
    session_paths: Optional[List[dict]]
    lead_funnels: Optional[List[dict]]
    conversion_funnels: Optional[List[dict]]