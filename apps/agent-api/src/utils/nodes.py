import json
from datetime import datetime, timedelta, timezone
from langchain_openai import ChatOpenAI
from langgraph.errors import GraphInterrupt
from langchain_core.runnables import RunnableConfig
from langgraph.store.postgres import PostgresStore
from langchain_core.messages import BaseMessage, AIMessage, SystemMessage, HumanMessage
from langgraph.types import Command, interrupt
from utils.state import State, StructuredOutput
from utils.tools import tools 
from utils.store import store
from insights.cohorts import fetch_data, extract_event_features, cluster_visitors, classify_cohorts
from utils.prompts import CHATBOT_SYSTEM_PROMPT, CHECK_IN_SYSTEM_PROMPT, ONBOARDING_SYSTEM_PROMPT
from langgraph.graph import END

model = ChatOpenAI(
    model="gpt-4o-mini", temperature=0, streaming=True
)

model_with_tools = ChatOpenAI(
    model="gpt-4o-mini", temperature=0
).bind_tools(tools=tools, strict=True, parallel_tool_calls=True)

async def call_model(state: State, config: RunnableConfig):
    messages = state['messages']

    system_message = """
        You are an ecommerce marketing analyst.
        Your job is to identify insights from raw collected event data from the user's website.
        Respond to the user by telling them that you are putting together a report.
    """

    response = await model.ainvoke(
        [
            SystemMessage(system_message),
            *messages
        ]
    )
    return {"messages": [response]}

async def cohorts(state: State, config: RunnableConfig):
    """Classify visitor events, reconstruct journeys, and pass cohort summaries to an LLM for labeling."""

    # Fetch, process, and classify visitor sessions (not visitors)
    df = fetch_data()
    features = extract_event_features(df)  # ✅ Now session-based
    clustered_features = cluster_visitors(features)
    xbg_model, classified_data = classify_cohorts(clustered_features)

    # === Step 1: Aggregate Session Journeys per Cohort ===
    # ✅ No longer grouping by visitor_id, we now group by `session_id`
    classified_data["page_journey"] = classified_data.groupby("session_id")["path_<lambda>"].transform(lambda x: " → ".join(x))

    cohort_summary = classified_data.groupby("gmm_label").agg({
        "session_id": "count",  # ✅ Now counting sessions, not visitors
        "product_price_sum": "mean",
        "product_price_mean": "mean",
        "new_lead_sum": "sum",
        "session_duration": "mean",
        "created_at_count": "mean",
        "utm_campaign_sum": "mean",
        "utm_source_sum": "mean",
        "ad_click_sum": "mean",
        "page_journey": lambda x: x.value_counts().idxmax(),  # ✅ Most common journey in cohort
    }).reset_index()

    # Rename for clarity
    cohort_summary.rename(columns={"session_id": "num_sessions"}, inplace=True)

    # Convert to structured dict for LLM
    llm_input = cohort_summary.to_dict(orient="records")

    # === Step 2: Use LLM to Label Cohorts ===
    system_message = """
        You are an ecommerce marketing analyst.
        You will receive **aggregated cohort data**, summarizing session behaviors.

        **Each cohort summary includes:**
        - **Number of sessions** in the cohort.
        - **Average purchasing behavior**: Total and mean product price.
        - **Lead conversions**: How many sessions resulted in a lead.
        - **Average session duration** and event count (engagement).
        - **Marketing exposure**: UTM campaigns, ad clicks.
        - **Most common page journey** (reconstructed from session paths).

        **Your Goal:**
        - Assign a **clear, marketing-relevant cohort label** (e.g., "High Intent Buyers", "Cart Abandoners", "Window Shoppers").
        - Base the cohort label on their session-based behavior, page visits, and buying patterns.
        - Focus on **session behaviors**, not individual visitors.
    """

    # Send structured cohort summary to LLM
    response = await model.ainvoke(
        [
            SystemMessage(system_message),
            HumanMessage(content=json.dumps(llm_input, indent=2))  # Send formatted JSON
        ]
    )

    return {"messages": [response]}

