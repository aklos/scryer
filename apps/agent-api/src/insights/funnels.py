import pandas as pd
from typing import List
from utils.store import pool  # PG ConnectionPool


def get_top_paths(session_paths: List[dict]):
    session_paths = pd.DataFrame(session_paths)  # Convert back to DataFrame

    # Aggregate visitor paths
    path_counts = session_paths["page_journey"].value_counts().reset_index()
    path_counts.columns = ["visitor_path", "count"]

    # Keep top 10 most frequent visitor paths
    return path_counts.head(10)


def get_journeys():
    query = """
    WITH ranked_events AS (
        SELECT 
            e.visitor_id,
            e.created_at,
            e.data->>'event' AS data_event,
            e.data->>'path' AS data_path,
            COALESCE(e.data->'formData' ? 'email', FALSE) AS has_email,
            e.data->'formData'->>'formName' AS data_form_name,
            ROW_NUMBER() OVER (
                PARTITION BY e.visitor_id, e.data->>'event', e.data->>'path', DATE_TRUNC('second', e.created_at)
                ORDER BY e.created_at
            ) AS rn
        FROM events e
        WHERE e.data->>'event' IN ('page_visit', 'form_submission')
    ),
    session_markers AS (
        SELECT 
            re.*,
            LAG(re.created_at) OVER (
                PARTITION BY re.visitor_id ORDER BY re.created_at
            ) AS prev_event_time,
            LAG(re.data_event) OVER (
                PARTITION BY re.visitor_id ORDER BY re.created_at
            ) AS prev_event
        FROM ranked_events re
        WHERE rn = 1
    ),
    sessions AS (
        SELECT 
            *,
            SUM(
                CASE 
                    WHEN prev_event_time IS NULL THEN 1  -- First event starts a new session
                    WHEN data_event = 'page_visit' AND created_at - prev_event_time > INTERVAL '30 minutes' THEN 1
                    ELSE 0 
                END
            ) OVER (PARTITION BY visitor_id ORDER BY created_at) AS session_id
        FROM session_markers
    ),
    session_journeys AS (
        SELECT 
            visitor_id,
            session_id,
            ARRAY_AGG(
                CASE 
                    WHEN data_event = 'form_submission' THEN 
                        'form_submission:' || COALESCE(data_form_name, '[unknown_form]')
                    ELSE 
                        data_event || ':' || data_path
                END
                ORDER BY created_at
            ) AS journey
        FROM sessions
        GROUP BY visitor_id, session_id
    ),
    journey_counts AS (
        SELECT 
            journey,
            COUNT(*) AS journey_count
        FROM session_journeys
        GROUP BY journey
        ORDER BY journey_count DESC
    )
    SELECT journey, journey_count
    FROM journey_counts
    WHERE ARRAY_TO_STRING(journey, ' -> ') LIKE '%form_submission:%';
    """
    with pool.getconn() as conn:
        df = pd.read_sql(query, conn)
        pool.putconn(conn)
    return df


def compute_dropoffs(funnels, session_paths: List[dict]):
    """Calculate drop-off rates for each funnel step."""
    session_df = pd.DataFrame(session_paths)
    dropoff_analysis = []

    for funnel in funnels:
        path = funnel["visitor_path"]
        if isinstance(path, str):
            path = [p.strip() for p in path.split("→")]
        total_visitors = funnel["count"]

        if len(path) < 2:
            continue

        step_dropoffs = []
        for i in range(len(path) - 1):
            from_page = path[i]
            to_page = path[i + 1]

            # Count sessions that reached 'from_page' but not 'to_page'
            step_visitors = session_df[
                session_df["page_journey"].str.contains(from_page, na=False)
            ]
            next_step_visitors = session_df[
                session_df["page_journey"].str.contains(to_page, na=False)
            ]

            visitors_lost = len(step_visitors) - len(next_step_visitors)
            dropoff_rate = visitors_lost / max(
                len(step_visitors), 1
            )  # Avoid division by zero

            step_dropoffs.append(
                {
                    "from_page": from_page,
                    "to_page": to_page,
                    "dropoff_rate": round(
                        dropoff_rate * 100, 2
                    ),  # Convert to percentage
                }
            )

        dropoff_analysis.append(
            {
                "funnel": path,
                "total_visitors": total_visitors,
                "step_dropoffs": step_dropoffs,
            }
        )

    return dropoff_analysis
