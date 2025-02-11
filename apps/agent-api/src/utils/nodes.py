import json
from uuid import uuid4
from datetime import datetime, timedelta, timezone
from langchain_openai import ChatOpenAI
from langgraph.errors import GraphInterrupt
from langchain_core.runnables import RunnableConfig
from langgraph.store.postgres import PostgresStore
from langchain_core.messages import BaseMessage, AIMessage, SystemMessage, HumanMessage
from langgraph.types import Command, interrupt
from utils.state import State, FunnelOutput
from utils.tools import tools
from utils.store import store
from insights.site_info import extract_site_info
from insights.cohorts import (
    fetch_data,
    extract_event_features,
    cluster_visitors,
    classify_cohorts,
)
from insights.funnels import get_journeys, get_top_paths, compute_dropoffs
from langgraph.graph import END

model = ChatOpenAI(model="gpt-4o-mini", temperature=0, streaming=True)
smart_model = ChatOpenAI(model="gpt-4o", temperature=0)

model_with_funnel_output = ChatOpenAI(
    model="gpt-4o-mini", temperature=0, streaming=True
).with_structured_output(FunnelOutput)

model_with_tools = ChatOpenAI(model="gpt-4o-mini", temperature=0).bind_tools(
    tools=tools, strict=True, parallel_tool_calls=True
)


async def call_model(state: State, config: RunnableConfig):
    messages = state["messages"]

    system_message = """
        You are an ecommerce marketing analyst.
        Your job is to identify insights from raw collected event data from the user's website.
        Respond to the user by telling them that you are putting together a report.
    """

    response = await model.ainvoke([SystemMessage(system_message), *messages])
    return {"messages": response}


async def site_info(state: State, config: RunnableConfig):
    messages = state["messages"]
    [site_map, landing_page] = extract_site_info(state["site_url"])

    system_message = """
        You are an ecommerce marketing analyst.
        You will receive a list of sitemap locations, as well as the HTML for a site landing page.

        **Each sitemap entry includes:**
        - **Location** URL of the page.
        - **Modified** timestamp.

        **Your Goal:**
        - Identify the marketing/ecommerce goals of the website.
            - Summary of the site.
            - What most likely constitutes a lead?
            - What most likely constitutes a conversion?
        - Identify prospective lead and conversion funnels.
    """

    response = await model.ainvoke(
        [
            SystemMessage(system_message),
            HumanMessage(content=json.dumps(site_map, indent=2)),
            HumanMessage(content=landing_page),
        ]
    )

    return {"site_summary": response}


async def cohorts(state: State, config: RunnableConfig):
    """Classify visitor events, reconstruct journeys, and pass cohort summaries to an LLM for labeling."""

    # Fetch, process, and classify visitor sessions (not visitors)
    df = fetch_data()
    features = extract_event_features(df)  # ✅ Now session-based
    clustered_features = cluster_visitors(features)
    xbg_model, classified_data = classify_cohorts(clustered_features)

    # === Step 1: Aggregate Session Journeys per Cohort ===
    # ✅ No longer grouping by visitor_id, we now group by `session_id`
    classified_data["page_journey"] = classified_data.groupby("session_id")[
        "path_<lambda>"
    ].transform(lambda x: " → ".join(x))
    session_paths = classified_data[["session_id", "page_journey"]].drop_duplicates()

    cohort_summary = (
        classified_data.groupby("gmm_label")
        .agg(
            {
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
            }
        )
        .reset_index()
    )

    # Rename for clarity
    cohort_summary.rename(columns={"session_id": "num_sessions"}, inplace=True)

    # Convert to structured dict for LLM
    llm_input = cohort_summary.to_dict(orient="records")

    # === Step 2: Use LLM to Label Cohorts ===
    system_message = f"""
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

        **Site Summary:**
        {state["site_summary"]}
    """

    # Send structured cohort summary to LLM
    response = await model.ainvoke(
        [
            SystemMessage(system_message),
            HumanMessage(
                content=json.dumps(llm_input, indent=2)
            ),  # Send formatted JSON
        ]
    )

    return {
        "messages": [response],
        "session_paths": session_paths.to_dict(orient="records"),
    }


async def funnels(state: State):
    """Analyze journeys and identify key funnels"""
    journeys = get_journeys()
    system_message = f"""
        You are a marketing funnel analyst.
        You will receive a list of potential customer journeys, including page visits and form submissions.

        Your Goals:
        - Identify meaningful **lead generation** or **conversion funnels**.
        - Group **similar journeys** into a single funnel whenever possible.
        - If a funnel has multiple variations, extract the **core path** and use **wildcards (`*`)** to generalize.
        - Explain what each funnel represents and why it matters.

        ### **Rules for Funnels**
        1. A funnel must contain at least **one form submission**.
        2. Funnels should **ignore minor variations** (e.g., users visiting `/about` before `/contact`).
        3. If multiple journeys follow the **same structure**, generalize them by using `*` as a wildcard.

        ### **How to Generalize Journeys**
        - **If users visit similar pages under the same section**, replace specific pages with `*`.
            - `/vacatures/project-developer` → `/vacatures/*`
            - `/services/web-design` and `/services/seo` → `/services/*`
        - **Ignore exploratory pages** (e.g., `/about`, `/faq`) if they don’t change intent.

        **Site Summary:**
        {state["site_summary"]}
    """

    response = await smart_model.with_structured_output(FunnelOutput).ainvoke(
        [
            SystemMessage(system_message),
            HumanMessage(
                content=json.dumps(journeys.to_dict(orient="records"), indent=2)
            ),
        ]
    )

    funnel_data = response.dict()

    return {
        # "lead_funnels": lead_funnel_analysis,
        # "conversion_funnels": conversion_funnel_analysis,
        "messages": [AIMessage(content=json.dumps(response.dict(), indent=2))]
    }


# async def funnels(state: State, config: RunnableConfig):
#     """Analyze visitor paths, validate conversion funnels with LLM, and detect bottlenecks."""
#     top_paths = get_top_paths(state["session_paths"])
#     funnel_analysis = compute_dropoffs(
#         top_paths.to_dict(orient="records"), state["session_paths"]
#     )
#     funnel_analysis = [{"id": str(uuid4()), **x} for x in funnel_analysis]

#     system_message = f"""
#         You are an ecommerce marketing analyst.
#         You will receive a list of potential funnel journeys.

#         **Each entry includes:**
#         - Funnel: The sequence of pages visited.
#         - Total Visitors: Number of visitors that followed this exact path.
#         - Step Dropoffs: Analysis of dropoffs between pages.

#         **Your Goals:**
#         - Identify whether these are potential **lead generation** or **conversion** funnels.
#         - Explain what each funnel is likely to be for.

#         **Site Summary:**
#         {state["site_summary"]}
#     """

#     response = await model_with_funnel_output.ainvoke(
#         [
#             SystemMessage(system_message),
#             HumanMessage(content=json.dumps(funnel_analysis, indent=2)),
#         ]
#     )

#     funnel_data = response.dict()
#     # lead_funnels = funnel_data.get("lead_funnels", [])
#     # conversion_funnels = funnel_data.get("conversion_funnels", [])

#     # lead_funnel_analysis = compute_dropoffs(lead_funnels, state["session_paths"])
#     # conversion_funnel_analysis = compute_dropoffs(conversion_funnels, state["session_paths"])

#     return {
#         # "lead_funnels": lead_funnel_analysis,
#         # "conversion_funnels": conversion_funnel_analysis,
#         "messages": [AIMessage(content=json.dumps(response.dict(), indent=2))]
#     }
